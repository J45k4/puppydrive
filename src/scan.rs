use std::collections::HashMap;
use std::fs::canonicalize;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::path::Path;
use std::path::PathBuf;
use chrono::Utc;
use rusqlite::Connection;
use rusqlite::ToSql;
use walkdir::WalkDir;
#[cfg(feature = "rayon")]
use rayon::prelude::*;

use crate::db::FileLocation;

#[cfg(feature = "ring")]
fn sha256_hash<R: Read>(mut reader: R) -> io::Result<[u8; 32]> {
	let mut context = ring::digest::Context::new(&ring::digest::SHA256);
	let mut buffer = [0u8; 4096];
	loop {
		let count = reader.read(&mut buffer)?;
		if count == 0 { break; }
		context.update(&buffer[..count]);
	}
	// Finalize the hash and copy it into a fixed-size array.
	let digest: ring::digest::Digest = context.finish();
	let mut hash = [0u8; 32];
	hash.copy_from_slice(digest.as_ref());
	Ok(hash)
}

#[cfg(all(not(feature = "ring"), feature = "sha2"))]
fn sha256_hash<R: Read>(mut reader: R) -> io::Result<[u8; 32]> {
	use sha2::Digest;
	let mut hasher = sha2::Sha256::new();
	let mut buffer = [0u8; 4096];	
	loop {
		let count = reader.read(&mut buffer)?;
		if count == 0 { break; }
		hasher.update(&buffer[..count]);
	}
	Ok(hasher.finalize().into())
}

fn to_datetime(m: std::io::Result<std::time::SystemTime>) -> Option<chrono::DateTime<chrono::Utc>> {
	m.ok().map(|t| chrono::DateTime::from(t))
}

fn handle_path<P: AsRef<Path>>(path: P) -> FileLocation {
	let full_path = canonicalize(path.as_ref()).unwrap();
	log::info!("processing {}", full_path.display());
	let mut file = std::fs::File::open(path).unwrap();
	let m = file.metadata().unwrap();
	let created_at = to_datetime(m.created());
	let modified_at = to_datetime(m.modified());
	let accessed_at = to_datetime(m.accessed());
	let mut buffer = [0u8; 1024];
	let mime_type = match file.read(&mut buffer) {
		Ok(count) => match infer::get(&buffer[..count]).map(|mime| mime.to_string()) {
			Some(mime) => Some(mime),
			None => mime_guess::from_path(&full_path).first().map(|m| m.to_string())
		},
		Err(_) => None
	};
	file.seek(std::io::SeekFrom::Start(0)).unwrap();
	let hash = sha256_hash(file).unwrap();
	FileLocation {
		path: full_path,
		hash: Some(hash),
		size: m.len(),
		mime_type,
		timestamp: Utc::now(),
		created_at,
		modified_at,
		accessed_at
	}
}

const INSERT_FILE_LOCATION: &str = "INSERT INTO file_locations (node_id, path, hash, size, timestamp, created_at, modified_at, accessed_at) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";
const UPDATE_FILE_LOCATION: &str = "UPDATE file_locations SET hash = ?, size = ?, timestamp = ?, created_at = ?, modified_at = ?, accessed_at = ? WHERE node_id = ? and path = ?";
const DELETE_FILE_LOCATION: &str = "DELETE FROM file_locations WHERE node_id = ? and path = ?";
const UPSERT_FILE_ENTRY: &str = "INSERT INTO file_entries (hash, size, mime_type, first_datetime, latest_datetime) VALUES (?, ?, ?, ?, ?) ON CONFLICT(hash) DO UPDATE SET latest_datetime = excluded.latest_datetime";


pub struct ScanResult {
	pub updated_count: u64,
	pub inserted_count: u64,
	pub removed_count: u64,
	pub duration: std::time::Duration
}

