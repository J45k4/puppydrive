use rusqlite::Connection;
use rusqlite::ToSql;

use crate::types::FileEntry;
use crate::types::FileLocation;


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
			hash blob null unique,
			size integer not null,
			first_datetime timestamp null,
			latest_datetime timestamp null
		);
		create table file_locations (
			id integer primary key,
			path text not null,
			file_entry_hash blob null references file_entries(id),
			created_at timestamp null,
			modified_at timestamp null,
			accessed_at timestamp null
		);
		"
	}
];


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

pub struct DB {
	conn: Connection
}

impl DB {
	pub fn new() -> Self {
		let conn = Connection::open("puppydrive.db").unwrap();
		Self {
			conn
		}
	}

	pub fn save_file_metadatas(&mut self, entries: &mut [FileEntry]) -> anyhow::Result<()> {
        // Start a transaction.
        let tx = self.conn.transaction()?;
		{
			// Prepare the statements once to reuse them for each entry.
			let mut insert_stmt = tx.prepare(
				"INSERT INTO file_entries (size, hash, first_datetime) VALUES (?1, ?2, ?3)"
			)?;

			// Process each entry in the batch.
			for entry in entries.iter_mut() {
				insert_stmt.execute(&[
					&entry.size as &dyn ToSql,
					&entry.hash as &dyn ToSql,
					&entry.first_datetime as &dyn ToSql,
				])?;
			}
		}
        // Commit the transaction.
        tx.commit()?;
        Ok(())
    }
}