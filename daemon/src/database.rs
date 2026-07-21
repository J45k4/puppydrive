use std::collections::HashMap;
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanTrigger {
    ManualFolder,
    ManualRefresh,
    FilesystemChange,
}

impl ScanTrigger {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ManualFolder => "manual-folder",
            Self::ManualRefresh => "manual-refresh",
            Self::FilesystemChange => "filesystem-change",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "manual-folder" => Self::ManualFolder,
            "manual-refresh" => Self::ManualRefresh,
            _ => Self::FilesystemChange,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScanOutcome {
    Completed,
    Incomplete,
    Failed,
}

impl ScanOutcome {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Incomplete => "incomplete",
            Self::Failed => "failed",
        }
    }

    fn from_str(value: &str) -> Self {
        match value {
            "completed" => Self::Completed,
            "incomplete" => Self::Incomplete,
            _ => Self::Failed,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScanHistoryEntry {
    pub scanned_folder_id: u32,
    pub trigger: ScanTrigger,
    pub outcome: ScanOutcome,
    pub started_at: i64,
    pub finished_at: i64,
    pub directories_scanned: usize,
    pub files_indexed: usize,
    pub error_message: Option<String>,
}

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

    pub fn scanned_folder_scan_history(&self, folder_id: u32) -> Result<Vec<ScanHistoryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT id, scanned_folder_id, trigger, outcome, started_at, finished_at,
                    directories_scanned, files_indexed, error_message
             FROM scanned_folder_scans
             WHERE scanned_folder_id = ?1
             ORDER BY finished_at DESC, id DESC",
        )?;
        let rows = statement.query_map([folder_id], scan_history_entry_from_row)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn save_scanned_folder_scan(&self, entry: ScanHistoryEntry) -> Result<ScanHistoryEntry> {
        let mut connection = self.connection()?;
        let transaction = connection.transaction()?;
        transaction.execute(
            "INSERT INTO scanned_folder_scans
                (scanned_folder_id, trigger, outcome, started_at, finished_at,
                 directories_scanned, files_indexed, error_message)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                entry.scanned_folder_id,
                entry.trigger.as_str(),
                entry.outcome.as_str(),
                entry.started_at,
                entry.finished_at,
                entry.directories_scanned as i64,
                entry.files_indexed as i64,
                entry.error_message,
            ],
        )?;
        transaction.execute(
            "DELETE FROM scanned_folder_scans
             WHERE scanned_folder_id = ?1 AND id IN (
                 SELECT id FROM scanned_folder_scans
                 WHERE scanned_folder_id = ?1
                 ORDER BY finished_at DESC, id DESC
                 LIMIT -1 OFFSET 100
             )",
            [entry.scanned_folder_id],
        )?;
        transaction.commit()?;
        Ok(entry)
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
            "WITH media_locations AS (
                 SELECT location.path, location.size, location.mime_type, location.modified_at,
                        MIN(membership.scanned_folder_id) AS scanned_folder_id, location.hash
                 FROM file_locations location
                 JOIN scanned_folder_locations membership
                   ON membership.node_id = location.node_id AND membership.path = location.path
                 JOIN ScannedFolder folder ON folder.id = membership.scanned_folder_id
                 WHERE location.node_id = ?1 AND folder.enabled = 1 AND membership.indexer = 'media'
                   AND (location.mime_type LIKE 'image/%' OR location.mime_type LIKE 'video/%')
                 GROUP BY location.node_id, location.path
             ), media_representatives AS (
                 SELECT candidate.*,
                        CASE WHEN candidate.hash IS NULL THEN 1 ELSE (
                            SELECT COUNT(*) FROM media_locations replica
                            WHERE replica.hash = candidate.hash
                        ) END AS replica_count
                 FROM media_locations candidate
                 WHERE candidate.hash IS NULL OR candidate.path = (
                     SELECT MIN(replica.path) FROM media_locations replica
                     WHERE replica.hash = candidate.hash
                 )
             )
             SELECT path, size, mime_type, modified_at, scanned_folder_id, hash, replica_count
             FROM media_representatives
             ORDER BY lower(path)",
        )?;
        let rows = statement.query_map([node_id], |row| {
            Ok(IndexedMediaFile {
                path: PathBuf::from(row.get::<_, String>(0)?),
                size: row.get::<_, i64>(1)? as u64,
                mime_type: row.get(2)?,
                modified_at: row.get(3)?,
                scanned_folder_id: row.get::<_, i64>(4)? as u32,
                hash: row.get(5)?,
                replica_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn cached_audio(&self, node_id: &[u8]) -> Result<Vec<IndexedMediaFile>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "WITH audio_locations AS (
                 SELECT location.path, location.size, location.mime_type, location.modified_at,
                        MIN(membership.scanned_folder_id) AS scanned_folder_id, location.hash
                 FROM file_locations location
                 JOIN scanned_folder_locations membership
                   ON membership.node_id = location.node_id AND membership.path = location.path
                 JOIN ScannedFolder folder ON folder.id = membership.scanned_folder_id
                 WHERE location.node_id = ?1 AND folder.enabled = 1 AND membership.indexer = 'media'
                   AND location.mime_type LIKE 'audio/%'
                 GROUP BY location.node_id, location.path
             ), audio_representatives AS (
                 SELECT candidate.*,
                        CASE WHEN candidate.hash IS NULL THEN 1 ELSE (
                            SELECT COUNT(*) FROM audio_locations replica
                            WHERE replica.hash = candidate.hash
                        ) END AS replica_count
                 FROM audio_locations candidate
                 WHERE candidate.hash IS NULL OR candidate.path = (
                     SELECT MIN(replica.path) FROM audio_locations replica
                     WHERE replica.hash = candidate.hash
                 )
             )
             SELECT path, size, mime_type, modified_at, scanned_folder_id, hash, replica_count
             FROM audio_representatives
             ORDER BY lower(path)",
        )?;
        let rows = statement.query_map([node_id], |row| {
            Ok(IndexedMediaFile {
                path: PathBuf::from(row.get::<_, String>(0)?),
                size: row.get::<_, i64>(1)? as u64,
                mime_type: row.get(2)?,
                modified_at: row.get(3)?,
                scanned_folder_id: row.get::<_, i64>(4)? as u32,
                hash: row.get(5)?,
                replica_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn cached_files(&self, node_id: &[u8]) -> Result<Vec<IndexedFile>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT location.path, location.size, location.mime_type, location.modified_at,
                    location.hash,
                    (SELECT MIN(membership.scanned_folder_id) FROM scanned_folder_locations membership
                     WHERE membership.node_id = location.node_id AND membership.path = location.path),
                    (SELECT COUNT(*) FROM file_locations replica WHERE replica.hash = location.hash)
             FROM file_locations location
             WHERE location.node_id = ?1
             ORDER BY lower(location.path)",
        )?;
        let rows = statement.query_map([node_id], |row| {
            Ok(IndexedFile {
                path: PathBuf::from(row.get::<_, String>(0)?),
                size: row.get::<_, i64>(1)? as u64,
                mime_type: row.get(2)?,
                modified_at: row.get(3)?,
                hash: row.get(4)?,
                scanned_folder_id: row.get::<_, Option<i64>>(5)?.map(|id| id as u32),
                replica_count: row.get::<_, i64>(6)? as usize,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn scanned_folder_location_metadata(
        &self,
        node_id: &[u8],
        folder_id: u32,
    ) -> Result<HashMap<PathBuf, IndexedLocationMetadata>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT location.path, location.hash, location.size, location.created_at, location.modified_at
             FROM file_locations location
             JOIN scanned_folder_locations membership
               ON membership.node_id = location.node_id AND membership.path = location.path
             WHERE membership.scanned_folder_id = ?1 AND membership.node_id = ?2
               AND membership.indexer = 'media'",
        )?;
        let mut locations = HashMap::new();
        let rows = statement.query_map(params![folder_id, node_id], |row| {
            Ok((
                PathBuf::from(row.get::<_, String>(0)?),
                IndexedLocationMetadata {
                    hash: row.get(1)?,
                    size: row.get::<_, i64>(2)? as u64,
                    created_at: row.get(3)?,
                    modified_at: row.get(4)?,
                },
            ))
        })?;
        for row in rows {
            let (path, metadata) = row?;
            locations.insert(path, metadata);
        }
        Ok(locations)
    }

    pub fn media_thumbnail_source(
        &self,
        node_id: &[u8],
        folder_id: u32,
        hash: &[u8],
    ) -> Result<Option<PathBuf>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT location.path
                 FROM file_locations location
                 JOIN scanned_folder_locations membership
                   ON membership.node_id = location.node_id AND membership.path = location.path
                 JOIN ScannedFolder folder ON folder.id = membership.scanned_folder_id
                 WHERE location.node_id = ?1 AND membership.scanned_folder_id = ?2
                   AND membership.indexer = 'media' AND folder.enabled = 1 AND location.hash = ?3
                   AND location.mime_type LIKE 'image/%'
                 ORDER BY lower(location.path)
                 LIMIT 1",
                params![node_id, folder_id, hash],
                |row| row.get::<_, String>(0).map(PathBuf::from),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn virtual_directories(&self) -> Result<Vec<VirtualDirectory>> {
        let connection = self.connection()?;
        let mut statement = connection
            .prepare("SELECT id, name FROM virtual_directories ORDER BY lower(name), id")?;
        let rows = statement.query_map([], |row| {
            Ok(VirtualDirectory {
                id: row.get::<_, i64>(0)? as u32,
                name: row.get(1)?,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn create_virtual_directory(&self, name: &str) -> Result<VirtualDirectory> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("virtual directory name cannot be empty");
        }
        let connection = self.connection()?;
        connection.execute(
            "INSERT INTO virtual_directories (name, created_at) VALUES (?1, ?2)",
            params![name, now_millis()],
        )?;
        Ok(VirtualDirectory {
            id: connection.last_insert_rowid() as u32,
            name: name.to_owned(),
        })
    }

    pub fn file_hash_for_location(&self, node_id: &[u8], path: &Path) -> Result<Option<Vec<u8>>> {
        let connection = self.connection()?;
        connection
            .query_row(
                "SELECT hash FROM file_locations WHERE node_id = ?1 AND path = ?2 AND hash IS NOT NULL",
                params![node_id, path.to_string_lossy()],
                |row| row.get(0),
            )
            .optional()
            .map_err(Into::into)
    }

    pub fn add_file_to_virtual_directory(&self, directory_id: u32, hash: &[u8]) -> Result<()> {
        let connection = self.connection()?;
        connection.execute(
            "INSERT OR IGNORE INTO virtual_directory_entries
                (virtual_directory_id, file_hash, added_at) VALUES (?1, ?2, ?3)",
            params![directory_id, hash, now_millis()],
        )?;
        Ok(())
    }

    pub fn virtual_directory_entries(&self, node_id: &[u8]) -> Result<Vec<VirtualDirectoryEntry>> {
        let connection = self.connection()?;
        let mut statement = connection.prepare(
            "SELECT directory.id, entry.size, entry.mime_type, entry.hash,
                    (SELECT location.path FROM file_locations location
                     WHERE location.node_id = ?1 AND location.hash = entry.hash
                     ORDER BY lower(location.path) LIMIT 1),
                    (SELECT location.modified_at FROM file_locations location
                     WHERE location.node_id = ?1 AND location.hash = entry.hash
                     ORDER BY lower(location.path) LIMIT 1),
                    (SELECT MIN(membership.scanned_folder_id) FROM scanned_folder_locations membership
                     JOIN ScannedFolder folder ON folder.id = membership.scanned_folder_id
                     WHERE membership.node_id = ?1 AND membership.path = (
                         SELECT location.path FROM file_locations location
                         WHERE location.node_id = ?1 AND location.hash = entry.hash
                         ORDER BY lower(location.path) LIMIT 1
                     ) AND membership.indexer = 'media' AND folder.enabled = 1),
                    (SELECT COUNT(*) FROM file_locations replica WHERE replica.hash = entry.hash)
             FROM virtual_directories directory
             JOIN virtual_directory_entries link ON link.virtual_directory_id = directory.id
             JOIN file_entries entry ON entry.hash = link.file_hash
             ORDER BY lower(directory.name), link.added_at, entry.hash",
        )?;
        let rows = statement.query_map([node_id], |row| {
            Ok(VirtualDirectoryEntry {
                virtual_directory_id: row.get::<_, i64>(0)? as u32,
                size: row.get::<_, i64>(1)? as u64,
                mime_type: row.get(2)?,
                hash: row.get(3)?,
                path: row.get::<_, Option<String>>(4)?.map(PathBuf::from),
                modified_at: row.get(5)?,
                scanned_folder_id: row.get::<_, Option<i64>>(6)?.map(|id| id as u32),
                replica_count: row.get::<_, i64>(7)? as usize,
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
    pub hash: Option<Vec<u8>>,
    pub replica_count: usize,
}

#[derive(Debug, Clone)]
pub struct IndexedFile {
    pub path: PathBuf,
    pub size: u64,
    pub mime_type: Option<String>,
    pub modified_at: Option<i64>,
    pub hash: Option<Vec<u8>>,
    pub scanned_folder_id: Option<u32>,
    pub replica_count: usize,
}

#[derive(Debug, Clone)]
pub struct IndexedLocationMetadata {
    pub hash: Option<Vec<u8>>,
    pub size: u64,
    pub created_at: Option<i64>,
    pub modified_at: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct VirtualDirectory {
    pub id: u32,
    pub name: String,
}

#[derive(Debug, Clone)]
pub struct VirtualDirectoryEntry {
    pub virtual_directory_id: u32,
    pub size: u64,
    pub mime_type: Option<String>,
    pub hash: Vec<u8>,
    pub path: Option<PathBuf>,
    pub modified_at: Option<i64>,
    pub scanned_folder_id: Option<u32>,
    pub replica_count: usize,
}

fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn scan_history_entry_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<ScanHistoryEntry> {
    Ok(ScanHistoryEntry {
        scanned_folder_id: row.get::<_, i64>(1)? as u32,
        trigger: ScanTrigger::from_str(&row.get::<_, String>(2)?),
        outcome: ScanOutcome::from_str(&row.get::<_, String>(3)?),
        started_at: row.get(4)?,
        finished_at: row.get(5)?,
        directories_scanned: row.get::<_, i64>(6)? as usize,
        files_indexed: row.get::<_, i64>(7)? as usize,
        error_message: row.get(8)?,
    })
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

    #[tokio::test]
    async fn scanned_folder_history_is_newest_first_and_keeps_last_hundred() {
        let path = temporary_database("scan-history");
        let db = Database::open(&path).unwrap();
        let folder = db
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: "/photos".to_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        for finished_at in 0..101 {
            db.save_scanned_folder_scan(ScanHistoryEntry {
                scanned_folder_id: folder.id,
                trigger: ScanTrigger::ManualFolder,
                outcome: ScanOutcome::Completed,
                started_at: finished_at - 1,
                finished_at,
                directories_scanned: 1,
                files_indexed: 2,
                error_message: None,
            })
            .unwrap();
        }
        let history = db.scanned_folder_scan_history(folder.id).unwrap();
        assert_eq!(history.len(), 100);
        assert_eq!(history.first().unwrap().finished_at, 100);
        assert_eq!(history.last().unwrap().finished_at, 1);
        assert!(db.delete_scanned_folder(folder.id).unwrap());
        assert!(
            db.scanned_folder_scan_history(folder.id)
                .unwrap()
                .is_empty()
        );
        drop(db);
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
        let media = db.cached_media(&node_id).unwrap();
        assert_eq!(media.len(), 1);
        assert_eq!(media[0].replica_count, 2);

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

    #[tokio::test]
    async fn virtual_directory_links_are_idempotent_and_survive_forgotten_locations() {
        let path = temporary_database("virtual-directories");
        let db = Database::open(&path).unwrap();
        let node_id = db.local_node_id("PuppyDrive").unwrap();
        let folder = db
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: "/photos".to_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let hash = vec![9; 32];
        db.sync_media_scan(
            &node_id,
            folder.id,
            &[MediaIndexObservation {
                path: PathBuf::from("/photos/a.jpg"),
                hash: Some(hash.clone()),
                size: 42,
                mime_type: Some("image/jpeg".to_owned()),
                created_at: None,
                modified_at: Some(1),
                accessed_at: None,
            }],
            true,
        )
        .unwrap();
        let directory = db.create_virtual_directory("Favourites").unwrap();
        db.add_file_to_virtual_directory(directory.id, &hash)
            .unwrap();
        db.add_file_to_virtual_directory(directory.id, &hash)
            .unwrap();
        assert_eq!(db.virtual_directory_entries(&node_id).unwrap().len(), 1);

        assert!(db.delete_scanned_folder(folder.id).unwrap());
        let entries = db.virtual_directory_entries(&node_id).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].path, None);
        drop(db);
        let _ = fs::remove_file(path);
    }
}
