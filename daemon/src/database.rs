use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use rusqlite::{OptionalExtension, params};
use serde::{Deserialize, Serialize};
use wgui::{DbTable, HasId, SQLLiteDB, SqliteTable, Wdb, WguiModel, apply_sqlite_migrations};

#[derive(Debug, Clone, Serialize, Deserialize, WguiModel)]
pub struct ScannedFolder {
    pub id: u32,
    pub path: String,
    pub enabled: bool,
    pub indexers: String,
}

impl ScannedFolder {
    pub const MEDIA_INDEXER: &'static str = "media";

    pub fn indexes_media(&self) -> bool {
        serde_json::from_str::<Vec<String>>(&self.indexers).is_ok_and(|indexers| {
            indexers
                .iter()
                .any(|indexer| indexer == Self::MEDIA_INDEXER)
        })
    }
}

/// Compatibility alias while the UI transitions from the old Media-folder wording.
pub type MediaScanPath = ScannedFolder;

impl HasId for ScannedFolder {
    fn id(&self) -> u32 {
        self.id
    }

    fn set_id(&mut self, id: u32) {
        self.id = id;
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, WguiModel)]
pub struct Source {
    pub id: u32,
    pub source_key: String,
    pub name: String,
    pub source_type: String,
    pub config_schema_version: i32,
    pub config: String,
    pub enabled: bool,
}

impl HasId for Source {
    fn id(&self) -> u32 {
        self.id
    }

    fn set_id(&mut self, id: u32) {
        self.id = id;
    }
}

#[derive(Debug, Wdb)]
#[allow(dead_code)]
struct PuppyDriveDb {
    scanned_folders: DbTable<ScannedFolder>,
    sources: DbTable<Source>,
}

pub struct Database {
    path: PathBuf,
    scanned_folders: SqliteTable<ScannedFolder>,
    sources: SqliteTable<Source>,
}

