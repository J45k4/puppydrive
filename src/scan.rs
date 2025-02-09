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
use crate::types::FileLocation;

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
	// conn.execute_batch("PRAGMA synchronous = OFF; PRAGMA journal_mode = MEMORY;").unwrap();
	let tx = conn.transaction().unwrap();
	{
		let mut file_locations_stmt = match tx.prepare(
			"SELECT path, hash, size, timestamp, created_at, modified_at, accessed_at FROM file_locations where path like ?"
		) {
			Ok(stmt) => stmt,
			Err(err) => return Err(format!("error preparing statement: {:?}", err))
		};
		let file_locations: HashMap<PathBuf, FileLocation> = match file_locations_stmt.query_map([&(absolute_path.to_string_lossy() + "%")], |row| {
			Ok(FileLocation {
				path: PathBuf::from(row.get::<_, String>(0)?),
				hash: row.get(1)?,
				size: row.get(2)?,
				mime_type: None,
				timestamp: row.get(3)?,
				created_at: row.get(4)?,
				modified_at: row.get(5)?,
				accessed_at: row.get(6)?
			})
		}) {
			Ok(iter) => iter.filter_map(Result::ok).map(|file_location| (file_location.path.to_path_buf(), file_location)).collect(),
			Err(err) => return Err(format!("error querying file locations: {:?}", err))
		};

		let scan_timer = std::time::Instant::now();
		let scanned_file_locations = WalkDir::new(absolute_path)
			.into_iter()
			.filter_map(|e| e.ok())
			.filter_map(|entry| {
				if entry.file_type().is_file() {
					Some(entry)
				} else {
					None        
				}
			})
			.collect::<Vec<_>>()
			.into_iter();
		log::info!("scanning {} files in {:?}", scanned_file_locations.len(), scan_timer.elapsed());

		#[cfg(feature = "rayon")]
		let scanned_file_locations = scanned_file_locations.par_bridge();

		let hash_timer = std::time::Instant::now();
		let scanned_file_locations = scanned_file_locations.map(|entry| {
				let file_location = handle_path(entry.path());
				(file_location.path.to_path_buf(), file_location)
			})
			.collect::<HashMap<PathBuf, FileLocation>>();
		log::info!("hashed {} files in {:?}", scanned_file_locations.len(), hash_timer.elapsed());

		let mut delete_file_location = tx.prepare(DELETE_FILE_LOCATION).unwrap();
		for file_location in file_locations.values() {
			if !scanned_file_locations.contains_key(&file_location.path) {
				delete_file_location.execute(&[&node_id.to_le_bytes() as &dyn ToSql, &file_location.path.to_string_lossy() as &dyn ToSql]).unwrap();
				removed_count += 1;
			}
		}
		let mut insert_file_location = tx.prepare(INSERT_FILE_LOCATION).unwrap();
		let mut update_file_location = tx.prepare(UPDATE_FILE_LOCATION).unwrap();
		for scanned_file_location in scanned_file_locations.values() {
			match file_locations.get(&scanned_file_location.path) {
				Some(prev) => {
					if scanned_file_location == prev { continue; }
					update_file_location.execute(&[
						&scanned_file_location.hash as &dyn ToSql,
						&scanned_file_location.size as &dyn ToSql,
						&scanned_file_location.timestamp as &dyn ToSql,
						&scanned_file_location.created_at as &dyn ToSql,
						&scanned_file_location.modified_at as &dyn ToSql,
						&scanned_file_location.accessed_at as &dyn ToSql,
						&node_id.to_le_bytes() as &dyn ToSql,
						&scanned_file_location.path.to_string_lossy() as &dyn ToSql
					]).unwrap();
					updated_count += 1;
				},
				None => {
					insert_file_location.execute(&[
						&node_id.to_le_bytes() as &dyn ToSql,
						&scanned_file_location.path.to_string_lossy() as &dyn ToSql,
						&scanned_file_location.hash as &dyn ToSql,
						&scanned_file_location.size as &dyn ToSql,
						&scanned_file_location.timestamp as &dyn ToSql,
						&scanned_file_location.created_at as &dyn ToSql,
						&scanned_file_location.modified_at as &dyn ToSql,
						&scanned_file_location.accessed_at as &dyn ToSql
					]).unwrap();
					inserted_count += 1;
				}
			}
			
		}

		let mut upsert_file_entry = tx.prepare(UPSERT_FILE_ENTRY).unwrap();
		for file_location in scanned_file_locations.values() {
			// Collect the available timestamps into a Vec.
			let timestamps: Vec<_> = [
				file_location.created_at,
				file_location.modified_at,
				file_location.accessed_at,
			]
			.iter()
			.copied()
			.flatten() // Remove any None values.
			.collect();
		
			// Compute the earliest and latest dates if any timestamps exist.
			let first_datetime = timestamps.iter().min().copied();
			let latest_datetime = timestamps.iter().max().copied();
		
			upsert_file_entry.execute(&[
				&file_location.hash as &dyn ToSql,
				&file_location.size as &dyn ToSql,
				&file_location.mime_type as &dyn ToSql,
				&first_datetime as &dyn ToSql,
				&latest_datetime as &dyn ToSql,
			]).unwrap();
		}
	}

	tx.commit().unwrap();
	Ok(ScanResult {
		updated_count,
		inserted_count,
		removed_count,
		duration: timer.elapsed()
	})
}