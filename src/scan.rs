use std::fs::canonicalize;
use std::io;
use std::io::Read;
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use rayon::iter::IntoParallelIterator;
use rayon::iter::ParallelBridge;
use walkdir::WalkDir;
use rayon::prelude::*;

use crate::db::DB;
use crate::types::FileEntry;
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

fn handle_path<P: AsRef<Path>>(path: P) -> FileEntry {
	let full_path = canonicalize(path.as_ref()).unwrap();

	let file = std::fs::File::open(path).unwrap();
	let m = file.metadata().unwrap();
	let modified = to_datetime(m.modified());
	let accessed = to_datetime(m.accessed());
	let created = to_datetime(m.created());
	let hash = sha256_hash(file).unwrap();
	FileEntry {
		size: m.len(),
		first_datetime: created,
		hash: Some(hash),
		..Default::default()
	}
}

fn create_handle_path<P: AsRef<Path>>(path: P) -> String {
	path.as_ref().to_string_lossy().to_string()
}

fn to_datetime(m: std::io::Result<std::time::SystemTime>) -> Option<chrono::DateTime<chrono::Utc>> {
	m.ok().map(|t| chrono::DateTime::from(t))
}

pub fn scan<P: AsRef<Path>>(path: P, mut db: DB) {
	let path = path.as_ref().to_path_buf();
	let (tx, rx) = mpsc::channel();
	let timer = std::time::Instant::now();

	thread::spawn(move || {
		WalkDir::new(path)
			.into_iter()
			.filter_map(|e| e.ok())
			.filter_map(|entry| {
				if entry.file_type().is_file() {
					Some(entry)
				} else {
					None
				}
			})
			.enumerate()
			.par_bridge()
			.into_par_iter()
			.for_each(|(i, entry)| {
				if i % 1000 == 0 {
					let speed = i as f64 / timer.elapsed().as_secs_f64();
					println!("[{}] {:0.2}/s", i, speed);
				}
				let file_entry = handle_path(entry.path()); 
				tx.send(file_entry).unwrap();
			});
	});
	let mut file_entries = rx.iter().collect::<Vec<FileEntry>>();
	println!("scan took: {:?}", timer.elapsed());
	let timer = std::time::Instant::now();
	db.save_file_metadatas(&mut file_entries);
	println!("saving {} entries took: {:?}", file_entries.len(), timer.elapsed());
	// for mut file_entry in rx {
	// 	db.save_file_entry(&mut file_entry).unwrap();
	// }
}

pub struct Scanner {

}