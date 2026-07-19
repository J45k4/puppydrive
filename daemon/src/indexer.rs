use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender as StdSender};
use std::thread;
use std::time::SystemTime;

use tokio::sync::mpsc::Sender;

use crate::database::{
    Database, MediaIndexObservation, ScanHistoryEntry, ScanOutcome, ScanTrigger, ScannedFolder,
};
use crate::managed_folder::ManagedFolder;

#[derive(Debug, Clone)]
pub enum IndexerEvent {
    Started {
        folder_ids: Vec<u32>,
    },
    Progress {
        folder_id: u32,
        directories_scanned: usize,
        media_files_indexed: usize,
        current_path: Option<String>,
    },
    FolderFinished {
        history: ScanHistoryEntry,
    },
    Finished {
        truncated: bool,
        errors: Vec<String>,
    },
    Failed {
        message: String,
    },
}

#[derive(Clone)]
struct IndexRequest {
    folders: Vec<ScannedFolder>,
    node_id: Vec<u8>,
    max_items: usize,
    max_directories: usize,
    ignored_directory_names: Vec<String>,
    max_file_size_bytes: u64,
    trigger: ScanTrigger,
}

/// Runs all blocking filesystem traversal, hashing, and SQLite index writes away
/// from the UI task. Requests are processed in order so explicit user scans are
/// never lost behind a watcher-triggered scan.
pub struct IndexerWorker {
    requests: StdSender<IndexRequest>,
}

impl IndexerWorker {
    pub fn start(database_path: PathBuf, events: Sender<IndexerEvent>) -> Self {
        let (requests, receiver) = mpsc::channel::<IndexRequest>();
        thread::Builder::new()
            .name("puppydrive-indexer".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    log::info!(
                        "indexer worker received scan request for folders {:?}",
                        request
                            .folders
                            .iter()
                            .map(|folder| folder.id)
                            .collect::<Vec<_>>()
                    );
                    let result = index(&database_path, &events, request);
                    if let Err(error) = result {
                        log::error!("indexer worker request failed: {error:#}");
                        let _ = events.try_send(IndexerEvent::Failed {
                            message: format!("{error:#}"),
                        });
                    }
                }
            })
            .expect("failed starting PuppyDrive indexer thread");
        Self { requests }
    }

    pub fn request_scan(
        &self,
        folders: Vec<ScannedFolder>,
        node_id: Vec<u8>,
        max_items: usize,
        max_directories: usize,
        ignored_directory_names: Vec<String>,
        max_file_size_bytes: u64,
        trigger: ScanTrigger,
    ) {
        let request = IndexRequest {
            folders,
            node_id,
            max_items,
            max_directories,
            ignored_directory_names,
            max_file_size_bytes,
            trigger,
        };
        if self.requests.send(request).is_err() {
            log::error!("PuppyDrive indexer thread has stopped");
        }
    }
}