pub fn scan<P: AsRef<Path>>(node_id: u128, path: P, mut conn: Connection) -> Result<ScanResult, String> {
	let timer = std::time::Instant::now();
	let mut updated_count = 0;
	let mut inserted_count = 0;
	let mut removed_count = 0;
	let path = path.as_ref().to_path_buf();
	let absolute_path = canonicalize(&path).unwrap();
	let tx = conn.transaction().unwrap();

	{
		// load all existing file_locations into a map
		let mut file_locations_stmt = tx.prepare(
			"SELECT path, hash, size, timestamp, created_at, modified_at, accessed_at \
			FROM file_locations \
			WHERE path LIKE ?"
		).map_err(|e| format!("error preparing statement: {:?}", e))?;
		let existing: HashMap<PathBuf, FileLocation> = file_locations_stmt
			.query_map([&(absolute_path.to_string_lossy().to_string() + "%")], |row| {
				Ok(FileLocation {
					path: PathBuf::from(row.get::<_, String>(0)?),
					hash: row.get(1)?,
					size: row.get(2)?,
					mime_type: None,          // we don’t need mime here
					timestamp: row.get(3)?,
					created_at: row.get(4)?,
					modified_at: row.get(5)?,
					accessed_at: row.get(6)?,
				})
			})
			.map_err(|e| format!("error querying file locations: {:?}", e))?
			.filter_map(Result::ok)
			.map(|fl| (fl.path.clone(), fl))
			.collect();

		// scan disk
		let entries = WalkDir::new(&absolute_path)
			.into_iter()
			.filter_map(|e| e.ok())
			.filter(|e| e.file_type().is_file())
			.collect::<Vec<_>>();

		#[cfg(feature = "rayon")]
		let mapped = entries.par_iter().map(|entry| (entry.path().to_path_buf(), entry.clone()));
		#[cfg(not(feature = "rayon"))]
		let mapped = entries.iter().map(|entry| (entry.path().to_path_buf(), entry.clone()));

		let mut scanned: HashMap<PathBuf, FileLocation> = mapped
			.map(|(pbuf, entry)| {
				// 1) quick metadata check
				let meta = std::fs::metadata(&pbuf).unwrap();
				let created_at  = to_datetime(meta.created());
				let modified_at = to_datetime(meta.modified());
				let accessed_at = to_datetime(meta.accessed());
				let size = meta.len();

				if let Some(prev) = existing.get(&pbuf) {
					if prev.size == size
						&& prev.created_at == created_at
						&& prev.modified_at == modified_at
						&& prev.accessed_at == accessed_at
					{
						// unchanged → reuse previous hash & mime; only update timestamp
						return (pbuf.clone(), FileLocation {
							path: pbuf.clone(),
							hash: prev.hash,
							size,
							mime_type: prev.mime_type.clone(),
							timestamp: Utc::now(),
							created_at,
							modified_at,
							accessed_at,
						});
					}
				}

				// metadata changed (or new file) → do full read+hash
				let fl = handle_path(&pbuf);
				(pbuf.clone(), fl)
			})
			.collect();

		// remove deleted files
		let mut delete_stmt = tx.prepare(DELETE_FILE_LOCATION).unwrap();
		for old in existing.keys() {
			if !scanned.contains_key(old) {
				delete_stmt.execute(&[
					&node_id.to_le_bytes() as &dyn ToSql,
					&old.to_string_lossy() as &dyn ToSql,
				]).unwrap();
				removed_count += 1;
			}
		}

		// insert or update each scanned file
		let mut insert_stmt = tx.prepare(INSERT_FILE_LOCATION).unwrap();
		let mut update_stmt = tx.prepare(UPDATE_FILE_LOCATION).unwrap();
		for (path, fl) in scanned.iter() {
			if let Some(prev) = existing.get(path) {
				if fl == prev {
					// completely identical (including timestamp) → skip
					continue;
				}
				// else: update hash/size/timestamps
				update_stmt.execute(&[
					&fl.hash as &dyn ToSql,
					&fl.size as &dyn ToSql,
					&fl.timestamp as &dyn ToSql,
					&fl.created_at as &dyn ToSql,
					&fl.modified_at as &dyn ToSql,
					&fl.accessed_at as &dyn ToSql,
					&node_id.to_le_bytes() as &dyn ToSql,
					&fl.path.to_string_lossy() as &dyn ToSql,
				]).unwrap();
				updated_count += 1;
			} else {
				// new file
				insert_stmt.execute(&[
					&node_id.to_le_bytes() as &dyn ToSql,
					&fl.path.to_string_lossy() as &dyn ToSql,
					&fl.hash as &dyn ToSql,
					&fl.size as &dyn ToSql,
					&fl.timestamp as &dyn ToSql,
					&fl.created_at as &dyn ToSql,
					&fl.modified_at as &dyn ToSql,
					&fl.accessed_at as &dyn ToSql,
				]).unwrap();
				inserted_count += 1;
			}
		}

		// upsert into file_entries as before…
		let mut upsert_stmt = tx.prepare(UPSERT_FILE_ENTRY).unwrap();
		for fl in scanned.values() {
			let timestamps: Vec<_> = [fl.created_at, fl.modified_at, fl.accessed_at]
				.iter().copied().flatten().collect();
			let first_dt  = timestamps.iter().min().copied();
			let latest_dt = timestamps.iter().max().copied();
			upsert_stmt.execute(&[
				&fl.hash as &dyn ToSql,
				&fl.size as &dyn ToSql,
				&fl.mime_type as &dyn ToSql,
				&first_dt as &dyn ToSql,
				&latest_dt as &dyn ToSql,
			]).unwrap();
		}
	}

	tx.commit().unwrap();
	Ok(ScanResult {
		updated_count,
		inserted_count,
		removed_count,
		duration: timer.elapsed(),
	})
}