use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender as StdSender};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime};

use tokio::sync::mpsc::Sender;

use crate::database::{
    Database, IndexedLocationMetadata, MediaIndexObservation, ScanHistoryEntry, ScanOutcome,
    ScanTrigger, ScannedFolder,
};
use crate::managed_folder::{Blake3Hash, ManagedFolder};

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
    cancellations: HashMap<u32, Arc<AtomicBool>>,
}

/// Runs all blocking filesystem traversal, hashing, and SQLite index writes away
/// from the UI task. Requests are processed in order so explicit user scans are
/// never lost behind a watcher-triggered scan.
pub struct IndexerWorker {
    requests: StdSender<IndexRequest>,
    cancellations: Arc<Mutex<HashMap<u32, Arc<AtomicBool>>>>,
}

impl IndexerWorker {
    pub fn start(database_path: PathBuf, events: Sender<IndexerEvent>) -> Self {
        let (requests, receiver) = mpsc::channel::<IndexRequest>();
        let cancellations = Arc::new(Mutex::new(HashMap::<u32, Arc<AtomicBool>>::new()));
        let worker_cancellations = cancellations.clone();
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
                    let request_cancellations = request.cancellations.clone();
                    let result = index(&database_path, &events, request);
                    let mut active_cancellations = worker_cancellations
                        .lock()
                        .expect("indexer cancellation lock poisoned");
                    for (folder_id, cancellation) in request_cancellations {
                        if active_cancellations
                            .get(&folder_id)
                            .is_some_and(|active| Arc::ptr_eq(active, &cancellation))
                        {
                            active_cancellations.remove(&folder_id);
                        }
                    }
                    drop(active_cancellations);
                    if let Err(error) = result {
                        log::error!("indexer worker request failed: {error:#}");
                        let _ = events.try_send(IndexerEvent::Failed {
                            message: format!("{error:#}"),
                        });
                    }
                }
            })
            .expect("failed starting PuppyDrive indexer thread");
        Self {
            requests,
            cancellations,
        }
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
        let cancellations = folders
            .iter()
            .map(|folder| (folder.id, Arc::new(AtomicBool::new(false))))
            .collect::<HashMap<_, _>>();
        self.cancellations
            .lock()
            .expect("indexer cancellation lock poisoned")
            .extend(cancellations.clone());
        let request = IndexRequest {
            folders,
            node_id,
            max_items,
            max_directories,
            ignored_directory_names,
            max_file_size_bytes,
            trigger,
            cancellations,
        };
        if self.requests.send(request).is_err() {
            log::error!("PuppyDrive indexer thread has stopped");
        }
    }

    pub fn cancel_scan(&self, folder_id: u32) -> bool {
        let Some(cancellation) = self
            .cancellations
            .lock()
            .expect("indexer cancellation lock poisoned")
            .get(&folder_id)
            .cloned()
        else {
            return false;
        };
        cancellation.store(true, Ordering::Relaxed);
        true
    }
}