fn index(
    database_path: &Path,
    events: &Sender<IndexerEvent>,
    request: IndexRequest,
) -> anyhow::Result<()> {
    let started_at = system_time_millis(SystemTime::now());
    let active = request
        .folders
        .iter()
        .filter(|folder| folder.enabled && folder.indexes_media())
        .cloned()
        .collect::<Vec<_>>();
    log::info!(
        "indexer processing {} active folders: {}",
        active.len(),
        active
            .iter()
            .map(|folder| format!("{} ({})", folder.id, folder.path))
            .collect::<Vec<_>>()
            .join(", ")
    );
    let _ = events.try_send(IndexerEvent::Started {
        folder_ids: active.iter().map(|folder| folder.id).collect(),
    });
    let folders = active
        .iter()
        .filter_map(
            |folder| match ManagedFolder::open(folder.id, &folder.path) {
                Ok(folder) => Some((folder.id(), folder)),
                Err(error) => {
                    log::warn!(
                        "indexer cannot open scanned folder {} ({}): {error:#}",
                        folder.id,
                        folder.path
                    );
                    let _ = events.try_send(IndexerEvent::Failed {
                        message: format!("{}: {error:#}", folder.path),
                    });
                    None
                }
            },
        )
        .collect::<HashMap<_, _>>();
    let scan = scan_media(
        &active,
        &folders,
        request.max_items,
        request.max_directories,
        &request.ignored_directory_names,
        request.max_file_size_bytes,
        events,
    );
    log::info!(
        "indexer traversal finished: {} folder observations, truncated: {}, errors: {}",
        scan.observations.len(),
        scan.truncated,
        scan.errors.len()
    );
    let database = Database::open(database_path)?;
    let finished_at = system_time_millis(SystemTime::now());
    for folder in &active {
        let folder_id = folder.id;
        let available = folders.contains_key(&folder_id);
        let observations = scan
            .observations
            .get(&folder_id)
            .map_or(&[][..], Vec::as_slice);
        if available {
            log::debug!(
                "indexer writing {} observations for scanned folder {folder_id}",
                observations.len()
            );
            let folder_complete =
                !scan.truncated && scan.folder_errors.get(&folder_id).is_none_or(Vec::is_empty);
            database.sync_media_scan(&request.node_id, folder_id, observations, folder_complete)?;
        }
        let mut messages = scan
            .folder_errors
            .get(&folder_id)
            .cloned()
            .unwrap_or_default();
        if scan.truncated {
            messages.push("Scan limit reached".to_owned());
        }
        let outcome = if !available {
            ScanOutcome::Failed
        } else if scan.truncated || !messages.is_empty() {
            ScanOutcome::Incomplete
        } else {
            ScanOutcome::Completed
        };
        let history = database.save_scanned_folder_scan(ScanHistoryEntry {
            scanned_folder_id: folder_id,
            trigger: request.trigger,
            outcome,
            started_at,
            finished_at,
            directories_scanned: *scan.folder_directories.get(&folder_id).unwrap_or(&0),
            files_indexed: *scan.folder_files.get(&folder_id).unwrap_or(&0),
            error_message: (!messages.is_empty()).then(|| messages.join("; ")),
        })?;
        let _ = events.try_send(IndexerEvent::FolderFinished { history });
    }
    let _ = events.try_send(IndexerEvent::Finished {
        truncated: scan.truncated,
        errors: scan.errors,
    });
    Ok(())
}

struct ScanResult {
    observations: HashMap<u32, Vec<MediaIndexObservation>>,
    folder_directories: HashMap<u32, usize>,
    folder_files: HashMap<u32, usize>,
    truncated: bool,
    errors: Vec<String>,
    folder_errors: HashMap<u32, Vec<String>>,
}