impl Database {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed creating database directory {}", parent.display())
            })?;
        }
        let migrations = Path::new(env!("CARGO_MANIFEST_DIR")).join("migrations");
        apply_sqlite_migrations(path, &migrations).with_context(|| {
            format!(
                "failed applying migrations from {} to {}",
                migrations.display(),
                path.display()
            )
        })?;
        let db = SQLLiteDB::<PuppyDriveDb>::open(path)
            .with_context(|| format!("failed opening database {}", path.display()))?;
        Ok(Self {
            path: path.to_path_buf(),
            scanned_folders: db.table()?,
            sources: db.table()?,
        })
    }

    pub fn scanned_folders(&self) -> Result<Vec<ScannedFolder>> {
        self.scanned_folders.snapshot_sync()
    }

    pub fn media_scan_paths(&self) -> Result<Vec<ScannedFolder>> {
        self.scanned_folders()
    }

    pub fn sources(&self) -> Result<Vec<Source>> {
        self.sources.snapshot_sync()
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn insert_initial_scanned_folder(&self, path: &Path) -> Result<()> {
        self.scanned_folders.insert_sync(ScannedFolder {
            id: 0,
            path: path.to_string_lossy().into_owned(),
            enabled: true,
            indexers: r#"["media"]"#.to_owned(),
        })
    }

    pub fn insert_initial_media_path(&self, path: &Path) -> Result<()> {
        self.insert_initial_scanned_folder(path)
    }

    pub async fn save_scanned_folder(&self, folder: ScannedFolder) -> Result<ScannedFolder> {
        self.scanned_folders.save(folder).await
    }

    pub async fn save_media_path(&self, folder: ScannedFolder) -> Result<ScannedFolder> {
        self.save_scanned_folder(folder).await
    }

    pub async fn save_source(&self, source: Source) -> Result<Source> {
        self.sources.save(source).await
    }

    pub fn delete_scanned_folder(&self, id: u32) -> Result<bool> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "DELETE FROM scanned_folder_locations WHERE scanned_folder_id = ?1",
            [id],
        )?;
        transaction.execute(
            "DELETE FROM file_locations WHERE NOT EXISTS (
                SELECT 1 FROM scanned_folder_locations membership
                WHERE membership.node_id = file_locations.node_id
                  AND membership.path = file_locations.path
            )",
            [],
        )?;
        let affected = transaction.execute("DELETE FROM ScannedFolder WHERE id = ?1", [id])?;
        transaction.commit()?;
        Ok(affected > 0)
    }

    pub fn delete_media_path(&self, id: u32) -> Result<bool> {
        self.delete_scanned_folder(id)
    }

    pub fn local_node_id(&self, name: &str) -> Result<Vec<u8>> {
        let connection = self.connection()?;
        let existing = connection
            .query_row("SELECT node_id FROM nodes WHERE is_local = 1", [], |row| {
                row.get(0)
            })
            .optional()?;
        if let Some(node_id) = existing {
            return Ok(node_id);
        }
        let node_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        connection.execute(
            "INSERT INTO nodes (node_id, name, is_local, created_at) VALUES (?1, ?2, 1, ?3)",
            params![node_id, name, now_millis()],
        )?;
        Ok(node_id)
    }

    pub fn cached_media(&self, node_id: &[u8]) -> Result<Vec<IndexedMediaFile>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT location.path, location.size, location.mime_type, location.modified_at,
                    MIN(membership.scanned_folder_id)
             FROM file_locations location
             JOIN scanned_folder_locations membership
               ON membership.node_id = location.node_id AND membership.path = location.path
             JOIN ScannedFolder folder ON folder.id = membership.scanned_folder_id
             WHERE location.node_id = ?1 AND folder.enabled = 1 AND membership.indexer = 'media'
             GROUP BY location.node_id, location.path
             ORDER BY lower(location.path)",
        )?;
        let rows = statement.query_map([node_id], |row| {
            Ok(IndexedMediaFile {
                path: PathBuf::from(row.get::<_, String>(0)?),
                size: row.get::<_, i64>(1)? as u64,
                mime_type: row.get(2)?,
                modified_at: row.get(3)?,
                scanned_folder_id: row.get::<_, i64>(4)? as u32,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn sync_media_scan(
        &self,
        node_id: &[u8],
        folder_id: u32,
        observations: &[MediaIndexObservation],
        complete: bool,
    ) -> Result<()> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        let scan_id = uuid::Uuid::new_v4().as_bytes().to_vec();
        let indexed_at = now_millis();
        for observation in observations {
            let path = observation.path.to_string_lossy();
            if let Some(hash) = &observation.hash {
                transaction.execute(
                    "INSERT INTO file_entries (hash, size, mime_type, first_indexed_at, last_indexed_at)
                     VALUES (?1, ?2, ?3, ?4, ?4)
                     ON CONFLICT(hash) DO UPDATE SET last_indexed_at = excluded.last_indexed_at",
                    params![hash, observation.size as i64, observation.mime_type, indexed_at],
                )?;
            }
            transaction.execute(
                "INSERT INTO file_locations
                    (node_id, path, hash, size, mime_type, last_indexed_at, created_at, modified_at, accessed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                 ON CONFLICT(node_id, path) DO UPDATE SET
                    hash = excluded.hash, size = excluded.size, mime_type = excluded.mime_type,
                    last_indexed_at = excluded.last_indexed_at, created_at = excluded.created_at,
                    modified_at = excluded.modified_at, accessed_at = excluded.accessed_at",
                params![
                    node_id,
                    path,
                    observation.hash,
                    observation.size as i64,
                    observation.mime_type,
                    indexed_at,
                    observation.created_at,
                    observation.modified_at,
                    observation.accessed_at,
                ],
            )?;
            transaction.execute(
                "INSERT INTO scanned_folder_locations
                    (scanned_folder_id, node_id, path, indexer, last_seen_scan)
                 VALUES (?1, ?2, ?3, 'media', ?4)
                 ON CONFLICT(scanned_folder_id, node_id, path, indexer)
                 DO UPDATE SET last_seen_scan = excluded.last_seen_scan",
                params![folder_id, node_id, path, scan_id],
            )?;
        }
        if complete {
            transaction.execute(
                "DELETE FROM scanned_folder_locations
                 WHERE scanned_folder_id = ?1 AND indexer = 'media' AND last_seen_scan != ?2",
                params![folder_id, scan_id],
            )?;
            transaction.execute(
                "DELETE FROM file_locations WHERE NOT EXISTS (
                    SELECT 1 FROM scanned_folder_locations membership
                    WHERE membership.node_id = file_locations.node_id
                      AND membership.path = file_locations.path
                )",
                [],
            )?;
        }
        transaction.commit()?;
        Ok(())
    }

    fn connection(&self) -> Result<rusqlite::Connection> {
        let connection = rusqlite::Connection::open(&self.path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        Ok(connection)
    }
}

#[derive(Debug, Clone)]
pub struct MediaIndexObservation {
    pub path: PathBuf,
    pub hash: Option<Vec<u8>>,
    pub size: u64,
    pub mime_type: Option<String>,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
    pub accessed_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct IndexedMediaFile {
    pub path: PathBuf,
    pub size: u64,
    pub mime_type: Option<String>,
    pub modified_at: Option<i64>,
    pub scanned_folder_id: u32,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LocalSourceConfig {
    pub path: String,
}

pub fn validate_source_config(source_type: &str, version: i32, config: &str) -> Result<()> {
    match (source_type, version) {
        ("local", 1) => {
            let config: LocalSourceConfig =
                serde_json::from_str(config).context("invalid local source configuration")?;
            let path = Path::new(&config.path);
            if !path.is_absolute() {
                anyhow::bail!("local source path must be absolute");
            }
            Ok(())
        }
        (known, _) if known == "local" => {
            anyhow::bail!("unsupported local source configuration version {version}")
        }
        _ => anyhow::bail!("unsupported source type '{source_type}'"),
    }
}

pub fn local_source_path(source: &Source) -> Option<PathBuf> {
    if source.source_type != "local" || source.config_schema_version != 1 {
        return None;
    }
    serde_json::from_str::<LocalSourceConfig>(&source.config)
        .ok()
        .map(|config| PathBuf::from(config.path))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_database(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("puppydrive-{name}-{}.db", uuid::Uuid::new_v4()))
    }

    #[tokio::test]
    async fn records_persist_and_media_paths_can_be_removed() {
        let path = temporary_database("persistence");
        let db = Database::open(&path).unwrap();
        let media = db
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: "/photos".to_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let source = db
            .save_source(Source {
                id: 0,
                source_key: uuid::Uuid::new_v4().to_string(),
                name: "Archive".to_owned(),
                source_type: "local".to_owned(),
                config_schema_version: 1,
                config: r#"{"path":"/archive"}"#.to_owned(),
                enabled: true,
            })
            .await
            .unwrap();
        drop(db);

        let reopened = Database::open(&path).unwrap();
        assert_eq!(reopened.scanned_folders().unwrap()[0].path, "/photos");
        assert!(reopened.scanned_folders().unwrap()[0].indexes_media());
        assert_eq!(reopened.sources().unwrap()[0].source_key, source.source_key);
        assert!(reopened.delete_media_path(media.id).unwrap());
        assert!(reopened.media_scan_paths().unwrap().is_empty());
        drop(reopened);
        let _ = fs::remove_file(path);
    }

    #[test]
    fn typed_local_config_rejects_secrets_and_relative_paths() {
        assert!(validate_source_config("local", 1, r#"{"path":"/archive"}"#).is_ok());
        assert!(validate_source_config("local", 1, r#"{"path":"archive"}"#).is_err());
        assert!(
            validate_source_config("local", 1, r#"{"path":"/archive","password":"plaintext"}"#)
                .is_err()
        );
    }

    #[tokio::test]
    async fn persistent_index_deduplicates_content_and_preserves_entries_after_forget() {
        let path = temporary_database("persistent-index");
        let db = Database::open(&path).unwrap();
        let node_id = db.local_node_id("PuppyDrive").unwrap();
        assert_eq!(node_id, db.local_node_id("Renamed PuppyDrive").unwrap());

        let first = db
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: "/photos".to_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let second = db
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: "/backup/photos".to_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let hash = vec![7; 32];
        let observation = |path: &str| MediaIndexObservation {
            path: PathBuf::from(path),
            hash: Some(hash.clone()),
            size: 42,
            mime_type: Some("image/jpeg".to_owned()),
            created_at: None,
            modified_at: Some(1),
            accessed_at: None,
        };
        db.sync_media_scan(&node_id, first.id, &[observation("/photos/a.jpg")], true)
            .unwrap();
        db.sync_media_scan(
            &node_id,
            second.id,
            &[observation("/backup/photos/a.jpg")],
            true,
        )
        .unwrap();
        assert_eq!(db.cached_media(&node_id).unwrap().len(), 2);

        assert!(db.delete_scanned_folder(first.id).unwrap());
        assert_eq!(db.cached_media(&node_id).unwrap().len(), 1);
        let connection = db.connection().unwrap();
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        assert!(db.delete_scanned_folder(second.id).unwrap());
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM file_locations", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
        assert_eq!(
            connection
                .query_row("SELECT COUNT(*) FROM file_entries", [], |row| row
                    .get::<_, i64>(0))
                .unwrap(),
            1
        );
        drop(connection);
        drop(db);
        let _ = fs::remove_file(path);
    }
}