fn index(
    database_path: &Path,
    events: &Sender<IndexerEvent>,
    request: IndexRequest,
) -> anyhow::Result<()> {
    let scan_started = Instant::now();
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
    let database = Database::open(database_path)?;
    let metadata_lookup_started = Instant::now();
    let previous_locations = active
        .iter()
        .map(|folder| {
            database
                .scanned_folder_location_metadata(&request.node_id, folder.id)
                .map(|locations| (folder.id, locations))
        })
        .collect::<anyhow::Result<HashMap<_, _>>>()?;
    let metadata_lookup_duration = metadata_lookup_started.elapsed();
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
        &previous_locations,
        &request.cancellations,
        events,
    );
    log::info!(
        "indexer traversal finished: {} folder observations, truncated: {}, errors: {}",
        scan.observations.len(),
        scan.truncated,
        scan.errors.len()
    );
    let finished_at = system_time_millis(SystemTime::now());
    let database_write_started = Instant::now();
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
            let folder_complete = !scan.truncated
                && !scan.cancelled_folders.contains(&folder_id)
                && scan.folder_errors.get(&folder_id).is_none_or(Vec::is_empty);
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
        if scan.cancelled_folders.contains(&folder_id) {
            messages.push("Scan stopped by user".to_owned());
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
    let database_write_duration = database_write_started.elapsed();
    log::info!(
        "indexer scan telemetry: total={:?}, metadata_lookup={:?}, traversal={:?}, hashing_wall={:?} (read_total={:?}, hash_update_total={:?}, open_total={:?}; {} files, {} bytes; workers peak={}, final={}), reused_hashes={}, database_write={:?}",
        scan_started.elapsed(),
        metadata_lookup_duration,
        scan.traversal_duration,
        scan.hashing_duration,
        scan.file_read_duration,
        scan.hash_update_duration,
        scan.file_open_duration,
        scan.files_hashed,
        scan.bytes_hashed,
        scan.hash_workers_peak,
        scan.hash_workers_final,
        scan.reused_hashes,
        database_write_duration,
    );
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
    cancelled_folders: HashSet<u32>,
    truncated: bool,
    errors: Vec<String>,
    folder_errors: HashMap<u32, Vec<String>>,
    traversal_duration: Duration,
    hashing_duration: Duration,
    files_hashed: usize,
    bytes_hashed: u64,
    reused_hashes: usize,
    hash_workers_peak: usize,
    hash_workers_final: usize,
    file_open_duration: Duration,
    file_read_duration: Duration,
    hash_update_duration: Duration,
}

const HASH_BATCH_SIZE: usize = 32;
const MAX_HASH_WORKERS: usize = 4;

struct HashCandidate {
    folder: ManagedFolder,
    observation: MediaIndexObservation,
    current_path: String,
}

struct HashBatchResult {
    candidates: Vec<(HashCandidate, Option<Blake3Hash>)>,
    duration: Duration,
    bytes_hashed: u64,
    files_hashed: usize,
    file_open_duration: Duration,
    file_read_duration: Duration,
    hash_update_duration: Duration,
}

struct AdaptiveHashPool {
    workers: usize,
    max_workers: usize,
    peak_workers: usize,
    best_throughput: Option<f64>,
}

impl AdaptiveHashPool {
    fn new() -> Self {
        let max_workers = thread::available_parallelism()
            .map(|parallelism| parallelism.get())
            .unwrap_or(1)
            .clamp(1, MAX_HASH_WORKERS);
        Self {
            workers: 1,
            max_workers,
            peak_workers: 1,
            best_throughput: None,
        }
    }

    fn observe(&mut self, batch: &HashBatchResult) {
        if batch.bytes_hashed < 1_048_576 || batch.duration.is_zero() {
            return;
        }
        let throughput = batch.bytes_hashed as f64 / batch.duration.as_secs_f64();
        let best = self.best_throughput.unwrap_or(throughput);
        if self.workers == 1 && self.max_workers > 1 {
            self.workers = 2;
        } else if throughput > best * 1.10 && self.workers < self.max_workers {
            self.workers += 1;
        } else if throughput < best * 0.85 && self.workers > 1 {
            self.workers -= 1;
        }
        self.peak_workers = self.peak_workers.max(self.workers);
        self.best_throughput = Some(best.max(throughput));
    }
}

