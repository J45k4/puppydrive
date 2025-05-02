use core::hash;
use std::path::PathBuf;

use chrono::DateTime;
use chrono::Utc;
use rusqlite::Connection;
use rusqlite::ToSql;
use serde::Serialize;
use tokio::sync::Mutex;

struct Migration {
    id: u32,
    name: &'static str,
    sql: &'static str,
}

const MIGRATIONS: &[Migration] = &[
	Migration {
		id: 20250208,
		name: "init_database",
		sql: r"
		create table file_entries (
			hash blob not null unique primary key,
			size integer not null,
			mime_type text null,
			first_datetime timestamp null,
			latest_datetime timestamp null
		);
		create table file_locations (
			node_id bytes not null,
			path text not null,
			hash blob null,
			size integer not null,
			timestamp timestamp not null,
			created_at timestamp null,
			modified_at timestamp null,
			accessed_at timestamp null,
			primary key (node_id, path)
		);
		create table nodes (
			id integer not null primary key,
			name text not null,
			you bool not null,
			created_at timestamp not null,
			modified_at timestamp not null,
			accessed_at timestamp not null
		);
		CREATE INDEX IF NOT EXISTS idx_file_locations_path ON file_locations(path);
		CREATE INDEX IF NOT EXISTS idx_file_locations_hash ON file_locations(hash);
		"
	}
];

#[derive(Debug, Default, Serialize)]
pub struct FileEntry {
	pub hash: Vec<u8>,
	pub size: i64,
	pub mime_type: Option<String>,
	pub first_datetime: String,
	pub latest_datetime: String
}

#[derive(Debug, Default, Serialize)]
pub struct FileLocation {
	pub path: PathBuf,
	pub hash: Option<[u8; 32]>,
	pub size: u64,
	pub mime_type: Option<String>,
	pub timestamp: DateTime<Utc>,
	pub created_at: Option<DateTime<Utc>>,
	pub modified_at: Option<DateTime<Utc>>,
	pub accessed_at: Option<DateTime<Utc>>,
}

impl PartialEq for FileLocation {
	fn eq(&self, other: &Self) -> bool {
		self.path == other.path && 
		self.hash == other.hash && 
		self.size == other.size &&
		self.mime_type == other.mime_type &&
		self.created_at == other.created_at &&
		self.modified_at == other.modified_at &&
		self.accessed_at == other.accessed_at
	}
}

#[derive(Debug, Default, Serialize)]
pub struct ListArgs {
	search_word: Option<String>,
}

pub struct DB {
	conn: Mutex<Connection>
}

pub fn list_files(conn: &Connection, args: ListArgs) -> anyhow::Result<Vec<FileEntry>> {
	// Build SQL and params based on whether we have a search term
	let mut sql = String::from(
		"SELECT hash, size, mime_type, first_datetime, latest_datetime \
		 FROM file_entries"
	);
	let mut params: Vec<&dyn ToSql> = Vec::new();

	// if let Some(ref term) = args.search_word.filter(|s| !s.is_empty()) {
	// 	sql.push_str(" WHERE mime_type LIKE ?1");
	// 	let pattern = format!("%{}%", term);
	// 	params.push(&pattern);
	// }

	let mut stmt = conn.prepare(&sql)?;
	let rows = stmt.query_map(&params[..], |row| {
		Ok(FileEntry {
			hash: row.get(0)?,
			size: row.get(1)?,
			mime_type: row.get(2)?,
			first_datetime: row.get(3)?,
			latest_datetime: row.get(4)?,
		})
	})?;

	let mut files = Vec::new();
	for file in rows {
		files.push(file?);
	}
	Ok(files)
}


pub async fn get_mime_types(conn: &Connection) -> anyhow::Result<Vec<String>> {
	let mut stmt = conn.prepare("SELECT DISTINCT mime_type FROM file_entries WHERE mime_type IS NOT NULL")?;
	let rows = stmt.query_map((), |row| row.get::<_, String>(0)).unwrap();

	let mut mime_types = Vec::new();
	for mime_type in rows {
		mime_types.push(mime_type?);
	}

	Ok(mime_types)
}