fn scan_media(
    roots: &[ScannedFolder],
    folders: &HashMap<u32, ManagedFolder>,
    max_items: usize,
    max_directories: usize,
    ignored_directory_names: &[String],
    max_file_size_bytes: u64,
    events: &Sender<IndexerEvent>,
) -> ScanResult {
    let mut queue = VecDeque::new();
    let mut observations: HashMap<u32, Vec<MediaIndexObservation>> = HashMap::new();
    let mut errors = Vec::new();
    let mut folder_errors = HashMap::<u32, Vec<String>>::new();
    let mut truncated = false;
    for root in roots {
        let Some(folder) = folders.get(&root.id) else {
            let error = format!("{} is unavailable", root.path);
            errors.push(error.clone());
            folder_errors.entry(root.id).or_default().push(error);
            continue;
        };
        queue.push_back((folder.clone(), folder.root().to_path_buf()));
        observations.entry(root.id).or_default();
    }
    let mut visited = HashSet::new();
    let mut media_files = 0;
    let mut folder_directories = HashMap::<u32, usize>::new();
    let mut folder_files = HashMap::<u32, usize>::new();
    while let Some((folder, directory)) = queue.pop_front() {
        if media_files >= max_items || visited.len() >= max_directories {
            truncated = true;
            break;
        }
        let Ok(directory) = folder.canonicalize(directory) else {
            continue;
        };
        if !visited.insert(directory.clone()) {
            continue;
        }
        let directories_scanned = folder_directories.entry(folder.id()).or_default();
        *directories_scanned += 1;
        let Ok(mut entries) = folder.read_dir(&directory) else {
            let error = format!("{} cannot be read", directory.display());
            errors.push(error.clone());
            folder_errors.entry(folder.id()).or_default().push(error);
            continue;
        };
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());
        for entry in entries {
            if media_files >= max_items {
                truncated = true;
                break;
            }
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let ignored = name.starts_with('.')
                    || ignored_directory_names
                        .iter()
                        .any(|ignored| ignored.eq_ignore_ascii_case(&name));
                if !ignored && visited.len() + queue.len() < max_directories {
                    queue.push_back((folder.clone(), path));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let mime_type = file_mime_type(&path).to_owned();
            let Ok(path) = folder.canonicalize(path) else {
                continue;
            };
            let Ok(metadata) = folder.metadata(&path) else {
                continue;
            };
            if max_file_size_bytes > 0 && metadata.len() > max_file_size_bytes {
                log::debug!(
                    "indexer skipping {} because it exceeds the {} byte size limit",
                    path.display(),
                    max_file_size_bytes
                );
                continue;
            }
            let current_path = path.display().to_string();
            observations
                .entry(folder.id())
                .or_default()
                .push(MediaIndexObservation {
                    hash: folder.blake3(&path).ok(),
                    path,
                    size: metadata.len(),
                    mime_type: Some(mime_type),
                    created_at: metadata.created().ok().map(system_time_millis),
                    modified_at: metadata.modified().ok().map(system_time_millis),
                    accessed_at: metadata.accessed().ok().map(system_time_millis),
                });
            media_files += 1;
            let files_indexed = folder_files.entry(folder.id()).or_default();
            *files_indexed += 1;
            if *files_indexed == 1 || files_indexed.is_multiple_of(10) {
                let _ = events.try_send(IndexerEvent::Progress {
                    folder_id: folder.id(),
                    directories_scanned: *folder_directories.get(&folder.id()).unwrap_or(&0),
                    media_files_indexed: *files_indexed,
                    current_path: Some(current_path),
                });
            }
        }
        if visited.len().is_multiple_of(8) {
            let _ = events.try_send(IndexerEvent::Progress {
                folder_id: folder.id(),
                directories_scanned: *folder_directories.get(&folder.id()).unwrap_or(&0),
                media_files_indexed: *folder_files.get(&folder.id()).unwrap_or(&0),
                current_path: Some(directory.display().to_string()),
            });
        }
    }
    ScanResult {
        observations,
        folder_directories,
        folder_files,
        truncated,
        errors,
        folder_errors,
    }
}

fn file_mime_type(path: &Path) -> &'static str {
    let Some(extension) = path.extension().and_then(|extension| extension.to_str()) else {
        return "application/octet-stream";
    };
    if extension.eq_ignore_ascii_case("png") {
        "image/png"
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        "image/jpeg"
    } else if extension.eq_ignore_ascii_case("gif") {
        "image/gif"
    } else if extension.eq_ignore_ascii_case("webp") {
        "image/webp"
    } else if extension.eq_ignore_ascii_case("bmp") {
        "image/bmp"
    } else if extension.eq_ignore_ascii_case("avif") {
        "image/avif"
    } else if extension.eq_ignore_ascii_case("svg") {
        "image/svg+xml"
    } else if extension.eq_ignore_ascii_case("ico") {
        "image/x-icon"
    } else if extension.eq_ignore_ascii_case("mp4") {
        "video/mp4"
    } else if extension.eq_ignore_ascii_case("webm") {
        "video/webm"
    } else if extension.eq_ignore_ascii_case("ogv") || extension.eq_ignore_ascii_case("ogg") {
        "video/ogg"
    } else if extension.eq_ignore_ascii_case("mov") {
        "video/quicktime"
    } else if extension.eq_ignore_ascii_case("m4v") {
        "video/x-m4v"
    } else if extension.eq_ignore_ascii_case("mkv") {
        "video/x-matroska"
    } else if extension.eq_ignore_ascii_case("txt") || extension.eq_ignore_ascii_case("md") {
        "text/plain"
    } else if extension.eq_ignore_ascii_case("json") {
        "application/json"
    } else if extension.eq_ignore_ascii_case("pdf") {
        "application/pdf"
    } else {
        "application/octet-stream"
    }
}