fn scan_media(
    roots: &[ScannedFolder],
    folders: &HashMap<u32, ManagedFolder>,
    max_items: usize,
    max_directories: usize,
    ignored_directory_names: &[String],
    max_file_size_bytes: u64,
    previous_locations: &HashMap<u32, HashMap<PathBuf, IndexedLocationMetadata>>,
    cancellations: &HashMap<u32, Arc<AtomicBool>>,
    events: &Sender<IndexerEvent>,
) -> ScanResult {
    let traversal_started = Instant::now();
    let mut queue = VecDeque::new();
    let mut observations: HashMap<u32, Vec<MediaIndexObservation>> = HashMap::new();
    let mut errors = Vec::new();
    let mut folder_errors = HashMap::<u32, Vec<String>>::new();
    let mut truncated = false;
    let mut hashing_duration = Duration::ZERO;
    let mut files_hashed = 0;
    let mut bytes_hashed = 0;
    let mut reused_hashes = 0;
    let mut file_open_duration = Duration::ZERO;
    let mut file_read_duration = Duration::ZERO;
    let mut hash_update_duration = Duration::ZERO;
    let mut accepted_files = 0;
    let mut pending_hashes = Vec::new();
    let mut hash_pool = AdaptiveHashPool::new();
    let mut cancelled_folders = HashSet::new();
    for root in roots {
        observations.entry(root.id).or_default();
        if is_cancelled(cancellations, root.id) {
            cancelled_folders.insert(root.id);
            continue;
        }
        let Some(folder) = folders.get(&root.id) else {
            let error = format!("{} is unavailable", root.path);
            errors.push(error.clone());
            folder_errors.entry(root.id).or_default().push(error);
            continue;
        };
        queue.push_back((folder.clone(), folder.root().to_path_buf()));
    }
    let mut visited = HashSet::new();
    let mut folder_directories = HashMap::<u32, usize>::new();
    let mut folder_files = HashMap::<u32, usize>::new();
    while let Some((folder, directory)) = queue.pop_front() {
        if is_cancelled(cancellations, folder.id()) {
            cancelled_folders.insert(folder.id());
            continue;
        }
        if (max_items > 0 && accepted_files >= max_items)
            || (max_directories > 0 && visited.len() >= max_directories)
        {
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
            if is_cancelled(cancellations, folder.id()) {
                cancelled_folders.insert(folder.id());
                break;
            }
            if max_items > 0 && accepted_files >= max_items {
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
                if !ignored
                    && (max_directories == 0 || visited.len() + queue.len() < max_directories)
                {
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
            let created_at = metadata.created().ok().map(system_time_millis);
            let modified_at = metadata.modified().ok().map(system_time_millis);
            let reused_hash = reusable_hash(
                previous_locations
                    .get(&folder.id())
                    .and_then(|locations| locations.get(&path)),
                metadata.len(),
                created_at,
                modified_at,
            );
            let observation = MediaIndexObservation {
                hash: None,
                path,
                size: metadata.len(),
                mime_type: Some(mime_type),
                created_at,
                modified_at,
                accessed_at: metadata.accessed().ok().map(system_time_millis),
            };
            accepted_files += 1;
            if let Some(hash) = reused_hash {
                reused_hashes += 1;
                let mut observation = observation;
                observation.hash = Some(hash);
                record_observation(
                    folder.id(),
                    observation,
                    current_path,
                    &mut observations,
                    &folder_directories,
                    &mut folder_files,
                    events,
                );
            } else {
                pending_hashes.push(HashCandidate {
                    folder: folder.clone(),
                    observation,
                    current_path,
                });
                if pending_hashes.len() >= HASH_BATCH_SIZE {
                    let batch = flush_hashes(
                        &mut pending_hashes,
                        &mut hash_pool,
                        &mut observations,
                        &folder_directories,
                        &mut folder_files,
                        cancellations,
                        &mut cancelled_folders,
                        events,
                    );
                    hashing_duration += batch.duration;
                    files_hashed += batch.files_hashed;
                    bytes_hashed += batch.bytes_hashed;
                    file_open_duration += batch.file_open_duration;
                    file_read_duration += batch.file_read_duration;
                    hash_update_duration += batch.hash_update_duration;
                }
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
    let batch = flush_hashes(
        &mut pending_hashes,
        &mut hash_pool,
        &mut observations,
        &folder_directories,
        &mut folder_files,
        cancellations,
        &mut cancelled_folders,
        events,
    );
    hashing_duration += batch.duration;
    files_hashed += batch.files_hashed;
    bytes_hashed += batch.bytes_hashed;
    file_open_duration += batch.file_open_duration;
    file_read_duration += batch.file_read_duration;
    hash_update_duration += batch.hash_update_duration;
    ScanResult {
        observations,
        folder_directories,
        folder_files,
        cancelled_folders,
        truncated,
        errors,
        folder_errors,
        traversal_duration: traversal_started.elapsed(),
        hashing_duration,
        files_hashed,
        bytes_hashed,
        reused_hashes,
        hash_workers_peak: hash_pool.peak_workers,
        hash_workers_final: hash_pool.workers,
        file_open_duration,
        file_read_duration,
        hash_update_duration,
    }
}

fn flush_hashes(
    pending: &mut Vec<HashCandidate>,
    pool: &mut AdaptiveHashPool,
    observations: &mut HashMap<u32, Vec<MediaIndexObservation>>,
    folder_directories: &HashMap<u32, usize>,
    folder_files: &mut HashMap<u32, usize>,
    cancellations: &HashMap<u32, Arc<AtomicBool>>,
    cancelled_folders: &mut HashSet<u32>,
    events: &Sender<IndexerEvent>,
) -> HashBatchResult {
    pending.retain(|candidate| {
        let cancelled = is_cancelled(cancellations, candidate.folder.id());
        if cancelled {
            cancelled_folders.insert(candidate.folder.id());
        }
        !cancelled
    });
    if pending.is_empty() {
        return HashBatchResult {
            candidates: Vec::new(),
            duration: Duration::ZERO,
            bytes_hashed: 0,
            files_hashed: 0,
            file_open_duration: Duration::ZERO,
            file_read_duration: Duration::ZERO,
            hash_update_duration: Duration::ZERO,
        };
    }
    let batch = hash_candidates(std::mem::take(pending), pool.workers);
    for (candidate, hash) in batch.candidates.iter() {
        let mut observation = candidate.observation.clone();
        observation.hash = hash.as_ref().map(|hash| hash.hash.clone());
        record_observation(
            candidate.folder.id(),
            observation,
            candidate.current_path.clone(),
            observations,
            folder_directories,
            folder_files,
            events,
        );
    }
    pool.observe(&batch);
    batch
}

fn is_cancelled(cancellations: &HashMap<u32, Arc<AtomicBool>>, folder_id: u32) -> bool {
    cancellations
        .get(&folder_id)
        .is_some_and(|cancellation| cancellation.load(Ordering::Relaxed))
}

fn hash_candidates(candidates: Vec<HashCandidate>, workers: usize) -> HashBatchResult {
    let workers = workers.clamp(1, candidates.len());
    let queue = Mutex::new(VecDeque::from(candidates));
    let results = Mutex::new(Vec::new());
    let started = Instant::now();
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| {
                loop {
                    let Some(candidate) =
                        queue.lock().expect("hash queue lock poisoned").pop_front()
                    else {
                        break;
                    };
                    let hash = candidate
                        .folder
                        .blake3_timed(&candidate.observation.path)
                        .ok();
                    results
                        .lock()
                        .expect("hash results lock poisoned")
                        .push((candidate, hash));
                }
            });
        }
    });
    let candidates = results.into_inner().expect("hash results lock poisoned");
    let (files_hashed, bytes_hashed, file_open_duration, file_read_duration, hash_update_duration) =
        candidates.iter().fold(
            (0, 0, Duration::ZERO, Duration::ZERO, Duration::ZERO),
            |(files, bytes, open, read, update), (candidate, hash)| match hash {
                Some(hash) => (
                    files + 1,
                    bytes + candidate.observation.size,
                    open + hash.open_duration,
                    read + hash.read_duration,
                    update + hash.update_duration,
                ),
                None => (files, bytes, open, read, update),
            },
        );
    HashBatchResult {
        candidates,
        duration: started.elapsed(),
        bytes_hashed,
        files_hashed,
        file_open_duration,
        file_read_duration,
        hash_update_duration,
    }
}

fn record_observation(
    folder_id: u32,
    observation: MediaIndexObservation,
    current_path: String,
    observations: &mut HashMap<u32, Vec<MediaIndexObservation>>,
    folder_directories: &HashMap<u32, usize>,
    folder_files: &mut HashMap<u32, usize>,
    events: &Sender<IndexerEvent>,
) {
    observations.entry(folder_id).or_default().push(observation);
    let files_indexed = folder_files.entry(folder_id).or_default();
    *files_indexed += 1;
    if *files_indexed == 1 || files_indexed.is_multiple_of(10) {
        let _ = events.try_send(IndexerEvent::Progress {
            folder_id,
            directories_scanned: *folder_directories.get(&folder_id).unwrap_or(&0),
            media_files_indexed: *files_indexed,
            current_path: Some(current_path),
        });
    }
}

fn reusable_hash(
    previous: Option<&IndexedLocationMetadata>,
    size: u64,
    created_at: Option<i64>,
    modified_at: Option<i64>,
) -> Option<Vec<u8>> {
    let previous = previous?;
    (previous.size == size
        && previous.created_at == created_at
        && previous.modified_at == modified_at)
        .then(|| previous.hash.clone())
        .flatten()
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

    #[test]
    fn unchanged_metadata_reuses_the_previous_hash() {
        let metadata = IndexedLocationMetadata {
            hash: Some(vec![4; 32]),
            size: 128,
            created_at: Some(10),
            modified_at: Some(20),
        };
        assert_eq!(
            reusable_hash(Some(&metadata), 128, Some(10), Some(20)),
            metadata.hash
        );
        assert!(reusable_hash(Some(&metadata), 129, Some(10), Some(20)).is_none());
        assert!(reusable_hash(Some(&metadata), 128, Some(10), Some(21)).is_none());
    }

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

    #[tokio::test]
    #[ignore = "set PUPPYDRIVE_PROFILE_ROOT to run a manual scan profile"]
    async fn profiles_configured_scan_root_twice() {
        let _ = simple_logger::SimpleLogger::new().init();
        let root = std::env::var_os("PUPPYDRIVE_PROFILE_ROOT")
            .map(PathBuf::from)
            .expect("set PUPPYDRIVE_PROFILE_ROOT to the folder to profile");
        assert!(
            root.is_dir(),
            "profile root is not a directory: {}",
            root.display()
        );
        let database_path =
            std::env::temp_dir().join(format!("puppydrive-profile-{}.db", uuid::Uuid::new_v4()));
        let database = Database::open(&database_path).unwrap();
        let folder = database
            .save_scanned_folder(ScannedFolder {
                id: 0,
                path: root.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let node_id = database.local_node_id("PuppyDrive profile").unwrap();
        let (events_tx, mut events_rx) = tokio::sync::mpsc::channel(256);
        let worker = IndexerWorker::start(database_path.clone(), events_tx);
        for pass in 1..=2 {
            eprintln!("starting profile scan pass {pass}");
            worker.request_scan(
                vec![folder.clone()],
                node_id.clone(),
                1_000,
                512,
                vec![
                    "node_modules".to_owned(),
                    ".git".to_owned(),
                    "target".to_owned(),
                ],
                1_024 * 1_024 * 1_024,
                ScanTrigger::ManualFolder,
            );
            let finished = tokio::time::timeout(std::time::Duration::from_secs(20 * 60), async {
                while let Some(event) = events_rx.recv().await {
                    if let IndexerEvent::Finished { truncated, errors } = event {
                        return (truncated, errors);
                    }
                }
                panic!("indexer event channel closed before the scan completed");
            })
            .await
            .expect("profile scan timed out");
            eprintln!(
                "profile scan pass {pass} complete: truncated={}, errors={}",
                finished.0,
                finished.1.len()
            );
        }
        let history = database.scanned_folder_scan_history(folder.id).unwrap();
        eprintln!("profile history: {history:#?}");
        drop(worker);
        drop(database);
        let _ = std::fs::remove_file(database_path);
    }
}