pub fn get_file_entry(conn: &Connection, hash: &[u8]) -> anyhow::Result<Option<FileEntry>> {
    match conn.query_row(
        "SELECT hash, size, mime_type, first_datetime, latest_datetime FROM file_entries WHERE hash = ?1",
        [hash],
        |row| {
            Ok(FileEntry {
                hash: row.get(0)?,
                size: row.get(1)?,
                mime_type: row.get(2)?,
                first_datetime: row.get(3)?,
                latest_datetime: row.get(4)?,
            })
        },
    ) {
        Ok(entry) => Ok(Some(entry)),
        Err(e) => Err(e.into()),
    }
}

pub fn get_file_location(
	conn: &Connection,
	node_id: &[u8],
	hash: &[u8],
) -> anyhow::Result<Option<FileLocation>> {
	let mut stmt = conn.prepare(
		"SELECT path, hash, size, timestamp, created_at, modified_at, accessed_at \
		 FROM file_locations \
		 WHERE node_id = ? AND hash = ?",
	)?;
	let mut rows = stmt.query_map(&[node_id, hash], |row| {
		// get an optional Vec<u8> for the hash
		let hash_opt: Option<Vec<u8>> = row.get(1)?;
		// convert Vec<u8> into [u8; 32] if present
		let hash = hash_opt
			.as_ref()
			.map(|v| v.as_slice().try_into().expect("hash must be 32 bytes"));
		Ok(FileLocation {
			path: PathBuf::from(row.get::<_, String>(0)?),
			hash,
			size: row.get::<_, i64>(2)? as u64,
			// file_locations does not store mime_type, set to None
			mime_type: None,
			timestamp: row.get(3)?,
			created_at: row.get(4)?,
			modified_at: row.get(5)?,
			accessed_at: row.get(6)?,
		})
	})?;

	// return the first matching row if any
	if let Some(res) = rows.next() {
		Ok(Some(res?))
	} else {
		Ok(None)
	}
}


/// Runs embedded database migrations.
///
/// # Arguments
///
/// * `conn` - A mutable reference to the rusqlite `Connection`.
///
/// # Errors
///
/// Returns an `anyhow::Error` if any database operation fails.
pub fn run_migrations() -> anyhow::Result<()> {
	log::info!("running migrations");
	let mut conn = Connection::open("puppydrive.db").unwrap();
    conn.execute(
        "CREATE TABLE IF NOT EXISTS migrations (
            id INTEGER PRIMARY KEY,
            name TEXT NOT NULL UNIQUE,
            applied_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
        )",
        (),
    )?;

    let applied_migrations: Vec<u32> = {
        let mut stmt = conn.prepare("SELECT id FROM migrations")?;
        let m = stmt.query_map((), |row| row.get(0))?;
    	m.filter_map(Result::ok).collect()
    };

    let mut pending_migrations: Vec<&Migration> = MIGRATIONS
        .iter()
        .filter(|migration| !applied_migrations.contains(&migration.id))
        .collect();

    // Sort pending migrations by id to ensure correct order
    pending_migrations.sort_by_key(|migration| migration.id);
    if !pending_migrations.is_empty() {
        for migration in &pending_migrations {
            log::info!("applying migration {}: {}", migration.id, migration.name);

            // Begin a transaction for atomicity
            let tx = conn.transaction()?;

            // Execute the migration SQL
            tx.execute_batch(migration.sql)?;

            // Record the applied migration
            tx.execute(
                "INSERT INTO migrations (id, name) VALUES (?1, ?2)",
				&[&migration.id as &dyn ToSql, &migration.name as &dyn ToSql],
            )?;

            // Commit the transaction
            tx.commit()?;

            log::info!("migration {} applied successfully.", migration.id);
        }
    } else {
        log::info!("No new migrations to apply.");
    }

    Ok(())
}

pub fn open_db() -> Connection {
	Connection::open("puppydrive.db").unwrap()
}