fn system_time_millis(time: SystemTime) -> i64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_indexes_files_and_filters_media() {
        let directory =
            std::env::temp_dir().join(format!("puppydrive-indexer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("photo.jpg"), b"photo").unwrap();
        std::fs::write(directory.join("notes.txt"), b"notes").unwrap();
        std::fs::create_dir_all(directory.join("node_modules")).unwrap();
        std::fs::write(directory.join("node_modules/skip.txt"), b"skip").unwrap();
        let database_path =
            std::env::temp_dir().join(format!("puppydrive-indexer-{}.db", uuid::Uuid::new_v4()));
        let database = Database::open(&database_path).unwrap();
        let folder = database
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: directory.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let node_id = database.local_node_id("PuppyDrive").unwrap();
        let folder_id = folder.id;
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(32);
        let worker = IndexerWorker::start(database_path.clone(), events_tx);
        worker.request_scan(
            vec![folder],
            node_id.clone(),
            10,
            10,
            vec!["node_modules".to_owned()],
            0,
            ScanTrigger::ManualFolder,
        );

        let finished = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(event) = events_rx.recv().await {
                if matches!(event, IndexerEvent::Finished { .. }) {
                    return true;
                }
            }
            false
        })
        .await
        .unwrap();
        assert!(finished);
        assert_eq!(database.cached_media(&node_id).unwrap().len(), 1);
        assert_eq!(database.cached_files(&node_id).unwrap().len(), 2);
        let history = database.scanned_folder_scan_history(folder_id).unwrap();
        assert_eq!(history.len(), 1);
        assert_eq!(history[0].trigger, ScanTrigger::ManualFolder);
        assert_eq!(history[0].outcome, ScanOutcome::Completed);
        assert_eq!(history[0].directories_scanned, 1);
        assert_eq!(history[0].files_indexed, 2);
        drop(worker);
        drop(database);
        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_file(database_path);
    }

    #[tokio::test]
    async fn worker_records_incomplete_and_unavailable_folder_scans() {
        let directory =
            std::env::temp_dir().join(format!("puppydrive-indexer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("first.jpg"), b"first").unwrap();
        std::fs::write(directory.join("second.txt"), b"second").unwrap();
        let missing_directory = directory.with_extension("missing");
        let database_path =
            std::env::temp_dir().join(format!("puppydrive-indexer-{}.db", uuid::Uuid::new_v4()));
        let database = Database::open(&database_path).unwrap();
        let complete_folder = database
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: directory.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let unavailable_folder = database
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: missing_directory.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let complete_folder_id = complete_folder.id;
        let unavailable_folder_id = unavailable_folder.id;
        let node_id = database.local_node_id("PuppyDrive").unwrap();
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(32);
        let worker = IndexerWorker::start(database_path.clone(), events_tx);
        worker.request_scan(
            vec![complete_folder, unavailable_folder],
            node_id,
            1,
            10,
            Vec::new(),
            0,
            ScanTrigger::FilesystemChange,
        );
        tokio::time::timeout(std::time::Duration::from_secs(5), async {
            while let Some(event) = events_rx.recv().await {
                if matches!(event, IndexerEvent::Finished { .. }) {
                    return;
                }
            }
        })
        .await
        .unwrap();
        let incomplete = database
            .scanned_folder_scan_history(complete_folder_id)
            .unwrap();
        assert_eq!(incomplete[0].outcome, ScanOutcome::Incomplete);
        assert_eq!(
            incomplete[0].error_message.as_deref(),
            Some("Scan limit reached")
        );
        let unavailable = database
            .scanned_folder_scan_history(unavailable_folder_id)
            .unwrap();
        assert_eq!(unavailable[0].outcome, ScanOutcome::Failed);
        assert!(
            unavailable[0]
                .error_message
                .as_deref()
                .is_some_and(|message| message.contains("unavailable"))
        );
        drop(worker);
        drop(database);
        let _ = std::fs::remove_dir_all(directory);
        let _ = std::fs::remove_file(database_path);
    }
}
