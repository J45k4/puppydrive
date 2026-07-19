use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, SyncSender, TrySendError};
use std::thread;
use std::time::SystemTime;

use tokio::sync::mpsc::Sender;

use crate::database::{Database, MediaIndexObservation, ScannedFolder};
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
    },
    FolderFinished {
        folder_id: u32,
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
}

/// Runs all blocking filesystem traversal, hashing, and SQLite index writes away
/// from the UI task. The one-slot queue coalesces bursts of watcher events.
pub struct IndexerWorker {
    requests: SyncSender<IndexRequest>,
}

impl IndexerWorker {
    pub fn start(database_path: PathBuf, events: Sender<IndexerEvent>) -> Self {
        let (requests, receiver) = mpsc::sync_channel(1);
        thread::Builder::new()
            .name("puppydrive-indexer".to_owned())
            .spawn(move || {
                while let Ok(request) = receiver.recv() {
                    let result = index(&database_path, &events, request);
                    if let Err(error) = result {
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
    ) {
        let request = IndexRequest {
            folders,
            node_id,
            max_items,
            max_directories,
        };
        match self.requests.try_send(request) {
            Ok(()) | Err(TrySendError::Full(_)) => {}
            Err(TrySendError::Disconnected(_)) => {
                log::error!("PuppyDrive indexer thread has stopped");
            }
        }
    }
}

fn index(
    database_path: &Path,
    events: &Sender<IndexerEvent>,
    request: IndexRequest,
) -> anyhow::Result<()> {
    let active = request
        .folders
        .iter()
        .filter(|folder| folder.enabled && folder.indexes_media())
        .cloned()
        .collect::<Vec<_>>();
    let _ = events.try_send(IndexerEvent::Started {
        folder_ids: active.iter().map(|folder| folder.id).collect(),
    });
    let folders = active
        .iter()
        .filter_map(
            |folder| match ManagedFolder::open(folder.id, &folder.path) {
                Ok(folder) => Some((folder.id(), folder)),
                Err(error) => {
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
        events,
    );
    let database = Database::open(database_path)?;
    let complete = !scan.truncated && scan.errors.is_empty();
    for (folder_id, observations) in &scan.observations {
        database.sync_media_scan(&request.node_id, *folder_id, observations, complete)?;
        let _ = events.try_send(IndexerEvent::FolderFinished {
            folder_id: *folder_id,
        });
    }
    let _ = events.try_send(IndexerEvent::Finished {
        truncated: scan.truncated,
        errors: scan.errors,
    });
    Ok(())
}

struct ScanResult {
    observations: HashMap<u32, Vec<MediaIndexObservation>>,
    truncated: bool,
    errors: Vec<String>,
}

fn scan_media(
    roots: &[ScannedFolder],
    folders: &HashMap<u32, ManagedFolder>,
    max_items: usize,
    max_directories: usize,
    events: &Sender<IndexerEvent>,
) -> ScanResult {
    let mut queue = VecDeque::new();
    let mut observations: HashMap<u32, Vec<MediaIndexObservation>> = HashMap::new();
    let mut errors = Vec::new();
    let mut truncated = false;
    for root in roots {
        let Some(folder) = folders.get(&root.id) else {
            errors.push(format!("{} is unavailable", root.path));
            continue;
        };
        queue.push_back((folder.clone(), folder.root().to_path_buf()));
        observations.entry(root.id).or_default();
    }
    let mut visited = HashSet::new();
    let mut media_files = 0;
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
        let Ok(mut entries) = folder.read_dir(&directory) else {
            errors.push(format!("{} cannot be read", directory.display()));
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
                let hidden = entry.file_name().to_string_lossy().starts_with('.');
                if !hidden && visited.len() + queue.len() < max_directories {
                    queue.push_back((folder.clone(), path));
                }
                continue;
            }
            if !file_type.is_file() {
                continue;
            }
            let mime_type = media_mime_type(&path).map(str::to_owned);
            let Some(mime_type) = mime_type else {
                continue;
            };
            let Ok(path) = folder.canonicalize(path) else {
                continue;
            };
            let Ok(metadata) = folder.metadata(&path) else {
                continue;
            };
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
        }
        if visited.len().is_multiple_of(8) {
            let _ = events.try_send(IndexerEvent::Progress {
                folder_id: folder.id(),
                directories_scanned: visited.len(),
                media_files_indexed: media_files,
            });
        }
    }
    ScanResult {
        observations,
        truncated,
        errors,
    }
}

fn media_mime_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("png") {
        Some("image/png")
    } else if extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg") {
        Some("image/jpeg")
    } else if extension.eq_ignore_ascii_case("gif") {
        Some("image/gif")
    } else if extension.eq_ignore_ascii_case("webp") {
        Some("image/webp")
    } else if extension.eq_ignore_ascii_case("bmp") {
        Some("image/bmp")
    } else if extension.eq_ignore_ascii_case("avif") {
        Some("image/avif")
    } else if extension.eq_ignore_ascii_case("svg") {
        Some("image/svg+xml")
    } else if extension.eq_ignore_ascii_case("ico") {
        Some("image/x-icon")
    } else if extension.eq_ignore_ascii_case("mp4") {
        Some("video/mp4")
    } else if extension.eq_ignore_ascii_case("webm") {
        Some("video/webm")
    } else if extension.eq_ignore_ascii_case("ogv") || extension.eq_ignore_ascii_case("ogg") {
        Some("video/ogg")
    } else if extension.eq_ignore_ascii_case("mov") {
        Some("video/quicktime")
    } else if extension.eq_ignore_ascii_case("m4v") {
        Some("video/x-m4v")
    } else if extension.eq_ignore_ascii_case("mkv") {
        Some("video/x-matroska")
    } else {
        None
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
    async fn worker_indexes_media_and_emits_completion() {
        let directory =
            std::env::temp_dir().join(format!("puppydrive-indexer-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("photo.jpg"), b"photo").unwrap();
        let database_path = directory.join("index.db");
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
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(32);
        let worker = IndexerWorker::start(database_path, events_tx);
        worker.request_scan(vec![folder], node_id.clone(), 10, 10);

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
        drop(worker);
        drop(database);
        let _ = std::fs::remove_dir_all(directory);
    }
}
