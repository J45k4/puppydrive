#[cfg(test)]
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, Instant, SystemTime};

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use wgui::{
    ClientEvent, HttpResponse, Item, StaticAsset, Wgui, button, checkbox, custom_component, hstack,
    link, modal, option, select, slider, text, text_input, vstack,
};

use crate::config::{self, AppConfig, InboxConfig};
#[cfg(test)]
use crate::database::MediaIndexObservation;
use crate::database::{
    Database, IndexedFile, IndexedMediaFile, LocalSourceConfig, MediaScanPath, ScanHistoryEntry,
    ScanOutcome, ScanTrigger, Source, VirtualDirectory, VirtualDirectoryEntry, local_source_path,
    validate_source_config,
};
use crate::indexer::{IndexerEvent, IndexerWorker};
use crate::managed_folder::ManagedFolder;
use crate::session_secrets::SessionSecretStore;

const THIS_COMPUTER_SOURCE_ID: u32 = 20;
const LOCAL_PARENT_ID: u32 = 21;
const LOCAL_BREADCRUMB_ID: u32 = 23;
const LOCAL_TREE_TOGGLE_ID: u32 = 24;
const LOCAL_TREE_SELECT_ID: u32 = 25;
const LOCAL_VIDEO_VIEW_ID: u32 = 26;
const CLOSE_FILE_VIEWER_ID: u32 = 27;
const LOCAL_TEXT_VIEW_ID: u32 = 28;
const TOGGLE_FILE_VIEWER_SIZE_ID: u32 = 29;
const TEXT_VIEW_MODE_ID: u32 = 31;
const HEX_VIEW_MODE_ID: u32 = 32;
#[allow(dead_code)]
const FOLDER_ROW_COMPONENT_ID: u32 = 34;
const FOLDER_CONTEXT_ID: u32 = 35;
const CLOSE_FOLDER_CONTEXT_ID: u32 = 36;
const OPEN_FOLDER_CONTEXT_ID: u32 = 37;
const TOGGLE_FOLDER_CONTEXT_ID: u32 = 38;
const LOCAL_TREE_NAVIGATE_ID: u32 = 39;
const SHOW_ADD_SOURCE_ID: u32 = 40;
const CLOSE_ADD_SOURCE_ID: u32 = 41;
const SAVE_ADD_SOURCE_ID: u32 = 42;
const ADD_SOURCE_NAME_INPUT_ID: u32 = 43;
const ADD_SOURCE_PATH_INPUT_ID: u32 = 44;
const LOCAL_IMAGE_VIEW_ID: u32 = 45;
const LOCAL_MEDIA_VIEW_ID: u32 = 52;
const REFRESH_MEDIA_ID: u32 = 53;
const MEDIA_THUMBNAIL_SIZE_ID: u32 = 54;
const MEDIA_VIEW_MODE_ID: u32 = 55;
const MEDIA_SORT_NAME_ID: u32 = 56;
const MEDIA_SORT_TYPE_ID: u32 = 57;
const MEDIA_SORT_SIZE_ID: u32 = 58;
const MEDIA_SORT_MODIFIED_ID: u32 = 59;
const PREVIOUS_FILE_VIEWER_ID: u32 = 60;
const NEXT_FILE_VIEWER_ID: u32 = 61;
const ADD_MEDIA_PATH_INPUT_ID: u32 = 62;
const ADD_MEDIA_PATH_ID: u32 = 63;
const TOGGLE_MEDIA_PATH_ID: u32 = 64;
const REMOVE_MEDIA_PATH_ID: u32 = 65;
const INCLUDE_FOLDER_MEDIA_ID: u32 = 66;
const ADDITIONAL_SOURCE_ID: u32 = 67;
const SHOW_MEDIA_FOLDER_PICKER_ID: u32 = 68;
const CLOSE_MEDIA_FOLDER_PICKER_ID: u32 = 69;
const MEDIA_FOLDER_PICKER_PARENT_ID: u32 = 70;
const MEDIA_FOLDER_PICKER_NAVIGATE_ID: u32 = 71;
const SELECT_MEDIA_FOLDER_PICKER_ID: u32 = 72;
const SHOW_VIRTUAL_DIRECTORY_PICKER_ID: u32 = 73;
const CLOSE_VIRTUAL_DIRECTORY_PICKER_ID: u32 = 74;
const ADD_TO_VIRTUAL_DIRECTORY_ID: u32 = 75;
const CREATE_VIRTUAL_DIRECTORY_ID: u32 = 76;
const VIRTUAL_DIRECTORY_NAME_INPUT_ID: u32 = 77;
const SHOW_CREATE_VIRTUAL_DIRECTORY_ID: u32 = 78;
const CLOSE_CREATE_VIRTUAL_DIRECTORY_ID: u32 = 79;
const SAVE_CREATE_VIRTUAL_DIRECTORY_ID: u32 = 80;
const VIRTUAL_FILE_VIEW_ID: u32 = 81;
const VIRTUAL_DIRECTORY_VIEW_MODE_ID: u32 = 82;
const INDEXED_FILE_VIEW_ID: u32 = 83;
const FILES_VIEW_MODE_ID: u32 = 84;
const FILES_MIME_FILTER_ID: u32 = 85;
const FILES_SORT_ID: u32 = 86;
const START_MEDIA_SCAN_ID: u32 = 87;
const STOP_MEDIA_SCAN_ID: u32 = 94;
const FILES_SCANNED_FOLDER_FILTER_ID: u32 = 88;
const SCAN_IGNORED_DIRECTORIES_INPUT_ID: u32 = 89;
const SCAN_MAX_FILE_SIZE_INPUT_ID: u32 = 90;
const MEDIA_TABLE_SORT_ID: u32 = 91;
const SCAN_MAX_ITEMS_INPUT_ID: u32 = 92;
const SCAN_MAX_DIRECTORIES_INPUT_ID: u32 = 93;
const FILES_PREVIOUS_PAGE_ID: u32 = 95;
const FILES_NEXT_PAGE_ID: u32 = 96;
const MEDIA_PREVIOUS_PAGE_ID: u32 = 97;
const MEDIA_NEXT_PAGE_ID: u32 = 98;
const MEDIA_SCANNED_FOLDER_FILTER_ID: u32 = 99;
const UPLOAD_COMPLETE_ID: u32 = 100;
const ADD_INBOX_ID: u32 = 101;
const INBOX_NAME_INPUT_ID: u32 = 102;
const INBOX_FOLDER_INPUT_ID: u32 = 103;
const REMOVE_INBOX_ID: u32 = 104;
const BROWSE_INBOX_FOLDER_ID: u32 = 105;
const BROWSE_SOURCE_PATH_ID: u32 = 106;
const SHOW_NEW_INBOX_ID: u32 = 107;
const CLOSE_NEW_INBOX_ID: u32 = 108;
const SHOW_NEW_FOLDER_ID: u32 = 109;
const CLOSE_NEW_FOLDER_ID: u32 = 110;
const NEW_FOLDER_NAME_INPUT_ID: u32 = 111;
const CREATE_NEW_FOLDER_ID: u32 = 112;
const SHOW_PICKER_NEW_FOLDER_ID: u32 = 113;
const AUDIO_PREVIOUS_PAGE_ID: u32 = 114;
const AUDIO_NEXT_PAGE_ID: u32 = 115;
const AUDIO_SCANNED_FOLDER_FILTER_ID: u32 = 116;
const MAX_FILE_PREVIEW_BYTES: u64 = 1_048_576;
const MAX_HEX_PREVIEW_BYTES: usize = 65_536;
const MAX_UPLOAD_BYTES: usize = 1_073_741_824;
const FILES_PAGE_SIZE: usize = 100;
const APP_CSS: &str = r#"
html,
body {
    height: 100%;
    overflow: hidden;
}

/* The rendered application root is the body's direct child. Keep the shell
   viewport-sized so only the explicit folder-listing scroller can overflow. */
body > div {
    height: 100%;
    min-height: 0;
    overflow: hidden;
}

[name~="mobile-nav-bar"],
[name~="mobile-nav-close"],
[name~="mobile-nav-scrim"] {
    display: none !important;
}

@media (max-width: 720px) {
    [name~="app-workspace"] {
        width: 100% !important;
        min-width: 0 !important;
    }

    [name~="mobile-nav-bar"] {
        display: flex !important;
        flex: 0 0 auto !important;
        padding: max(8px, env(safe-area-inset-top)) 8px 8px !important;
        border-bottom: 1px solid #dce5e8 !important;
        background: #ffffff !important;
    }

    [name~="app-sidebar"] {
        position: fixed !important;
        z-index: 1001 !important;
        top: 0 !important;
        bottom: 0 !important;
        left: 0 !important;
        width: min(82vw, 300px) !important;
        box-sizing: border-box !important;
        padding-top: max(8px, env(safe-area-inset-top)) !important;
        overflow: auto !important;
        box-shadow: 8px 0 24px rgb(15 23 42 / 22%) !important;
        transform: translateX(-105%);
        transition: transform 180ms ease;
    }

    body[data-mobile-nav-open="true"] [name~="app-sidebar"] {
        transform: translateX(0);
    }

    [name~="mobile-nav-close"] {
        display: flex !important;
    }

    [name~="mobile-nav-scrim"] {
        position: fixed !important;
        z-index: 1000 !important;
        inset: 0 !important;
        width: 100vw !important;
        height: 100vh !important;
        padding: 0 !important;
        border: 0 !important;
        background: rgb(15 23 42 / 38%) !important;
    }

    body[data-mobile-nav-open="true"] [name~="mobile-nav-scrim"] {
        display: block !important;
    }
}

@media (max-width: 720px) and (prefers-reduced-motion: reduce) {
    [name~="app-sidebar"] {
        transition: none;
    }
}
"#;
const BUILD_VERSION: &str = env!("CARGO_PKG_VERSION");
const FAVICON_BYTES: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../desktop/icons/icon.png"
));

pub struct App {
    wgui: Wgui,
    bind_addr: SocketAddr,
    client_ids: HashSet<usize>,
    database: Database,
    config: AppConfig,
    config_path: PathBuf,
    configured_this_computer_root: PathBuf,
    this_computer_root: PathBuf,
    active_files_root: Arc<RwLock<PathBuf>>,
    this_computer_path: PathBuf,
    tree_view_root: PathBuf,
    expanded_local_dirs: HashSet<PathBuf>,
    video_viewer_asset: StaticAsset,
    image_viewer_asset: StaticAsset,
    #[allow(dead_code)]
    folder_row_asset: StaticAsset,
    media_tile_asset: StaticAsset,
    file_table_asset: StaticAsset,
    mobile_nav_asset: StaticAsset,
    upload_asset: StaticAsset,
    media_entries: Vec<LocalEntry>,
    media_index_entries: Vec<IndexedMediaFile>,
    audio_entries: Vec<LocalEntry>,
    audio_index_entries: Vec<IndexedMediaFile>,
    indexed_files: Vec<IndexedFile>,
    filtered_files: Vec<FileListingEntry>,
    media_scan_truncated: bool,
    media_scan_errors: Vec<String>,
    media_paths: Vec<MediaScanPath>,
    served_media_paths: Arc<RwLock<Vec<MediaScanPath>>>,
    managed_folders: HashMap<u32, ManagedFolder>,
    served_managed_folders: Arc<RwLock<HashMap<u32, ManagedFolder>>>,
    served_inboxes: Arc<RwLock<Vec<InboxConfig>>>,
    local_node_id: Vec<u8>,
    indexer: IndexerWorker,
    indexer_events: tokio::sync::mpsc::Receiver<IndexerEvent>,
    index_status: HashMap<u32, FolderIndexStatus>,
    last_index_progress_render: Option<Instant>,
    selected_scanned_folder_id: Option<u32>,
    selected_scanned_folder_history: Vec<ScanHistoryEntry>,
    media_watcher: RecommendedWatcher,
    watched_media_paths: Vec<PathBuf>,
    media_change_rx: tokio::sync::mpsc::Receiver<u32>,
    new_media_path: String,
    media_path_error: Option<String>,
    new_inbox_name: String,
    new_inbox_folder: String,
    inbox_error: Option<String>,
    show_new_inbox: bool,
    show_new_folder: bool,
    new_folder_name: String,
    new_folder_error: Option<String>,
    new_folder_parent: PathBuf,
    new_folder_root: PathBuf,
    show_media_folder_picker: bool,
    media_folder_picker_path: PathBuf,
    folder_picker_target: FolderPickerTarget,
    media_thumbnail_size: i32,
    media_view_mode: String,
    media_sort_key: MediaSortKey,
    media_sort_descending: bool,
    media_page: usize,
    media_scanned_folder_filter: String,
    audio_page: usize,
    audio_scanned_folder_filter: String,
    files_view_mode: String,
    files_mime_filter: String,
    files_scanned_folder_filter: String,
    files_sort: String,
    files_page: usize,
    scan_ignored_directories: String,
    scan_max_file_size_mb: String,
    scan_max_items: String,
    scan_max_directories: String,
    selected_video: Option<VideoFile>,
    selected_image: Option<ImageFile>,
    selected_file: Option<FileViewer>,
    selected_file_hash: Option<Vec<u8>>,
    file_viewer_entries: FileViewerEntries,
    file_viewer_index: Option<usize>,
    file_viewer_expanded: bool,
    folder_context: Option<FolderContext>,
    virtual_directories: Vec<VirtualDirectory>,
    virtual_directory_entries: Vec<VirtualDirectoryEntry>,
    selected_virtual_directory_id: Option<u32>,
    virtual_directory_view_mode: String,
    show_virtual_directory_picker: bool,
    show_create_virtual_directory: bool,
    new_virtual_directory_name: String,
    virtual_directory_error: Option<String>,
    show_add_source: bool,
    new_source_name: String,
    new_source_path: String,
    sources: Vec<Source>,
    active_source_id: Option<u32>,
    _session_secrets: SessionSecretStore,
    viewing_this_computer: bool,
    active_page: AppPage,
}

#[allow(dead_code)]
#[derive(Clone)]
struct LocalEntry {
    path: PathBuf,
    name: String,
    is_directory: bool,
    is_symlink: bool,
    kind: &'static str,
    size: String,
    size_bytes: u64,
    modified: String,
    modified_at: Option<SystemTime>,
    media_root_id: Option<u32>,
}

#[derive(Clone)]
struct FileListingEntry {
    path: Option<PathBuf>,
    name: String,
    size: u64,
    mime_type: Option<String>,
    modified_at: Option<SystemTime>,
    hash: Option<Vec<u8>>,
    replica_count: usize,
    media_root_id: Option<u32>,
}

impl FileListingEntry {
    fn from_media(entry: &LocalEntry, indexed: &IndexedMediaFile) -> Self {
        Self {
            path: Some(entry.path.clone()),
            name: entry.name.clone(),
            size: entry.size_bytes,
            mime_type: indexed.mime_type.clone(),
            modified_at: entry.modified_at,
            hash: indexed.hash.clone(),
            replica_count: indexed.replica_count,
            media_root_id: entry.media_root_id,
        }
    }

    fn from_virtual(entry: &VirtualDirectoryEntry) -> Self {
        Self {
            path: entry.path.clone(),
            name: entry
                .path
                .as_ref()
                .and_then(|path| path.file_name())
                .map_or_else(
                    || "Unavailable file".to_owned(),
                    |name| name.to_string_lossy().into_owned(),
                ),
            size: entry.size,
            mime_type: entry.mime_type.clone(),
            modified_at: entry.modified_at.and_then(system_time_from_millis),
            hash: Some(entry.hash.clone()),
            replica_count: entry.replica_count,
            media_root_id: entry.scanned_folder_id,
        }
    }

    fn from_indexed(entry: &IndexedFile) -> Self {
        Self {
            path: Some(entry.path.clone()),
            name: entry.path.file_name().map_or_else(
                || entry.path.display().to_string(),
                |name| name.to_string_lossy().into_owned(),
            ),
            size: entry.size,
            mime_type: entry.mime_type.clone(),
            modified_at: entry.modified_at.and_then(system_time_from_millis),
            hash: entry.hash.clone(),
            replica_count: entry.replica_count,
            media_root_id: entry.scanned_folder_id,
        }
    }

    fn is_image(&self) -> bool {
        self.path
            .as_deref()
            .is_some_and(|path| image_content_type(path).is_some())
    }

    fn is_video(&self) -> bool {
        self.path
            .as_deref()
            .is_some_and(|path| video_content_type(path).is_some())
    }

    fn file_type(&self) -> &str {
        self.mime_type.as_deref().unwrap_or("Unknown")
    }

    fn hash_label(&self) -> String {
        self.hash.as_deref().map_or_else(
            || "Not indexed".to_owned(),
            |hash| hash.iter().map(|byte| format!("{byte:02x}")).collect(),
        )
    }

    fn as_local_entry(&self) -> Option<LocalEntry> {
        let path = self.path.clone()?;
        Some(LocalEntry {
            name: self.name.clone(),
            path,
            is_directory: false,
            is_symlink: false,
            kind: "Indexed file",
            size: format_size(self.size),
            size_bytes: self.size,
            modified: self
                .modified_at
                .map_or_else(|| "—".to_owned(), format_modified),
            modified_at: self.modified_at,
            media_root_id: self.media_root_id,
        })
    }
}

#[cfg(test)]
struct MediaScanResult {
    entries: Vec<LocalEntry>,
    _observations: HashMap<u32, Vec<MediaIndexObservation>>,
    truncated: bool,
    errors: Vec<String>,
}

#[derive(Default)]
struct FolderIndexStatus {
    scanning: bool,
    queued: bool,
    stopping: bool,
    directories_scanned: usize,
    media_files_indexed: usize,
    current_path: Option<String>,
    message: Option<String>,
    outcome: Option<ScanOutcome>,
}

impl LocalEntry {
    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

#[allow(dead_code)]
struct TreeNode {
    entry: LocalEntry,
    depth: usize,
}

struct VideoFile {
    name: String,
    size: String,
    modified: String,
    source_url: String,
}

struct ImageFile {
    name: String,
    size: String,
    modified: String,
    source_url: String,
    preview_url: Option<String>,
}

enum FileViewerEntries {
    Local(Vec<LocalEntry>),
    Indexed,
    Media(Vec<usize>),
}

impl FileViewerEntries {
    fn len(&self, indexed_entries: &[FileListingEntry]) -> usize {
        match self {
            Self::Local(entries) => entries.len(),
            Self::Indexed => indexed_entries.len(),
            Self::Media(indices) => indices.len(),
        }
    }

    fn clear(&mut self) {
        *self = Self::Local(Vec::new());
    }
}

enum FileViewMode {
    Text,
    Hex,
}

struct FileViewer {
    name: String,
    size: String,
    bytes: Vec<u8>,
    text: Option<String>,
    mode: FileViewMode,
    truncated: bool,
}

struct FolderContext {
    name: String,
    path: PathBuf,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum AppPage {
    Files,
    Media,
    Audio,
    ScannedFolder,
    VirtualDirectories,
    Transfers,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaSortKey {
    Name,
    Type,
    Size,
    Replicas,
    Modified,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FolderPickerTarget {
    Media,
    Inbox,
    Source,
}

impl App {
    pub fn new() -> Result<Self> {
        let (mut config, paths) = config::load()?;
        // These were previously hard-coded internal caps. They were never
        // configurable, so upgrade installations that still have the old
        // defaults to the new unlimited behaviour.
        if config.media.max_items == 1_000 && config.media.max_directories == 512 {
            config.media.max_items = 0;
            config.media.max_directories = 0;
            config::save(&config, &paths.config_file)?;
            log::info!("migrated legacy scan limits to unlimited");
        }
        let bind_address =
            std::env::var("BIND_ADDR").unwrap_or_else(|_| config.server.bind_address.clone());
        let bind_addr: SocketAddr = bind_address
            .parse()
            .context("invalid configured bind address")?;
        let configured_root = std::env::var_os("THIS_COMPUTER_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| config.server.this_computer_root.clone());
        let this_computer_root = fs::canonicalize(&configured_root).with_context(|| {
            format!(
                "unable to access This Computer root '{}'",
                configured_root.display()
            )
        })?;

        let database = Database::open(&paths.database_file)?;
        let existing_media_paths = database.media_scan_paths()?;
        if !config.media.paths_initialized {
            if existing_media_paths.is_empty() {
                for path in default_media_paths() {
                    database.insert_initial_media_path(&path)?;
                }
            }
            config.media.paths_initialized = true;
            config::save(&config, &paths.config_file)?;
        }
        let media_paths = database.media_scan_paths()?;
        let local_node_id = database.local_node_id(&config.general.device_name)?;
        let virtual_directories = database.virtual_directories()?;
        let virtual_directory_entries = database.virtual_directory_entries(&local_node_id)?;
        let (indexer_event_tx, indexer_events) = tokio::sync::mpsc::channel(256);
        let indexer = IndexerWorker::start(database.path().to_path_buf(), indexer_event_tx);
        let managed_folders = managed_folders(&media_paths);
        let sources = database.sources()?;
        for source in &sources {
            if let Err(error) = validate_source_config(
                &source.source_type,
                source.config_schema_version,
                &source.config,
            ) {
                log::warn!("source '{}' is unavailable: {error}", source.name);
            }
        }

        let wgui = Wgui::new(bind_addr);
        wgui.set_css(APP_CSS);
        let video_viewer_asset = wgui.mount_static_file(
            "/video-viewer.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/video-viewer.js"),
        );
        let image_viewer_asset = wgui.mount_static_file(
            "/image-viewer.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/image-viewer.js"),
        );
        let folder_row_asset = wgui.mount_static_file(
            "/folder-row.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/folder-row.js"),
        );
        let media_tile_asset = wgui.mount_static_file(
            "/media-tile.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/media-tile.js"),
        );
        let file_table_asset = wgui.mount_static_file(
            "/file-table.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/file-table.js"),
        );
        let mobile_nav_asset = wgui.mount_static_file(
            "/mobile-nav.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/mobile-nav.js"),
        );
        let upload_asset = wgui.mount_static_file(
            "/file-upload.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/file-upload.js"),
        );
        let active_files_root = Arc::new(RwLock::new(this_computer_root.clone()));
        let served_media_paths = Arc::new(RwLock::new(media_paths.clone()));
        let served_managed_folders = Arc::new(RwLock::new(managed_folders.clone()));
        let served_inboxes = Arc::new(RwLock::new(config.inboxes.clone()));
        let thumbnail_database_path = database.path().to_path_buf();
        let thumbnail_cache_dir = paths.thumbnail_cache_dir.clone();
        let thumbnail_node_id = local_node_id.clone();
        let handler_files_root = active_files_root.clone();
        let handler_media_paths = served_media_paths.clone();
        let handler_managed_folders = served_managed_folders.clone();
        let handler_upload_root = this_computer_root.clone();
        let handler_inboxes = served_inboxes.clone();
        wgui.set_http_handler(move |request| {
            let files_root = handler_files_root.clone();
            let media_paths = handler_media_paths.clone();
            let managed_folders = handler_managed_folders.clone();
            let thumbnail_database_path = thumbnail_database_path.clone();
            let thumbnail_cache_dir = thumbnail_cache_dir.clone();
            let thumbnail_node_id = thumbnail_node_id.clone();
            let upload_root = handler_upload_root.clone();
            let inboxes = handler_inboxes.clone();
            async move {
                if request.path == "/favicon.ico" {
                    return Some(
                        HttpResponse::new(200, FAVICON_BYTES.to_vec())
                            .header("content-type", "image/png"),
                    );
                }
                if request.path == "/uploads" {
                    return Some(upload_response(&request, &upload_root, &inboxes));
                }
                if let Some(relative_path) = request.path.strip_prefix("/source-files/") {
                    let root = files_root.read().ok()?.clone();
                    return local_media_response(relative_path, &root);
                }
                if let Some(thumbnail_request) = request.path.strip_prefix("/media-thumbnails/") {
                    let folders = managed_folders.read().ok()?.clone();
                    let thumbnail_request = thumbnail_request.to_owned();
                    let cache_only = request
                        .query
                        .get("cached")
                        .is_some_and(|value| value == "1");
                    return tokio::task::spawn_blocking(move || {
                        media_thumbnail_response(
                            &thumbnail_request,
                            &thumbnail_database_path,
                            &thumbnail_node_id,
                            &folders,
                            &thumbnail_cache_dir,
                            cache_only,
                        )
                    })
                    .await
                    .ok()
                    .flatten();
                }
                request.path.strip_prefix("/media-files/").and_then(|path| {
                    let (id, relative_path) = path.split_once('/')?;
                    let id = id.parse::<u32>().ok()?;
                    let roots = media_paths.read().ok()?;
                    let root = roots.iter().find(|root| root.id == id && root.enabled)?;
                    if !root.indexes_media() {
                        return None;
                    }
                    let folders = managed_folders.read().ok()?;
                    managed_media_response(relative_path, folders.get(&id)?)
                })
            }
        });

        let mut expanded_local_dirs = HashSet::new();
        expanded_local_dirs.insert(this_computer_root.clone());
        let cached_media =
            available_media_entries(database.cached_media(&local_node_id)?, &managed_folders);
        let cached_audio =
            available_media_entries(database.cached_audio(&local_node_id)?, &managed_folders);
        let indexed_files = database.cached_files(&local_node_id)?;
        let scan_ignored_directories = config.media.ignored_directory_names.join(", ");
        let scan_max_file_size_mb = config.media.max_file_size_mb.to_string();
        let scan_max_items = config.media.max_items.to_string();
        let scan_max_directories = config.media.max_directories.to_string();
        let (media_watcher, media_change_rx, watched_media_paths) = build_media_watcher(
            &media_paths,
            Duration::from_millis(config.media.watch_debounce_ms.max(1)),
            &config.media.ignored_directory_names,
        )?;
        let media_folder_picker_path = media_folder_picker_start_path(&this_computer_root);

        let app = Self {
            wgui,
            bind_addr,
            client_ids: HashSet::new(),
            database,
            config_path: paths.config_file,
            config,
            configured_this_computer_root: this_computer_root.clone(),
            active_files_root,
            this_computer_path: this_computer_root.clone(),
            tree_view_root: this_computer_root.clone(),
            this_computer_root: this_computer_root.clone(),
            expanded_local_dirs,
            video_viewer_asset,
            image_viewer_asset,
            folder_row_asset,
            media_tile_asset,
            file_table_asset,
            mobile_nav_asset,
            upload_asset,
            media_entries: local_entries_from_index(cached_media.clone()),
            media_index_entries: cached_media,
            audio_entries: local_entries_from_index(cached_audio.clone()),
            audio_index_entries: cached_audio,
            indexed_files,
            filtered_files: Vec::new(),
            media_scan_truncated: false,
            media_scan_errors: Vec::new(),
            media_paths,
            served_media_paths,
            managed_folders,
            served_managed_folders,
            served_inboxes,
            local_node_id,
            indexer,
            indexer_events,
            index_status: HashMap::new(),
            last_index_progress_render: None,
            selected_scanned_folder_id: None,
            selected_scanned_folder_history: Vec::new(),
            media_watcher,
            watched_media_paths,
            media_change_rx,
            new_media_path: String::new(),
            media_path_error: None,
            new_inbox_name: String::new(),
            new_inbox_folder: String::new(),
            inbox_error: None,
            show_new_inbox: false,
            show_new_folder: false,
            new_folder_name: String::new(),
            new_folder_error: None,
            new_folder_parent: this_computer_root.clone(),
            new_folder_root: this_computer_root.clone(),
            show_media_folder_picker: false,
            media_folder_picker_path,
            folder_picker_target: FolderPickerTarget::Media,
            media_thumbnail_size: 220,
            media_view_mode: "thumbnails".to_owned(),
            media_sort_key: MediaSortKey::Name,
            media_sort_descending: false,
            media_page: 0,
            media_scanned_folder_filter: "all".to_owned(),
            audio_page: 0,
            audio_scanned_folder_filter: "all".to_owned(),
            files_view_mode: "table".to_owned(),
            files_mime_filter: "all".to_owned(),
            files_scanned_folder_filter: "all".to_owned(),
            files_sort: "modified".to_owned(),
            files_page: 0,
            scan_ignored_directories,
            scan_max_file_size_mb,
            scan_max_items,
            scan_max_directories,
            selected_video: None,
            selected_image: None,
            selected_file: None,
            selected_file_hash: None,
            file_viewer_entries: FileViewerEntries::Local(Vec::new()),
            file_viewer_index: None,
            file_viewer_expanded: false,
            folder_context: None,
            virtual_directories,
            virtual_directory_entries,
            selected_virtual_directory_id: None,
            virtual_directory_view_mode: "table".to_owned(),
            show_virtual_directory_picker: false,
            show_create_virtual_directory: false,
            new_virtual_directory_name: String::new(),
            virtual_directory_error: None,
            show_add_source: false,
            new_source_name: String::new(),
            new_source_path: String::new(),
            sources,
            active_source_id: None,
            _session_secrets: SessionSecretStore::default(),
            viewing_this_computer: true,
            active_page: AppPage::Files,
        };
        let mut app = app;
        app.refresh_filtered_files(true);
        Ok(app)
    }

    pub fn bind_address(&self) -> SocketAddr {
        self.bind_addr
    }

    fn render(&self) -> Item {
        let version = format!("v{BUILD_VERSION}");
        let mut sidebar_items = vec![
            hstack([
                text("◕  PuppyDrive").grow(1).color("#0f6175"),
                button("×")
                    .name("mobile-nav-close")
                    .width(36)
                    .padding(4)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .padding(6),
            nav_link("□  Files", "/", self.active_page == AppPage::Files),
            nav_link("▧  Media", "/media", self.active_page == AppPage::Media),
            nav_link("♫  Audio", "/audio", self.active_page == AppPage::Audio),
            nav_link(
                "◫  Virtual directories",
                "/virtual-directories",
                self.active_page == AppPage::VirtualDirectories,
            ),
            nav_link(
                "⇄  Transfers",
                "/transfers",
                self.active_page == AppPage::Transfers,
            ),
            hstack([
                text("Sources").grow(1).color("#6b7280"),
                button("+")
                    .id(SHOW_ADD_SOURCE_ID)
                    .width(28)
                    .padding(2)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .padding_top(6),
            this_computer_nav_item(
                self.active_source_id.is_none() && self.active_page == AppPage::Files,
            ),
        ];
        sidebar_items.extend(self.sources.iter().map(|source| {
            let available =
                source.enabled && local_source_path(source).is_some_and(|path| path.is_dir());
            source_nav_button(
                source,
                self.active_source_id == Some(source.id) && self.active_page == AppPage::Files,
                available,
            )
        }));
        sidebar_items.push(
            text("Scanned folders")
                .padding_top(8)
                .padding_bottom(2)
                .color("#6b7280"),
        );
        sidebar_items.extend(self.media_paths.iter().map(|folder| {
            scanned_folder_nav_item(folder, self.managed_folders.contains_key(&folder.id))
        }));
        sidebar_items.extend([
            vstack(Vec::<Item>::new()).grow(1),
            nav_link(
                "⚙  Settings",
                "/settings",
                self.active_page == AppPage::Settings,
            ),
            text(&version).padding(8).color("#6b7280"),
        ]);
        let sidebar = vstack(sidebar_items)
            .name("app-sidebar")
            .width(210)
            .spacing(4)
            .padding(8)
            .background_color("#f8fbfc");

        let content = self
            .files_panel()
            .grow(1)
            .padding_left(6)
            .padding_right(6)
            .overflow("hidden");

        let workspace = match self.active_page {
            AppPage::Settings => self.settings_panel().grow(1).padding(6).overflow("auto"),
            AppPage::Media => self.media_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::Audio => self.audio_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::ScannedFolder => self
                .scanned_folder_detail_panel()
                .grow(1)
                .padding(6)
                .overflow("hidden"),
            AppPage::VirtualDirectories => self
                .virtual_directories_panel()
                .grow(1)
                .padding(6)
                .overflow("hidden"),
            AppPage::Transfers => self.transfers_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::Files => content,
        };

        let page_title = match self.active_page {
            AppPage::Files => "Files",
            AppPage::Media => "Media",
            AppPage::Audio => "Audio",
            AppPage::ScannedFolder => "Scanned folder",
            AppPage::VirtualDirectories => "Virtual directories",
            AppPage::Transfers => "Transfers",
            AppPage::Settings => "Settings",
        };
        let mobile_nav_bar = hstack([
            custom_component(
                "mobile-nav-toggle",
                self.mobile_nav_asset.url(),
                serde_json::json!({}),
            )
            .width(44)
            .height(38),
            text(page_title).grow(1).color("#1f2937"),
        ])
        .name("mobile-nav-bar")
        .spacing(8);

        let mut shell = vec![
            sidebar,
            vstack([mobile_nav_bar, workspace])
                .name("app-workspace")
                .grow(1)
                .fill(true)
                .overflow("hidden")
                .background_color("#f4f7f8"),
            button("").name("mobile-nav-scrim"),
        ];
        if let Some(viewer) = self.file_viewer_modal() {
            shell.push(viewer);
        }
        if let Some(context_menu) = self.folder_context_modal() {
            shell.push(context_menu);
        }
        if self.show_add_source {
            shell.push(self.add_source_modal());
        }
        if self.show_new_inbox {
            shell.push(self.new_inbox_modal());
        }
        if self.show_media_folder_picker {
            shell.push(self.media_folder_picker_modal());
        }
        if self.show_new_folder {
            shell.push(self.new_folder_modal());
        }
        if self.show_virtual_directory_picker {
            shell.push(self.virtual_directory_picker_modal());
        }
        if self.show_create_virtual_directory {
            shell.push(self.create_virtual_directory_modal());
        }

        hstack(shell).fill(true).overflow("hidden")
    }

    pub async fn run(&mut self) {
        loop {
            let message = tokio::select! {
                message = self.wgui.next() => message,
                changed = self.media_change_rx.recv() => {
                    let Some(folder_id) = changed else {
                        break;
                    };
                    self.queue_media_index_for(folder_id, ScanTrigger::FilesystemChange);
                    self.render_all_clients().await;
                    continue;
                }
                event = self.indexer_events.recv() => {
                    if let Some(event) = event {
                        let is_progress = matches!(&event, IndexerEvent::Progress { .. });
                        let should_render = !is_progress || self
                            .last_index_progress_render
                            .is_none_or(|last| last.elapsed() >= Duration::from_millis(250));
                        self.handle_indexer_event(event);
                        if should_render {
                            self.render_all_clients().await;
                            if is_progress {
                                self.last_index_progress_render = Some(Instant::now());
                            }
                        }
                    }
                    continue;
                }
            };
            let Some(message) = message else {
                break;
            };
            let client_id = message.client_id;
            match message.event {
                ClientEvent::Connected { id: _ } => {
                    self.client_ids.insert(client_id);
                    self.wgui.render(client_id, self.render()).await;
                    log::info!("wgui client {client_id} connected");
                }
                ClientEvent::Disconnected { id: _ } => {
                    self.client_ids.remove(&client_id);
                    log::info!("wgui client {client_id} disconnected");
                }
                ClientEvent::PathChanged(change) => {
                    let path = change.path.trim_end_matches('/');
                    let scanned_folder_id = path
                        .strip_prefix("/scanned-folders/")
                        .and_then(|id| id.parse::<u32>().ok())
                        .filter(|id| self.media_paths.iter().any(|folder| folder.id == *id));
                    self.active_page = match path {
                        "/media" => AppPage::Media,
                        "/audio" => AppPage::Audio,
                        _ if scanned_folder_id.is_some() => {
                            self.selected_scanned_folder_id = scanned_folder_id;
                            self.refresh_selected_scanned_folder_history();
                            AppPage::ScannedFolder
                        }
                        "/virtual-directories" => {
                            self.selected_virtual_directory_id = None;
                            AppPage::VirtualDirectories
                        }
                        "/transfers" => AppPage::Transfers,
                        "/settings" => AppPage::Settings,
                        _ if path
                            .strip_prefix("/virtual-directories/")
                            .and_then(|id| id.parse::<u32>().ok())
                            .is_some_and(|id| {
                                self.virtual_directories
                                    .iter()
                                    .any(|directory| directory.id == id)
                            }) =>
                        {
                            self.selected_virtual_directory_id = path
                                .strip_prefix("/virtual-directories/")
                                .and_then(|id| id.parse::<u32>().ok());
                            AppPage::VirtualDirectories
                        }
                        _ => AppPage::Files,
                    };
                    if self.active_page != AppPage::Files {
                        self.close_file_viewer();
                        self.folder_context = None;
                        self.show_add_source = false;
                        self.show_virtual_directory_picker = false;
                        self.show_create_virtual_directory = false;
                    }
                    if self.active_page != AppPage::VirtualDirectories {
                        self.selected_virtual_directory_id = None;
                    }
                    if self.active_page != AppPage::ScannedFolder {
                        self.selected_scanned_folder_id = None;
                        self.selected_scanned_folder_history.clear();
                    }
                    if self.active_page != AppPage::Settings {
                        self.show_new_inbox = false;
                    }
                    if self.active_page != AppPage::Files {
                        self.show_new_folder = false;
                    }
                }
                ClientEvent::OnSelect(change) if change.id == MEDIA_VIEW_MODE_ID => {
                    self.media_view_mode = change.value;
                    self.media_page = 0;
                }
                ClientEvent::OnSelect(change) if change.id == MEDIA_SCANNED_FOLDER_FILTER_ID => {
                    self.media_scanned_folder_filter = change.value;
                    self.media_page = 0;
                }
                ClientEvent::OnSelect(change) if change.id == AUDIO_SCANNED_FOLDER_FILTER_ID => {
                    self.audio_scanned_folder_filter = change.value;
                    self.audio_page = 0;
                }
                ClientEvent::OnSelect(change) if change.id == VIRTUAL_DIRECTORY_VIEW_MODE_ID => {
                    self.virtual_directory_view_mode = change.value;
                }
                ClientEvent::OnSelect(change) if change.id == FILES_VIEW_MODE_ID => {
                    self.files_view_mode = change.value;
                }
                ClientEvent::OnSelect(change) if change.id == FILES_MIME_FILTER_ID => {
                    self.files_mime_filter = change.value;
                    self.refresh_filtered_files(true);
                }
                ClientEvent::OnSelect(change) if change.id == FILES_SCANNED_FOLDER_FILTER_ID => {
                    self.files_scanned_folder_filter = change.value;
                    self.refresh_filtered_files(true);
                }
                ClientEvent::OnSelect(change) if change.id == FILES_SORT_ID => {
                    self.files_sort = change.value;
                    self.refresh_filtered_files(true);
                }
                ClientEvent::OnSliderChange(change) if change.id == MEDIA_THUMBNAIL_SIZE_ID => {
                    self.media_thumbnail_size = change.value.clamp(140, 320);
                }
                ClientEvent::OnTextChanged(change) if change.id == ADD_SOURCE_NAME_INPUT_ID => {
                    self.new_source_name = change.value;
                }
                ClientEvent::OnTextChanged(change) if change.id == ADD_SOURCE_PATH_INPUT_ID => {
                    self.new_source_path = change.value;
                }
                ClientEvent::OnTextChanged(change) if change.id == ADD_MEDIA_PATH_INPUT_ID => {
                    self.new_media_path = change.value;
                    self.media_path_error = None;
                }
                ClientEvent::OnTextChanged(change) if change.id == INBOX_NAME_INPUT_ID => {
                    self.new_inbox_name = change.value;
                    self.inbox_error = None;
                }
                ClientEvent::OnTextChanged(change) if change.id == INBOX_FOLDER_INPUT_ID => {
                    self.new_inbox_folder = change.value;
                    self.inbox_error = None;
                }
                ClientEvent::OnTextChanged(change) if change.id == NEW_FOLDER_NAME_INPUT_ID => {
                    self.new_folder_name = change.value;
                    self.new_folder_error = None;
                }
                ClientEvent::OnTextChanged(change)
                    if change.id == SCAN_IGNORED_DIRECTORIES_INPUT_ID =>
                {
                    self.scan_ignored_directories = change.value;
                    self.config.media.ignored_directory_names = self
                        .scan_ignored_directories
                        .split(',')
                        .map(str::trim)
                        .filter(|name| !name.is_empty())
                        .map(str::to_owned)
                        .collect();
                    self.save_config();
                }
                ClientEvent::OnTextChanged(change) if change.id == SCAN_MAX_FILE_SIZE_INPUT_ID => {
                    self.scan_max_file_size_mb = change.value;
                    if let Ok(limit) = self.scan_max_file_size_mb.trim().parse() {
                        self.config.media.max_file_size_mb = limit;
                        self.save_config();
                    }
                }
                ClientEvent::OnTextChanged(change) if change.id == SCAN_MAX_ITEMS_INPUT_ID => {
                    self.scan_max_items = change.value;
                    if let Ok(limit) = self.scan_max_items.trim().parse() {
                        self.config.media.max_items = limit;
                        self.save_config();
                    }
                }
                ClientEvent::OnTextChanged(change)
                    if change.id == SCAN_MAX_DIRECTORIES_INPUT_ID =>
                {
                    self.scan_max_directories = change.value;
                    if let Ok(limit) = self.scan_max_directories.trim().parse() {
                        self.config.media.max_directories = limit;
                        self.save_config();
                    }
                }
                ClientEvent::OnTextChanged(change)
                    if change.id == VIRTUAL_DIRECTORY_NAME_INPUT_ID =>
                {
                    self.new_virtual_directory_name = change.value;
                    self.virtual_directory_error = None;
                }
                ClientEvent::OnKeyDown(key) if key.keycode == "ArrowLeft" => {
                    self.navigate_file_viewer(-1);
                }
                ClientEvent::OnKeyDown(key) if key.keycode == "ArrowRight" => {
                    self.navigate_file_viewer(1);
                }
                ClientEvent::OnCustom(event) if event.id == UPLOAD_COMPLETE_ID => {
                    // The file listing reads the filesystem on render, so the normal render at
                    // the end of this event loop immediately reflects completed uploads.
                }
                ClientEvent::OnCustom(event) if event.id == LOCAL_TREE_SELECT_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        self.select_tree_directory(index);
                    }
                }
                ClientEvent::OnCustom(event) if event.id == LOCAL_TREE_TOGGLE_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        self.toggle_tree_directory(index);
                    }
                }
                ClientEvent::OnCustom(event) if event.id == LOCAL_TREE_NAVIGATE_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        self.navigate_to_tree_directory(index);
                    }
                }
                ClientEvent::OnCustom(event) if event.id == FOLDER_CONTEXT_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        self.open_folder_context(index);
                    }
                }
                ClientEvent::OnCustom(event) if event.id == LOCAL_MEDIA_VIEW_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        if self.active_page == AppPage::Audio {
                            self.open_audio(index);
                        } else {
                            self.open_media(index);
                        }
                    }
                }
                ClientEvent::OnCustom(event) if event.id == MEDIA_TABLE_SORT_ID => {
                    if matches!(self.active_page, AppPage::Media | AppPage::Audio) {
                        if let Some(key) = media_sort_key_from_payload(&event.payload) {
                            self.toggle_media_sort(key);
                            self.media_page = 0;
                            self.audio_page = 0;
                        }
                    }
                }
                ClientEvent::OnCustom(event) if event.id == VIRTUAL_FILE_VIEW_ID => {
                    if let (Some(directory_id), Some(index)) = (
                        self.selected_virtual_directory_id,
                        custom_event_index(&event.payload),
                    ) {
                        self.open_virtual_file(directory_id, index);
                    }
                }
                ClientEvent::OnCustom(event) if event.id == INDEXED_FILE_VIEW_ID => {
                    if let Some(index) = custom_event_index(&event.payload) {
                        self.open_indexed_file(index);
                    }
                }
                ClientEvent::OnClick(click) => match click.id {
                    THIS_COMPUTER_SOURCE_ID => {
                        self.activate_files_root(self.configured_this_computer_root.clone(), None);
                        self.wgui.handle().push_state(client_id, "/").await;
                    }
                    ADDITIONAL_SOURCE_ID => {
                        if let Some(id) = click.inx {
                            self.activate_source(id);
                            self.wgui.handle().push_state(client_id, "/").await;
                        }
                    }
                    LOCAL_PARENT_ID if self.viewing_this_computer => self.go_to_parent(),
                    LOCAL_BREADCRUMB_ID if self.viewing_this_computer => {
                        if let Some(depth) = click.inx {
                            self.go_to_breadcrumb(depth as usize);
                        }
                    }
                    LOCAL_TREE_TOGGLE_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.toggle_tree_directory(index as usize);
                        }
                    }
                    LOCAL_TREE_SELECT_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.select_tree_directory(index as usize);
                        }
                    }
                    LOCAL_VIDEO_VIEW_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.open_local_file(index as usize);
                        }
                    }
                    LOCAL_IMAGE_VIEW_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.open_local_file(index as usize);
                        }
                    }
                    LOCAL_TEXT_VIEW_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.open_local_file(index as usize);
                        }
                    }
                    CLOSE_FILE_VIEWER_ID => self.close_file_viewer(),
                    FILES_PREVIOUS_PAGE_ID => {
                        self.files_page = self.files_page.saturating_sub(1);
                    }
                    FILES_NEXT_PAGE_ID => {
                        let page_count = self.filtered_files.len().div_ceil(FILES_PAGE_SIZE);
                        if self.files_page + 1 < page_count {
                            self.files_page += 1;
                        }
                    }
                    MEDIA_PREVIOUS_PAGE_ID => {
                        self.media_page = self.media_page.saturating_sub(1);
                    }
                    MEDIA_NEXT_PAGE_ID => {
                        let page_count = self.media_entries.len().div_ceil(FILES_PAGE_SIZE);
                        if self.media_page + 1 < page_count {
                            self.media_page += 1;
                        }
                    }
                    AUDIO_PREVIOUS_PAGE_ID => {
                        self.audio_page = self.audio_page.saturating_sub(1);
                    }
                    AUDIO_NEXT_PAGE_ID => {
                        let page_count = self
                            .filtered_audio_indices()
                            .len()
                            .div_ceil(FILES_PAGE_SIZE);
                        if self.audio_page + 1 < page_count {
                            self.audio_page += 1;
                        }
                    }
                    SHOW_VIRTUAL_DIRECTORY_PICKER_ID => {
                        if self.selected_file_hash.is_some() {
                            self.show_virtual_directory_picker = true;
                            self.virtual_directory_error = None;
                        }
                    }
                    CLOSE_VIRTUAL_DIRECTORY_PICKER_ID => {
                        self.show_virtual_directory_picker = false;
                        self.virtual_directory_error = None;
                    }
                    ADD_TO_VIRTUAL_DIRECTORY_ID => {
                        if let Some(id) = click.inx {
                            self.add_selected_file_to_virtual_directory(id);
                        }
                    }
                    CREATE_VIRTUAL_DIRECTORY_ID => {
                        self.create_virtual_directory_for_selected_file()
                    }
                    SHOW_CREATE_VIRTUAL_DIRECTORY_ID => {
                        self.show_create_virtual_directory = true;
                        self.new_virtual_directory_name.clear();
                        self.virtual_directory_error = None;
                    }
                    CLOSE_CREATE_VIRTUAL_DIRECTORY_ID => {
                        self.show_create_virtual_directory = false;
                        self.virtual_directory_error = None;
                    }
                    SAVE_CREATE_VIRTUAL_DIRECTORY_ID => self.create_empty_virtual_directory(),
                    PREVIOUS_FILE_VIEWER_ID => self.navigate_file_viewer(-1),
                    NEXT_FILE_VIEWER_ID => self.navigate_file_viewer(1),
                    TOGGLE_FILE_VIEWER_SIZE_ID => {
                        self.file_viewer_expanded = !self.file_viewer_expanded;
                    }
                    LOCAL_MEDIA_VIEW_ID => {
                        if let Some(index) = click.inx {
                            if self.active_page == AppPage::Audio {
                                self.open_audio(index as usize);
                            } else {
                                self.open_media(index as usize);
                            }
                        }
                    }
                    VIRTUAL_FILE_VIEW_ID => {
                        if let (Some(directory_id), Some(index)) =
                            (self.selected_virtual_directory_id, click.inx)
                        {
                            self.open_virtual_file(directory_id, index as usize);
                        }
                    }
                    INDEXED_FILE_VIEW_ID => {
                        if let Some(index) = click.inx {
                            self.open_indexed_file(index as usize);
                        }
                    }
                    MEDIA_SORT_NAME_ID => self.toggle_media_sort(MediaSortKey::Name),
                    MEDIA_SORT_TYPE_ID => self.toggle_media_sort(MediaSortKey::Type),
                    MEDIA_SORT_SIZE_ID => self.toggle_media_sort(MediaSortKey::Size),
                    MEDIA_SORT_MODIFIED_ID => self.toggle_media_sort(MediaSortKey::Modified),
                    // The media cache is kept current by indexer events. Refreshing this view
                    // only needs the normal render at the end of the event loop.
                    REFRESH_MEDIA_ID => {}
                    START_MEDIA_SCAN_ID => {
                        if let Some(id) = click.inx {
                            self.queue_media_index_for(id, ScanTrigger::ManualFolder);
                        }
                    }
                    STOP_MEDIA_SCAN_ID => {
                        if let Some(id) = click.inx {
                            self.stop_media_index_for(id);
                        }
                    }
                    TEXT_VIEW_MODE_ID => self.set_file_view_mode(FileViewMode::Text),
                    HEX_VIEW_MODE_ID => self.set_file_view_mode(FileViewMode::Hex),
                    CLOSE_FOLDER_CONTEXT_ID => self.folder_context = None,
                    OPEN_FOLDER_CONTEXT_ID => self.open_context_folder(),
                    TOGGLE_FOLDER_CONTEXT_ID => self.toggle_context_folder(),
                    INCLUDE_FOLDER_MEDIA_ID => self.include_context_folder_in_media().await,
                    SHOW_ADD_SOURCE_ID => self.show_add_source = true,
                    CLOSE_ADD_SOURCE_ID => self.show_add_source = false,
                    SAVE_ADD_SOURCE_ID => self.save_added_source().await,
                    SHOW_MEDIA_FOLDER_PICKER_ID => {
                        self.open_folder_picker(FolderPickerTarget::Media)
                    }
                    BROWSE_INBOX_FOLDER_ID => self.open_folder_picker(FolderPickerTarget::Inbox),
                    BROWSE_SOURCE_PATH_ID => self.open_folder_picker(FolderPickerTarget::Source),
                    CLOSE_MEDIA_FOLDER_PICKER_ID => self.show_media_folder_picker = false,
                    MEDIA_FOLDER_PICKER_PARENT_ID => self.media_folder_picker_parent(),
                    MEDIA_FOLDER_PICKER_NAVIGATE_ID => {
                        if let Some(index) = click.inx {
                            self.navigate_media_folder_picker(index as usize);
                        }
                    }
                    SELECT_MEDIA_FOLDER_PICKER_ID => self.select_media_folder_picker().await,
                    ADD_MEDIA_PATH_ID => self.add_media_path_from_input().await,
                    TOGGLE_MEDIA_PATH_ID => {
                        if let Some(id) = click.inx {
                            self.toggle_media_path(id).await;
                        }
                    }
                    REMOVE_MEDIA_PATH_ID => {
                        if let Some(id) = click.inx {
                            self.remove_media_path(id);
                        }
                    }
                    SHOW_NEW_INBOX_ID => {
                        self.new_inbox_name.clear();
                        self.new_inbox_folder.clear();
                        self.inbox_error = None;
                        self.show_new_inbox = true;
                    }
                    CLOSE_NEW_INBOX_ID => {
                        self.show_new_inbox = false;
                        self.inbox_error = None;
                    }
                    ADD_INBOX_ID => {
                        if self.add_inbox() {
                            self.show_new_inbox = false;
                        }
                    }
                    REMOVE_INBOX_ID => {
                        if let Some(index) = click.inx {
                            self.remove_inbox(index as usize);
                        }
                    }
                    SHOW_NEW_FOLDER_ID if self.viewing_this_computer => {
                        self.open_new_folder(
                            self.this_computer_path.clone(),
                            self.this_computer_root.clone(),
                        );
                    }
                    CLOSE_NEW_FOLDER_ID => {
                        self.show_new_folder = false;
                        self.new_folder_error = None;
                    }
                    CREATE_NEW_FOLDER_ID => {
                        if self.create_folder() {
                            self.show_new_folder = false;
                        }
                    }
                    SHOW_PICKER_NEW_FOLDER_ID => {
                        self.open_new_folder(
                            self.media_folder_picker_path.clone(),
                            self.folder_picker_root(),
                        );
                    }
                    _ => {}
                },
                event => {
                    log::debug!("wgui client {client_id} event: {event:?}");
                }
            }

            self.render_all_clients().await;
        }
    }

    async fn render_all_clients(&self) {
        for client_id in &self.client_ids {
            self.wgui.render(*client_id, self.render()).await;
        }
    }

    fn local_entries_at(&self, directory: &Path) -> Vec<LocalEntry> {
        let Ok(entries) = fs::read_dir(directory) else {
            return Vec::new();
        };

        let mut entries = entries
            .filter_map(Result::ok)
            .map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();
                let is_symlink = entry
                    .file_type()
                    .is_ok_and(|file_type| file_type.is_symlink());
                let metadata = entry.metadata().ok();
                let is_directory = metadata.as_ref().is_some_and(fs::Metadata::is_dir);
                let kind = if is_directory {
                    "Folder"
                } else if metadata.as_ref().is_some_and(fs::Metadata::is_file) {
                    "File"
                } else {
                    "Other"
                };
                let size = metadata
                    .as_ref()
                    .filter(|metadata| metadata.is_file())
                    .map_or_else(|| "—".to_owned(), |metadata| format_size(metadata.len()));
                let size_bytes = metadata
                    .as_ref()
                    .filter(|metadata| metadata.is_file())
                    .map_or(0, fs::Metadata::len);
                let modified_at = metadata
                    .as_ref()
                    .and_then(|metadata| metadata.modified().ok());
                let modified = modified_at.map_or_else(|| "—".to_owned(), format_modified);

                LocalEntry {
                    path,
                    name,
                    is_directory,
                    is_symlink,
                    kind,
                    size,
                    size_bytes,
                    modified,
                    modified_at,
                    media_root_id: None,
                }
            })
            .collect::<Vec<_>>();

        entries.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        });
        entries
    }

    fn go_to_parent(&mut self) {
        if self.this_computer_path == self.this_computer_root {
            return;
        }
        if let Some(parent) = self.this_computer_path.parent() {
            self.this_computer_path = parent.to_path_buf();
            self.tree_view_root = self.this_computer_path.clone();
            self.expanded_local_dirs.insert(self.tree_view_root.clone());
        }
    }

    fn go_to_breadcrumb(&mut self, depth: usize) {
        let relative_path = self
            .this_computer_path
            .strip_prefix(&self.this_computer_root)
            .unwrap_or(Path::new(""));
        let mut destination = self.this_computer_root.clone();
        for component in relative_path.components().take(depth) {
            destination.push(component.as_os_str());
        }

        let Ok(destination) = fs::canonicalize(destination) else {
            return;
        };
        if destination.starts_with(&self.this_computer_root) {
            self.this_computer_path = destination.clone();
            self.tree_view_root = destination;
            self.expanded_local_dirs.insert(self.tree_view_root.clone());
        }
    }

    fn tree_nodes(&self) -> Vec<TreeNode> {
        let mut nodes = Vec::new();
        self.append_tree_nodes(self.tree_root(), 0, &mut nodes);
        nodes
    }

    fn tree_root(&self) -> LocalEntry {
        LocalEntry {
            path: self.tree_view_root.clone(),
            name: self.tree_view_root.display().to_string(),
            is_directory: true,
            is_symlink: false,
            kind: "Folder",
            size: "—".to_owned(),
            size_bytes: 0,
            modified: "—".to_owned(),
            modified_at: None,
            media_root_id: None,
        }
    }

    fn append_tree_nodes(&self, entry: LocalEntry, depth: usize, nodes: &mut Vec<TreeNode>) {
        let can_expand = entry.is_directory && !entry.is_symlink;
        let expanded = can_expand && self.expanded_local_dirs.contains(&entry.path);
        let path = entry.path.clone();
        nodes.push(TreeNode { entry, depth });

        if expanded {
            for mut child in self.local_entries_at(&path) {
                if child.is_directory && !child.is_symlink {
                    let Ok(child_path) = fs::canonicalize(&child.path) else {
                        continue;
                    };
                    if !child_path.starts_with(&self.this_computer_root) {
                        continue;
                    }
                    child.path = child_path;
                }
                self.append_tree_nodes(child, depth + 1, nodes);
            }
        }
    }

    fn toggle_tree_directory(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if !entry.is_directory || entry.is_symlink {
            return;
        }
        let path = entry.path.clone();
        if !self.expanded_local_dirs.insert(path.clone()) {
            self.expanded_local_dirs.remove(&path);
        }
    }

    fn select_tree_directory(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if entry.is_directory && !entry.is_symlink {
            self.this_computer_path = entry.path.clone();
        }
    }

    fn navigate_to_tree_directory(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if entry.is_directory && !entry.is_symlink {
            self.this_computer_path = entry.path.clone();
            self.tree_view_root = entry.path.clone();
            self.expanded_local_dirs.insert(entry.path.clone());
        }
    }

    fn open_folder_context(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if entry.is_directory && !entry.is_symlink {
            self.folder_context = Some(FolderContext {
                name: entry.name.clone(),
                path: entry.path.clone(),
            });
        }
    }

    fn open_context_folder(&mut self) {
        let Some(context) = self.folder_context.as_ref() else {
            return;
        };
        self.this_computer_path = context.path.clone();
        self.tree_view_root = context.path.clone();
        self.expanded_local_dirs.insert(self.tree_view_root.clone());
        self.folder_context = None;
    }

    fn toggle_context_folder(&mut self) {
        let Some(context) = self.folder_context.as_ref() else {
            return;
        };
        let path = context.path.clone();
        if !self.expanded_local_dirs.insert(path.clone()) {
            self.expanded_local_dirs.remove(&path);
        }
        self.folder_context = None;
    }

    fn open_local_file(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| node.entry.clone()) else {
            return;
        };
        if entry.is_directory || entry.is_symlink {
            return;
        }
        let entries = nodes
            .into_iter()
            .map(|node| node.entry)
            .filter(|entry| !entry.is_directory && !entry.is_symlink)
            .collect::<Vec<_>>();
        let Some(viewer_index) = entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
        else {
            return;
        };
        self.file_viewer_entries = FileViewerEntries::Local(entries);
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = Some(viewer_index);
        }
    }

    fn open_media(&mut self, index: usize) {
        let Some(entry) = self.media_entries.get(index).cloned() else {
            return;
        };
        let indices = self.filtered_media_indices();
        let Some(viewer_index) = indices.iter().position(|candidate| *candidate == index) else {
            return;
        };
        self.file_viewer_entries = FileViewerEntries::Media(indices);
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = Some(viewer_index);
        }
    }

    fn open_audio(&mut self, index: usize) {
        let Some(entry) = self.audio_entries.get(index).cloned() else {
            return;
        };
        let entries = self
            .filtered_audio_indices()
            .into_iter()
            .filter_map(|index| self.audio_entries.get(index).cloned())
            .collect::<Vec<_>>();
        let Some(viewer_index) = entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
        else {
            return;
        };
        self.file_viewer_entries = FileViewerEntries::Local(entries);
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = Some(viewer_index);
        }
    }

    fn virtual_listing_entries(&self, directory_id: u32) -> Vec<FileListingEntry> {
        self.virtual_directory_entries
            .iter()
            .filter(|entry| entry.virtual_directory_id == directory_id)
            .map(FileListingEntry::from_virtual)
            .collect()
    }

    fn open_virtual_file(&mut self, directory_id: u32, index: usize) {
        let entries = self.virtual_listing_entries(directory_id);
        let Some(entry) = entries
            .get(index)
            .and_then(FileListingEntry::as_local_entry)
        else {
            return;
        };
        let viewer_entries = entries
            .iter()
            .filter_map(FileListingEntry::as_local_entry)
            .collect::<Vec<_>>();
        let Some(viewer_index) = viewer_entries
            .iter()
            .position(|candidate| candidate.path == entry.path)
        else {
            return;
        };
        self.file_viewer_entries = FileViewerEntries::Local(viewer_entries);
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = Some(viewer_index);
        }
    }

    fn sorted_media_indices(&self) -> Vec<usize> {
        let mut indices = (0..self.media_entries.len()).collect::<Vec<_>>();
        indices.sort_by(|left_index, right_index| {
            let left = &self.media_entries[*left_index];
            let right = &self.media_entries[*right_index];
            let ordering = match self.media_sort_key {
                MediaSortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                MediaSortKey::Type => media_kind(left).cmp(media_kind(right)),
                MediaSortKey::Size => left.size_bytes.cmp(&right.size_bytes),
                MediaSortKey::Replicas => self.media_index_entries[*left_index]
                    .replica_count
                    .cmp(&self.media_index_entries[*right_index].replica_count),
                MediaSortKey::Modified => left.modified_at.cmp(&right.modified_at),
            }
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
            if self.media_sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        indices
    }

    fn filtered_media_indices(&self) -> Vec<usize> {
        self.sorted_media_indices()
            .into_iter()
            .filter(|index| {
                self.media_scanned_folder_filter == "all"
                    || self
                        .media_entries
                        .get(*index)
                        .and_then(|entry| entry.media_root_id)
                        .is_some_and(|id| id.to_string() == self.media_scanned_folder_filter)
            })
            .collect()
    }

    fn filtered_audio_indices(&self) -> Vec<usize> {
        let mut indices = (0..self.audio_entries.len()).collect::<Vec<_>>();
        indices.sort_by(|left_index, right_index| {
            let left = &self.audio_entries[*left_index];
            let right = &self.audio_entries[*right_index];
            let ordering = match self.media_sort_key {
                MediaSortKey::Name => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
                MediaSortKey::Type => self.audio_index_entries[*left_index]
                    .mime_type
                    .cmp(&self.audio_index_entries[*right_index].mime_type),
                MediaSortKey::Size => left.size_bytes.cmp(&right.size_bytes),
                MediaSortKey::Replicas => self.audio_index_entries[*left_index]
                    .replica_count
                    .cmp(&self.audio_index_entries[*right_index].replica_count),
                MediaSortKey::Modified => left.modified_at.cmp(&right.modified_at),
            }
            .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
            if self.media_sort_descending {
                ordering.reverse()
            } else {
                ordering
            }
        });
        indices
            .into_iter()
            .filter(|index| {
                self.audio_scanned_folder_filter == "all"
                    || self
                        .audio_entries
                        .get(*index)
                        .and_then(|entry| entry.media_root_id)
                        .is_some_and(|id| id.to_string() == self.audio_scanned_folder_filter)
            })
            .collect()
    }

    fn toggle_media_sort(&mut self, key: MediaSortKey) {
        if self.media_sort_key == key {
            self.media_sort_descending = !self.media_sort_descending;
        } else {
            self.media_sort_key = key;
            self.media_sort_descending = false;
        }
    }

    fn select_video(&mut self, entry: &LocalEntry) -> bool {
        if !is_video_file(entry) {
            return false;
        }
        let Ok(path) = self.canonicalize_for_read(&entry.path) else {
            return false;
        };
        let Some(source_url) = self.entry_source_url(entry, &path) else {
            return false;
        };
        self.selected_file_hash = self
            .database
            .file_hash_for_location(&self.local_node_id, &path)
            .ok()
            .flatten();
        self.selected_video = Some(VideoFile {
            name: entry.name.clone(),
            size: entry.size.clone(),
            modified: entry.modified.clone(),
            source_url,
        });
        self.selected_image = None;
        self.selected_file = None;
        true
    }

    fn select_image(&mut self, entry: &LocalEntry) -> bool {
        if !is_image_file(entry) {
            return false;
        }
        let Ok(path) = self.canonicalize_for_read(&entry.path) else {
            return false;
        };
        let Some(source_url) = self.entry_source_url(entry, &path) else {
            return false;
        };
        self.selected_file_hash = self
            .database
            .file_hash_for_location(&self.local_node_id, &path)
            .ok()
            .flatten();
        self.selected_image = Some(ImageFile {
            name: entry.name.clone(),
            size: entry.size.clone(),
            modified: entry.modified.clone(),
            source_url,
            preview_url: entry
                .media_root_id
                .zip(self.selected_file_hash.as_deref())
                .map(|(folder_id, hash)| {
                    format!(
                        "/media-thumbnails/{folder_id}/{}?cached=1",
                        hex_encode(hash)
                    )
                }),
        });
        self.selected_video = None;
        self.selected_file = None;
        true
    }

    fn entry_source_url(&self, entry: &LocalEntry, path: &Path) -> Option<String> {
        if let Some(root_id) = entry.media_root_id {
            let root = self.media_paths.iter().find(|root| root.id == root_id)?;
            media_source_url(root, path)
        } else {
            local_source_url(&self.this_computer_root, path)
        }
    }

    fn select_file(&mut self, entry: &LocalEntry) -> bool {
        if entry.is_directory || entry.is_symlink {
            return false;
        }

        let Ok(path) = self.canonicalize_for_read(&entry.path) else {
            return false;
        };
        if !path.starts_with(&self.this_computer_root)
            && self.managed_folder_for_path(&path).is_none()
        {
            return false;
        }
        let bytes = match self.read_for_preview(&path) {
            Ok(bytes) => bytes,
            Err(_) => return false,
        };
        if bytes.is_empty() && entry.size_bytes() > 0 {
            return false;
        }
        let truncated = entry.size_bytes() > MAX_FILE_PREVIEW_BYTES;
        let text = String::from_utf8(bytes.clone()).ok();
        let mode = if text.is_some() {
            FileViewMode::Text
        } else {
            FileViewMode::Hex
        };

        self.selected_file_hash = self
            .database
            .file_hash_for_location(&self.local_node_id, &path)
            .ok()
            .flatten();
        self.selected_file = Some(FileViewer {
            name: entry.name.clone(),
            size: entry.size.clone(),
            bytes,
            text,
            mode,
            truncated,
        });
        self.selected_video = None;
        self.selected_image = None;
        true
    }

    fn managed_folder_for_path(&self, path: &Path) -> Option<&ManagedFolder> {
        self.managed_folders
            .values()
            .filter(|folder| folder.contains(path))
            .max_by_key(|folder| folder.root().components().count())
    }

    fn canonicalize_for_read(&self, path: &Path) -> Result<PathBuf> {
        match self.managed_folder_for_path(path) {
            Some(folder) => folder.canonicalize(path),
            None => Ok(fs::canonicalize(path)?),
        }
    }

    fn read_for_preview(&self, path: &Path) -> Result<Vec<u8>> {
        match self.managed_folder_for_path(path) {
            Some(folder) => folder.read(path, Some(MAX_FILE_PREVIEW_BYTES)),
            None => {
                let file = fs::File::open(path)?;
                let mut bytes = Vec::new();
                file.take(MAX_FILE_PREVIEW_BYTES).read_to_end(&mut bytes)?;
                Ok(bytes)
            }
        }
    }

    fn select_viewer_entry(&mut self, entry: &LocalEntry) -> bool {
        if is_video_file(entry) {
            self.select_video(entry)
        } else if is_image_file(entry) {
            self.select_image(entry)
        } else {
            self.select_file(entry)
        }
    }

    fn navigate_file_viewer(&mut self, direction: isize) {
        let Some(current_index) = self.file_viewer_index else {
            return;
        };
        let next_index = if direction < 0 {
            current_index.checked_sub(1)
        } else {
            current_index
                .checked_add(1)
                .filter(|index| *index < self.file_viewer_entries.len(&self.filtered_files))
        };
        let Some(entry) = next_index.and_then(|index| self.file_viewer_entry_at(index)) else {
            return;
        };
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = next_index;
        }
    }

    fn close_file_viewer(&mut self) {
        self.selected_video = None;
        self.selected_image = None;
        self.selected_file = None;
        self.selected_file_hash = None;
        self.file_viewer_entries.clear();
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        self.show_virtual_directory_picker = false;
        self.virtual_directory_error = None;
    }

    fn reload_virtual_directories(&mut self) {
        match self.database.virtual_directories() {
            Ok(directories) => self.virtual_directories = directories,
            Err(error) => log::warn!("could not load virtual directories: {error:#}"),
        }
        match self.database.virtual_directory_entries(&self.local_node_id) {
            Ok(entries) => self.virtual_directory_entries = entries,
            Err(error) => log::warn!("could not load virtual directory entries: {error:#}"),
        }
    }

    fn add_selected_file_to_virtual_directory(&mut self, directory_id: u32) {
        let Some(hash) = self.selected_file_hash.as_deref() else {
            self.virtual_directory_error = Some("This file has not been indexed yet.".to_owned());
            return;
        };
        match self
            .database
            .add_file_to_virtual_directory(directory_id, hash)
        {
            Ok(()) => {
                self.reload_virtual_directories();
                self.show_virtual_directory_picker = false;
                self.new_virtual_directory_name.clear();
                self.virtual_directory_error = None;
            }
            Err(error) => {
                self.virtual_directory_error = Some(format!("Could not add file: {error:#}"));
            }
        }
    }

    fn create_virtual_directory_for_selected_file(&mut self) {
        match self
            .database
            .create_virtual_directory(&self.new_virtual_directory_name)
        {
            Ok(directory) => {
                self.reload_virtual_directories();
                self.add_selected_file_to_virtual_directory(directory.id);
            }
            Err(error) => {
                self.virtual_directory_error =
                    Some(format!("Could not create directory: {error:#}"));
            }
        }
    }

    fn create_empty_virtual_directory(&mut self) {
        match self
            .database
            .create_virtual_directory(&self.new_virtual_directory_name)
        {
            Ok(_) => {
                self.reload_virtual_directories();
                self.show_create_virtual_directory = false;
                self.new_virtual_directory_name.clear();
                self.virtual_directory_error = None;
            }
            Err(error) => {
                self.virtual_directory_error =
                    Some(format!("Could not create directory: {error:#}"));
            }
        }
    }

    fn set_file_view_mode(&mut self, mode: FileViewMode) {
        if let Some(viewer) = &mut self.selected_file {
            viewer.mode = mode;
        }
    }

    async fn save_added_source(&mut self) {
        let name = self.new_source_name.trim();
        if name.is_empty() {
            return;
        }
        let Ok(path) = fs::canonicalize(self.new_source_path.trim()) else {
            log::warn!("source path '{}' is not accessible", self.new_source_path);
            return;
        };
        if !path.is_dir() {
            log::warn!("source path '{}' is not a directory", path.display());
            return;
        }
        let config = serde_json::to_string(&LocalSourceConfig {
            path: path.to_string_lossy().into_owned(),
        })
        .expect("serialize local source config");
        if let Err(error) = validate_source_config("local", 1, &config) {
            log::warn!("invalid local source: {error}");
            return;
        }
        let source = Source {
            id: 0,
            source_key: uuid::Uuid::new_v4().to_string(),
            name: name.to_owned(),
            source_type: "local".to_owned(),
            config_schema_version: 1,
            config,
            enabled: true,
        };
        let source = match self.database.save_source(source).await {
            Ok(source) => source,
            Err(error) => {
                log::error!("failed saving source: {error:#}");
                return;
            }
        };
        self.sources.push(source);
        self.new_source_name.clear();
        self.new_source_path.clear();
        self.show_add_source = false;
    }

    fn save_config(&self) {
        if let Err(error) = config::save(&self.config, &self.config_path) {
            log::error!("failed saving PuppyDrive settings: {error:#}");
        }
    }

    fn add_inbox(&mut self) -> bool {
        let name = self.new_inbox_name.trim();
        if name.is_empty() {
            self.inbox_error = Some("Give the Inbox a name.".to_owned());
            return false;
        }
        if self
            .config
            .inboxes
            .iter()
            .any(|inbox| inbox.name.eq_ignore_ascii_case(name))
        {
            self.inbox_error = Some("An Inbox with that name already exists.".to_owned());
            return false;
        }
        let folder = match inbox_folder_path(&self.new_inbox_folder) {
            Ok(folder) => folder,
            Err(error) => {
                self.inbox_error = Some(format!("Invalid Inbox folder: {error}"));
                return false;
            }
        };
        if let Err(error) = resolve_upload_folder(&self.configured_this_computer_root, &folder) {
            self.inbox_error = Some(format!("Inbox folder is unavailable: {error}"));
            return false;
        }

        let mut updated = self.config.clone();
        updated.inboxes.push(InboxConfig {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.to_owned(),
            folder,
        });
        if let Err(error) = config::save(&updated, &self.config_path) {
            log::error!("failed saving Inbox: {error:#}");
            self.inbox_error = Some("Could not save the Inbox.".to_owned());
            return false;
        }
        self.config = updated;
        self.sync_served_inboxes();
        self.new_inbox_name.clear();
        self.new_inbox_folder.clear();
        self.inbox_error = None;
        true
    }

    fn remove_inbox(&mut self, index: usize) {
        if index >= self.config.inboxes.len() {
            return;
        }
        let mut updated = self.config.clone();
        updated.inboxes.remove(index);
        if let Err(error) = config::save(&updated, &self.config_path) {
            log::error!("failed removing Inbox: {error:#}");
            self.inbox_error = Some("Could not remove the Inbox.".to_owned());
            return;
        }
        self.config = updated;
        self.sync_served_inboxes();
        self.inbox_error = None;
    }

    fn sync_served_inboxes(&self) {
        if let Ok(mut inboxes) = self.served_inboxes.write() {
            *inboxes = self.config.inboxes.clone();
        }
    }

    fn open_new_folder(&mut self, parent: PathBuf, root: PathBuf) {
        self.new_folder_parent = parent;
        self.new_folder_root = root;
        self.new_folder_name.clear();
        self.new_folder_error = None;
        self.show_new_folder = true;
    }

    fn folder_picker_root(&self) -> PathBuf {
        if self.folder_picker_target == FolderPickerTarget::Inbox {
            self.configured_this_computer_root.clone()
        } else {
            self.media_folder_picker_path
                .ancestors()
                .last()
                .unwrap_or_else(|| Path::new("/"))
                .to_path_buf()
        }
    }

    fn create_folder(&mut self) -> bool {
        match create_folder_at(
            &self.new_folder_root,
            &self.new_folder_parent,
            &self.new_folder_name,
        ) {
            Ok(folder) => {
                if self.new_folder_parent == self.this_computer_path {
                    self.expanded_local_dirs
                        .insert(self.this_computer_path.clone());
                }
                self.expanded_local_dirs.insert(folder);
                self.new_folder_name.clear();
                self.new_folder_error = None;
                true
            }
            Err(error) => {
                self.new_folder_error = Some(error.to_string());
                false
            }
        }
    }

    fn queue_media_index(&mut self, trigger: ScanTrigger) {
        log::info!(
            "queueing scan for all {} scanned folders",
            self.media_paths.len()
        );
        self.indexer.request_scan(
            self.media_paths.clone(),
            self.local_node_id.clone(),
            self.config.media.max_items,
            self.config.media.max_directories,
            self.config.media.ignored_directory_names.clone(),
            self.config
                .media
                .max_file_size_mb
                .saturating_mul(1_024 * 1_024),
            trigger,
        );
    }

    fn queue_media_index_for(&mut self, folder_id: u32, trigger: ScanTrigger) {
        if self
            .index_status
            .get(&folder_id)
            .is_some_and(|status| status.scanning || status.queued)
        {
            log::info!(
                "scan requested for scanned folder {folder_id}, but it is already queued or scanning"
            );
            return;
        }
        let folders: Vec<MediaScanPath> = self
            .media_paths
            .iter()
            .filter(|folder| folder.id == folder_id)
            .cloned()
            .collect();
        if folders.is_empty() {
            log::warn!("scan requested for unknown scanned folder id {folder_id}");
            return;
        }
        log::info!(
            "queueing scan for scanned folder {folder_id}: {}",
            folders[0].path
        );
        self.indexer.request_scan(
            folders,
            self.local_node_id.clone(),
            self.config.media.max_items,
            self.config.media.max_directories,
            self.config.media.ignored_directory_names.clone(),
            self.config
                .media
                .max_file_size_mb
                .saturating_mul(1_024 * 1_024),
            trigger,
        );
        self.index_status.insert(
            folder_id,
            FolderIndexStatus {
                queued: true,
                ..Default::default()
            },
        );
    }

    fn stop_media_index_for(&mut self, folder_id: u32) {
        if !self.indexer.cancel_scan(folder_id) {
            log::debug!("stop requested for scanned folder {folder_id}, but no scan is active");
            return;
        }
        let status = self.index_status.entry(folder_id).or_default();
        status.stopping = true;
        status.message = None;
        log::info!("stop requested for scanned folder {folder_id}");
    }

    fn handle_indexer_event(&mut self, event: IndexerEvent) {
        match event {
            IndexerEvent::Started { folder_ids } => {
                log::info!("indexer started scan for folders {folder_ids:?}");
                for folder_id in folder_ids {
                    let stopping = self
                        .index_status
                        .get(&folder_id)
                        .is_some_and(|status| status.stopping);
                    self.index_status.insert(
                        folder_id,
                        FolderIndexStatus {
                            scanning: true,
                            queued: false,
                            stopping,
                            ..Default::default()
                        },
                    );
                }
            }
            IndexerEvent::Progress {
                folder_id,
                directories_scanned,
                media_files_indexed,
                current_path,
            } => {
                let status = self.index_status.entry(folder_id).or_default();
                status.scanning = true;
                status.directories_scanned = directories_scanned;
                status.media_files_indexed = media_files_indexed;
                status.current_path = current_path;
            }
            IndexerEvent::FolderFinished { history } => {
                let folder_id = history.scanned_folder_id;
                log::info!("indexer finished scanned folder {folder_id}");
                if let Some(status) = self.index_status.get_mut(&folder_id) {
                    status.scanning = false;
                    status.stopping = false;
                    status.outcome = Some(history.outcome);
                    status.message = (history.outcome != ScanOutcome::Completed)
                        .then(|| history.error_message.clone())
                        .flatten();
                }
                if self.selected_scanned_folder_id == Some(folder_id) {
                    self.refresh_selected_scanned_folder_history();
                }
            }
            IndexerEvent::Finished { truncated, errors } => {
                if errors.is_empty() {
                    log::info!("indexer scan completed (truncated: {truncated})");
                } else {
                    log::warn!(
                        "indexer scan completed with {} errors (truncated: {truncated}): {}",
                        errors.len(),
                        errors.join("; ")
                    );
                }
                self.media_scan_truncated = truncated;
                self.media_scan_errors = errors.clone();
                for status in self
                    .index_status
                    .values_mut()
                    .filter(|status| status.queued)
                {
                    status.queued = false;
                    if !errors.is_empty() {
                        status.message = Some(errors.join("; "));
                    }
                }
                self.reload_media_cache();
            }
            IndexerEvent::Failed { message } => {
                log::error!("Media indexer: {message}");
                for status in self
                    .index_status
                    .values_mut()
                    .filter(|status| status.scanning)
                {
                    status.scanning = false;
                    status.stopping = false;
                    status.message = Some(message.clone());
                    status.outcome = Some(ScanOutcome::Failed);
                }
            }
        }
    }

    fn reload_media_cache(&mut self) {
        match (
            self.database.cached_media(&self.local_node_id),
            self.database.cached_audio(&self.local_node_id),
        ) {
            (Ok(media), Ok(audio)) => {
                let media = available_media_entries(media, &self.managed_folders);
                let audio = available_media_entries(audio, &self.managed_folders);
                self.media_entries = local_entries_from_index(media.clone());
                self.media_index_entries = media;
                self.audio_entries = local_entries_from_index(audio.clone());
                self.audio_index_entries = audio;
                let page_count = self.media_entries.len().div_ceil(FILES_PAGE_SIZE);
                self.media_page = if page_count == 0 {
                    0
                } else {
                    self.media_page.min(page_count - 1)
                };
                let audio_page_count = self.audio_entries.len().div_ceil(FILES_PAGE_SIZE);
                self.audio_page = if audio_page_count == 0 {
                    0
                } else {
                    self.audio_page.min(audio_page_count - 1)
                };
                self.indexed_files = self
                    .database
                    .cached_files(&self.local_node_id)
                    .unwrap_or_else(|error| {
                        log::error!("failed loading persistent file index: {error:#}");
                        Vec::new()
                    });
                self.refresh_filtered_files(false);
            }
            (Err(error), _) | (_, Err(error)) => {
                log::error!("failed loading persistent Media index: {error:#}")
            }
        }
    }

    fn refresh_selected_scanned_folder_history(&mut self) {
        let Some(folder_id) = self.selected_scanned_folder_id else {
            return;
        };
        match self.database.scanned_folder_scan_history(folder_id) {
            Ok(history) => self.selected_scanned_folder_history = history,
            Err(error) => {
                log::error!("failed loading scanned-folder history: {error:#}");
                self.selected_scanned_folder_history.clear();
            }
        }
    }

    fn scan_status_text(&self, path: &MediaScanPath) -> String {
        if !Path::new(&path.path).is_dir() {
            "Folder is unavailable".to_owned()
        } else if !path.enabled {
            "Media indexing paused".to_owned()
        } else if let Some(status) = self.index_status.get(&path.id) {
            if status.stopping {
                "Stopping scan…".to_owned()
            } else if status.scanning {
                format!("Scanning • {} files", status.media_files_indexed)
            } else if status.queued {
                "Scan queued".to_owned()
            } else if let Some(message) = &status.message {
                match status.outcome {
                    Some(ScanOutcome::Incomplete) => format!("Scan incomplete • {message}"),
                    _ => format!("Indexing error • {message}"),
                }
            } else {
                "Media indexing active".to_owned()
            }
        } else {
            "Media indexing active".to_owned()
        }
    }

    fn activate_files_root(&mut self, root: PathBuf, source_id: Option<u32>) {
        self.active_page = AppPage::Files;
        self.viewing_this_computer = true;
        self.active_source_id = source_id;
        self.this_computer_root = root.clone();
        self.this_computer_path = root.clone();
        self.tree_view_root = root.clone();
        self.expanded_local_dirs.clear();
        self.expanded_local_dirs.insert(root.clone());
        if let Ok(mut served_root) = self.active_files_root.write() {
            *served_root = root;
        }
    }

    fn activate_source(&mut self, id: u32) {
        let Some(source) = self
            .sources
            .iter()
            .find(|source| source.id == id && source.enabled)
        else {
            return;
        };
        let Some(path) = local_source_path(source) else {
            return;
        };
        let Ok(path) = fs::canonicalize(path) else {
            return;
        };
        if path.is_dir() {
            self.activate_files_root(path, Some(id));
        }
    }

    async fn add_media_path_from_input(&mut self) {
        let value = self.new_media_path.trim().to_owned();
        self.add_media_path(PathBuf::from(value)).await;
    }

    fn open_folder_picker(&mut self, target: FolderPickerTarget) {
        self.folder_picker_target = target;
        match target {
            FolderPickerTarget::Media => {
                self.media_path_error = None;
                if !self.media_folder_picker_path.is_dir() {
                    self.media_folder_picker_path =
                        media_folder_picker_start_path(&self.configured_this_computer_root);
                }
            }
            FolderPickerTarget::Inbox => {
                self.inbox_error = None;
                self.media_folder_picker_path = self.configured_this_computer_root.clone();
            }
            FolderPickerTarget::Source => {
                if !self.media_folder_picker_path.is_dir() {
                    self.media_folder_picker_path =
                        media_folder_picker_start_path(&self.configured_this_computer_root);
                }
            }
        }
        self.show_media_folder_picker = true;
    }

    fn media_folder_picker_entries(&self) -> Vec<(String, PathBuf)> {
        let Ok(entries) = fs::read_dir(&self.media_folder_picker_path) else {
            return Vec::new();
        };
        let mut folders = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let path = fs::canonicalize(entry.path()).ok()?;
                (path.is_dir()
                    && (self.folder_picker_target != FolderPickerTarget::Inbox
                        || path.starts_with(&self.configured_this_computer_root)))
                .then(|| (entry.file_name().to_string_lossy().into_owned(), path))
            })
            .collect::<Vec<_>>();
        folders.sort_by(|left, right| left.0.to_lowercase().cmp(&right.0.to_lowercase()));
        folders
    }

    fn navigate_media_folder_picker(&mut self, index: usize) {
        let Some((_, path)) = self.media_folder_picker_entries().get(index).cloned() else {
            return;
        };
        self.media_folder_picker_path = path;
    }

    fn media_folder_picker_parent(&mut self) {
        let Some(parent) = self.media_folder_picker_path.parent() else {
            return;
        };
        if let Ok(parent) = fs::canonicalize(parent) {
            if self.folder_picker_target == FolderPickerTarget::Inbox
                && !parent.starts_with(&self.configured_this_computer_root)
            {
                return;
            }
            self.media_folder_picker_path = parent;
        }
    }

    async fn select_media_folder_picker(&mut self) {
        let path = self.media_folder_picker_path.clone();
        match self.folder_picker_target {
            FolderPickerTarget::Media => {
                self.add_media_path(path).await;
                if self.media_path_error.is_none() {
                    self.show_media_folder_picker = false;
                }
            }
            FolderPickerTarget::Inbox => {
                let Ok(folder) = path.strip_prefix(&self.configured_this_computer_root) else {
                    self.inbox_error =
                        Some("Inbox folders must stay inside the server root.".to_owned());
                    return;
                };
                if folder.as_os_str().is_empty() {
                    self.inbox_error = Some("Choose a folder inside the server root.".to_owned());
                    return;
                }
                self.new_inbox_folder = folder.to_string_lossy().into_owned();
                self.inbox_error = None;
                self.show_media_folder_picker = false;
            }
            FolderPickerTarget::Source => {
                self.new_source_path = path.to_string_lossy().into_owned();
                self.show_media_folder_picker = false;
            }
        }
    }

    async fn include_context_folder_in_media(&mut self) {
        let Some(path) = self
            .folder_context
            .as_ref()
            .map(|context| context.path.clone())
        else {
            return;
        };
        self.folder_context = None;
        self.add_media_path(path).await;
    }

    async fn add_media_path(&mut self, path: PathBuf) {
        let path = match fs::canonicalize(&path) {
            Ok(path) if path.is_dir() => path,
            _ => {
                self.media_path_error =
                    Some(format!("{} is not an accessible directory", path.display()));
                return;
            }
        };
        let path_string = path.to_string_lossy().into_owned();
        if self
            .media_paths
            .iter()
            .any(|stored| stored.path == path_string)
        {
            self.media_path_error = Some("That folder is already included.".to_owned());
            return;
        }
        let row = MediaScanPath {
            id: 0,
            path: path_string,
            enabled: true,
            indexers: r#"["media"]"#.to_owned(),
        };
        match self.database.save_media_path(row).await {
            Ok(row) => {
                self.media_paths.push(row);
                self.new_media_path.clear();
                self.media_path_error = None;
                self.media_paths_changed();
            }
            Err(error) => {
                log::error!("failed saving Scanned folder: {error:#}");
                self.media_path_error = Some("Could not save the Scanned folder.".to_owned());
            }
        }
    }

    async fn toggle_media_path(&mut self, id: u32) {
        let Some(index) = self.media_paths.iter().position(|path| path.id == id) else {
            return;
        };
        let mut updated = self.media_paths[index].clone();
        updated.enabled = !updated.enabled;
        match self.database.save_media_path(updated).await {
            Ok(updated) => {
                self.media_paths[index] = updated;
                self.media_paths_changed();
            }
            Err(error) => log::error!("failed updating Scanned folder: {error:#}"),
        }
    }

    fn remove_media_path(&mut self, id: u32) {
        match self.database.delete_media_path(id) {
            Ok(true) => {
                self.media_paths.retain(|path| path.id != id);
                self.media_paths_changed();
            }
            Ok(false) => {}
            Err(error) => log::error!("failed forgetting Scanned folder: {error:#}"),
        }
    }

    fn media_paths_changed(&mut self) {
        self.managed_folders = managed_folders(&self.media_paths);
        if let Ok(mut served_paths) = self.served_media_paths.write() {
            *served_paths = self.media_paths.clone();
        }
        if let Ok(mut served_folders) = self.served_managed_folders.write() {
            *served_folders = self.managed_folders.clone();
        }
        self.reconfigure_media_watcher();
        self.queue_media_index(ScanTrigger::FilesystemChange);
    }

    fn reconfigure_media_watcher(&mut self) {
        match build_media_watcher(
            &self.media_paths,
            Duration::from_millis(self.config.media.watch_debounce_ms.max(1)),
            &self.config.media.ignored_directory_names,
        ) {
            Ok((watcher, changes, watched_paths)) => {
                self.media_watcher = watcher;
                self.media_change_rx = changes;
                self.watched_media_paths = watched_paths;
            }
            Err(error) => log::warn!("failed reconfiguring Media watcher: {error:#}"),
        }
    }
}

fn card(body: Item) -> Item {
    body.border("1px solid #dfe7e9").background_color("#ffffff")
}

fn source_nav_button(source: &Source, active: bool, available: bool) -> Item {
    let background = if active { "#e5f4f7" } else { "#f8fbfc" };
    let color = if active { "#0f6175" } else { "#374151" };
    let status = if available { "●" } else { "○" };
    button(&format!(
        "□  {}                       {status}",
        source.name
    ))
    .id(ADDITIONAL_SOURCE_ID)
    .inx(source.id)
    .width(180)
    .padding(5)
    .border("1px solid transparent")
    .background_color(background)
    .color(color)
    .text_align("left")
    .cursor("pointer")
}

fn this_computer_nav_item(active: bool) -> Item {
    let background = if active { "#e5f4f7" } else { "#f8fbfc" };
    let color = if active { "#0f6175" } else { "#374151" };

    button("▣  This Computer                       ●")
        .id(THIS_COMPUTER_SOURCE_ID)
        .width(180)
        .padding(5)
        .border("1px solid transparent")
        .background_color(background)
        .color(color)
        .cursor("pointer")
}

fn scanned_folder_nav_item(folder: &MediaScanPath, available: bool) -> Item {
    let (status, status_color) = if !folder.enabled {
        ("Paused", "#6b7280")
    } else if available {
        ("● Available", "#15803d")
    } else {
        ("○ Unavailable", "#b42318")
    };
    card(
        vstack([
            link(&format!("/scanned-folders/{}", folder.id), &folder.path)
                .break_words(true)
                .color("#0f6175")
                .cursor("pointer"),
            text(status).color(status_color),
        ])
        .spacing(2)
        .padding(6),
    )
    .width(180)
}

fn nav_link(label: &str, href: &str, active: bool) -> Item {
    let (background, color) = if active {
        ("#e5f4f7", "#0f6175")
    } else {
        ("transparent", "#374151")
    };

    link(href, label)
        .width(180)
        .padding(5)
        .border("1px solid transparent")
        .background_color(background)
        .color(color)
        .cursor("pointer")
}

fn media_kind(entry: &LocalEntry) -> &'static str {
    if is_image_file(entry) {
        "Image"
    } else {
        "Video"
    }
}

fn media_sort_key_name(key: MediaSortKey) -> &'static str {
    match key {
        MediaSortKey::Name => "name",
        MediaSortKey::Type => "type",
        MediaSortKey::Size => "sizeValue",
        MediaSortKey::Replicas => "replicaCount",
        MediaSortKey::Modified => "modifiedValue",
    }
}

fn media_sort_key_from_payload(payload: &serde_json::Value) -> Option<MediaSortKey> {
    let key = payload.get("key")?.as_str()?;
    match key {
        "name" => Some(MediaSortKey::Name),
        "sizeValue" => Some(MediaSortKey::Size),
        "replicaCount" => Some(MediaSortKey::Replicas),
        "modifiedValue" => Some(MediaSortKey::Modified),
        _ => None,
    }
}

fn settings_section(title: &str, rows: impl IntoIterator<Item = Item>) -> Item {
    vstack([
        text(title).color("#0f6175").padding_bottom(2),
        vstack(rows).spacing(6),
    ])
    .spacing(4)
}

impl App {
    fn refresh_filtered_files(&mut self, reset_page: bool) {
        let mut entries = self
            .indexed_files
            .iter()
            .filter(|entry| {
                self.files_scanned_folder_filter == "all"
                    || entry
                        .scanned_folder_id
                        .is_some_and(|id| id.to_string() == self.files_scanned_folder_filter)
            })
            .map(FileListingEntry::from_indexed)
            .filter(|entry| match self.files_mime_filter.as_str() {
                "images" => entry.is_image(),
                "videos" => entry.is_video(),
                "other" => !entry.is_image() && !entry.is_video(),
                _ => true,
            })
            .collect::<Vec<_>>();
        entries.sort_by(|left, right| match self.files_sort.as_str() {
            "name" => left.name.to_lowercase().cmp(&right.name.to_lowercase()),
            "size" => right.size.cmp(&left.size),
            _ => right.modified_at.cmp(&left.modified_at),
        });
        self.filtered_files = entries;
        let page_count = self.filtered_files.len().div_ceil(FILES_PAGE_SIZE);
        if reset_page || page_count == 0 {
            self.files_page = 0;
        } else {
            self.files_page = self.files_page.min(page_count - 1);
        }
    }

    fn file_viewer_entry_at(&self, index: usize) -> Option<LocalEntry> {
        match &self.file_viewer_entries {
            FileViewerEntries::Local(entries) => entries.get(index).cloned(),
            FileViewerEntries::Indexed => self
                .filtered_files
                .get(index)
                .and_then(FileListingEntry::as_local_entry),
            FileViewerEntries::Media(indices) => indices
                .get(index)
                .and_then(|index| self.media_entries.get(*index))
                .cloned(),
        }
    }

    fn open_indexed_file(&mut self, index: usize) {
        let Some(entry) = self
            .filtered_files
            .get(index)
            .and_then(FileListingEntry::as_local_entry)
        else {
            return;
        };
        self.file_viewer_entries = FileViewerEntries::Indexed;
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
        if self.select_viewer_entry(&entry) {
            self.file_viewer_index = Some(index);
        }
    }

    fn media_listing_entries(&self) -> Vec<FileListingEntry> {
        self.media_entries
            .iter()
            .zip(&self.media_index_entries)
            .map(|(entry, indexed)| FileListingEntry::from_media(entry, indexed))
            .collect()
    }

    fn file_source_status(&self, entry: &FileListingEntry) -> (bool, String) {
        let mut paths = entry
            .hash
            .as_deref()
            .and_then(|hash| {
                self.database
                    .file_replica_paths(&self.local_node_id, hash)
                    .ok()
            })
            .unwrap_or_default();
        if paths.is_empty() {
            if let Some(path) = &entry.path {
                paths.push(path.clone());
            }
        }

        let mut online = false;
        let mut locations = Vec::with_capacity(paths.len());
        for path in paths {
            let available = fs::metadata(&path).is_ok();
            online |= available;
            let status = if available {
                "● Online"
            } else {
                "○ Offline"
            };
            let source = self
                .sources
                .iter()
                .find(|source| {
                    source.enabled
                        && local_source_path(source).is_some_and(|root| path.starts_with(root))
                })
                .map(|source| source.name.clone())
                .unwrap_or_else(|| "This Computer".to_owned());
            locations.push(format!("{status} — {source}: {}", path.display()));
        }

        if locations.is_empty() {
            locations.push("○ Offline — No current source location".to_owned());
        }
        (
            online,
            format!("Sources containing this file:\n{}", locations.join("\n")),
        )
    }

    fn file_listing(
        &self,
        entries: &[FileListingEntry],
        view_mode: &str,
        thumbnail_size: u32,
        open_id: u32,
        indices: impl IntoIterator<Item = usize>,
        media_sort: Option<(MediaSortKey, bool)>,
    ) -> Item {
        let indices = indices.into_iter().collect::<Vec<_>>();
        if entries.is_empty() {
            return vstack([text("No files found.").color("#6b7280")])
                .grow(1)
                .padding(24)
                .background_color("#f8fafb");
        }
        if view_mode == "table" {
            let table_component_name = match open_id {
                INDEXED_FILE_VIEW_ID => "indexed-file-table",
                LOCAL_MEDIA_VIEW_ID => "media-file-table",
                VIRTUAL_FILE_VIEW_ID => "virtual-file-table",
                _ => "file-table",
            };
            let rows = indices.into_iter().filter_map(|index| {
                let entry = entries.get(index)?;
                let (online, source_tooltip) = self.file_source_status(entry);
                Some(serde_json::json!({
                    "index": index,
                    "icon": if entry.is_image() { "▧" } else if entry.is_video() { "▣" } else { "□" },
                    "name": entry.name,
                    "online": online,
                    "sourceTooltip": source_tooltip,
                    "type": entry.file_type(),
                    "size": format_size(entry.size),
                    "sizeValue": entry.size,
                    "hash": entry.hash_label(),
                    "replicas": replica_label(entry.replica_count),
                    "replicaCount": entry.replica_count,
                    "modified": entry.modified_at.map_or_else(|| "—".to_owned(), format_modified),
                    "modifiedValue": entry.modified_at.and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map(|duration| duration.as_millis()).unwrap_or(0),
                }))
            }).collect::<Vec<_>>();
            return custom_component(
                table_component_name,
                self.file_table_asset.url(),
                serde_json::json!({
                    "rows": rows,
                    "serverSort": media_sort.is_some(),
                    "sortKey": media_sort.map(|(key, _)| media_sort_key_name(key)),
                    "sortDescending": media_sort.is_some_and(|(_, descending)| descending),
                }),
            )
            .custom_event("open", open_id)
            .custom_event("sort", MEDIA_TABLE_SORT_ID)
            .grow(1)
            .fill(true)
            .width(0)
            .overflow("hidden");
        }
        let tile_height = thumbnail_size.saturating_mul(3) / 4 + 60;
        let tiles = indices.into_iter().filter_map(|index| {
            let entry = entries.get(index)?;
            let source_url = entry.path.as_ref().and_then(|path| {
                let root = self.media_paths.iter().find(|root| Some(root.id) == entry.media_root_id)?;
                media_source_url(root, path)
            });
            let thumbnail_url = entry.is_image().then(|| media_thumbnail_url(entry)).flatten();
            let preview_url = if entry.is_image() {
                thumbnail_url
            } else {
                source_url
            };
            Some(if (entry.is_image() || entry.is_video()) && preview_url.is_some() {
                custom_component(
                    "media-tile",
                    self.media_tile_asset.url(),
                    serde_json::json!({
                        "index": index,
                        "name": entry.name,
                        "kind": if entry.is_image() { "image" } else { "video" },
                        "src": preview_url,
                        "size": format_size(entry.size),
                        "modified": entry.modified_at.map_or_else(|| "—".to_owned(), format_modified),
                        "thumbnailSize": thumbnail_size,
                    }),
                )
                .custom_event("open", open_id)
                .width(thumbnail_size)
                .height(tile_height)
            } else {
                button(&format!("□\n{}\n{}", entry.name, format_size(entry.size)))
                    .id(open_id)
                    .inx(index as u32)
                    .width(thumbnail_size)
                    .height(tile_height)
                    .padding(10)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175")
                    .text_align("left")
            })
        });
        hstack(tiles)
            .wrap(true)
            .spacing(12)
            .grow(1)
            .padding(4)
            .overflow("auto")
    }

    fn media_panel(&self) -> Item {
        let media_indices = self.filtered_media_indices();
        let image_count = media_indices
            .iter()
            .filter_map(|index| self.media_entries.get(*index))
            .filter(|entry| is_image_file(entry))
            .count();
        let video_count = media_indices.len().saturating_sub(image_count);
        let mut media_summary = format!(
            "{image_count} images  •  {video_count} videos from {} folders",
            self.media_paths.iter().filter(|path| path.enabled).count()
        );
        if self.media_scanned_folder_filter != "all" {
            media_summary.push_str("  •  filtered");
        }
        if self.media_scan_truncated {
            media_summary.push_str("  •  Scan limit reached");
        }
        if !self.media_scan_errors.is_empty() {
            media_summary.push_str(&format!(
                "  •  {} folders unavailable",
                self.media_scan_errors.len()
            ));
        }
        let showing_thumbnails = self.media_view_mode != "table";
        let thumbnail_size = self.media_thumbnail_size.clamp(140, 320) as u32;
        let listing_entries = self.media_listing_entries();
        let page_count = media_indices.len().div_ceil(FILES_PAGE_SIZE);
        let page = self.media_page.min(page_count.saturating_sub(1));
        let page_start = page.saturating_mul(FILES_PAGE_SIZE);
        let page_end = (page_start + FILES_PAGE_SIZE).min(media_indices.len());
        let media_content = if media_indices.is_empty() {
            vstack([
                text("No media found").color("#374151"),
                text("Images and videos from active Scanned folders will appear here.")
                    .color("#6b7280"),
            ])
            .grow(1)
            .spacing(4)
            .padding(24)
            .background_color("#f8fafb")
        } else {
            self.file_listing(
                &listing_entries,
                &self.media_view_mode,
                thumbnail_size,
                LOCAL_MEDIA_VIEW_ID,
                media_indices
                    .iter()
                    .copied()
                    .skip(page_start)
                    .take(FILES_PAGE_SIZE),
                Some((self.media_sort_key, self.media_sort_descending)),
            )
        };
        let pagination = if media_indices.is_empty() {
            hstack(Vec::<Item>::new())
        } else {
            hstack([
                text(&format!(
                    "Showing {}–{} of {}",
                    page_start + 1,
                    page_end,
                    media_indices.len()
                ))
                .grow(1)
                .color("#6b7280"),
                button("← Previous")
                    .id(MEDIA_PREVIOUS_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page > 0 { "#ffffff" } else { "#f3f4f6" })
                    .color(if page > 0 { "#0f6175" } else { "#9ca3af" }),
                button("Next →")
                    .id(MEDIA_NEXT_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page + 1 < page_count {
                        "#ffffff"
                    } else {
                        "#f3f4f6"
                    })
                    .color(if page + 1 < page_count {
                        "#0f6175"
                    } else {
                        "#9ca3af"
                    }),
            ])
            .spacing(6)
            .padding_top(8)
        };

        let size_control = if showing_thumbnails {
            hstack([
                text(&format!("{thumbnail_size}px")).color("#6b7280"),
                slider()
                    .id(MEDIA_THUMBNAIL_SIZE_ID)
                    .min(140)
                    .max(320)
                    .step(20)
                    .ivalue(self.media_thumbnail_size)
                    .width(150),
            ])
            .spacing(6)
        } else {
            hstack(Vec::<Item>::new())
        };
        let mut scanned_folder_options = vec![option("all", "All scanned folders")];
        scanned_folder_options.extend(self.media_paths.iter().map(|folder| {
            let id = folder.id.to_string();
            option(&id, &folder.path)
        }));

        card(vstack([
            hstack([
                vstack([
                    text("Media").color("#1f2937"),
                    text(&media_summary).color(if self.media_scan_errors.is_empty() {
                        "#6b7280"
                    } else {
                        "#b54708"
                    }),
                ])
                .grow(1)
                .spacing(3),
                select(scanned_folder_options)
                    .id(MEDIA_SCANNED_FOLDER_FILTER_ID)
                    .svalue(&self.media_scanned_folder_filter)
                    .width(170)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                size_control,
                select([option("thumbnails", "Thumbnails"), option("table", "Table")])
                    .id(MEDIA_VIEW_MODE_ID)
                    .svalue(&self.media_view_mode)
                    .width(130)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("↻  Refresh view")
                    .id(REFRESH_MEDIA_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .spacing(10)
            .padding_bottom(10),
            media_content,
            pagination,
        ]))
        .grow(1)
        .padding(14)
        .overflow("hidden")
    }

    fn audio_listing_entries(&self) -> Vec<FileListingEntry> {
        self.audio_entries
            .iter()
            .zip(&self.audio_index_entries)
            .map(|(entry, indexed)| FileListingEntry::from_media(entry, indexed))
            .collect()
    }

    fn audio_panel(&self) -> Item {
        let audio_indices = self.filtered_audio_indices();
        let mut audio_summary = format!(
            "{} audio files from {} folders",
            audio_indices.len(),
            self.media_paths.iter().filter(|path| path.enabled).count()
        );
        if self.audio_scanned_folder_filter != "all" {
            audio_summary.push_str("  •  filtered");
        }
        if self.media_scan_truncated {
            audio_summary.push_str("  •  Scan limit reached");
        }
        if !self.media_scan_errors.is_empty() {
            audio_summary.push_str(&format!(
                "  •  {} folders unavailable",
                self.media_scan_errors.len()
            ));
        }

        let listing_entries = self.audio_listing_entries();
        let page_count = audio_indices.len().div_ceil(FILES_PAGE_SIZE);
        let page = self.audio_page.min(page_count.saturating_sub(1));
        let page_start = page.saturating_mul(FILES_PAGE_SIZE);
        let page_end = (page_start + FILES_PAGE_SIZE).min(audio_indices.len());
        let audio_content = if audio_indices.is_empty() {
            vstack([
                text("No audio found").color("#374151"),
                text("Audio from active Scanned folders will appear here after the next scan.")
                    .color("#6b7280"),
            ])
            .grow(1)
            .spacing(4)
            .padding(24)
            .background_color("#f8fafb")
        } else {
            self.file_listing(
                &listing_entries,
                "table",
                0,
                LOCAL_MEDIA_VIEW_ID,
                audio_indices
                    .iter()
                    .copied()
                    .skip(page_start)
                    .take(FILES_PAGE_SIZE),
                Some((self.media_sort_key, self.media_sort_descending)),
            )
        };
        let pagination = if audio_indices.is_empty() {
            hstack(Vec::<Item>::new())
        } else {
            hstack([
                text(&format!(
                    "Showing {}–{} of {}",
                    page_start + 1,
                    page_end,
                    audio_indices.len()
                ))
                .grow(1)
                .color("#6b7280"),
                button("← Previous")
                    .id(AUDIO_PREVIOUS_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page > 0 { "#ffffff" } else { "#f3f4f6" })
                    .color(if page > 0 { "#0f6175" } else { "#9ca3af" }),
                button("Next →")
                    .id(AUDIO_NEXT_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page + 1 < page_count {
                        "#ffffff"
                    } else {
                        "#f3f4f6"
                    })
                    .color(if page + 1 < page_count {
                        "#0f6175"
                    } else {
                        "#9ca3af"
                    }),
            ])
            .spacing(6)
            .padding_top(8)
        };
        let mut scanned_folder_options = vec![option("all", "All scanned folders")];
        scanned_folder_options.extend(self.media_paths.iter().map(|folder| {
            let id = folder.id.to_string();
            option(&id, &folder.path)
        }));

        card(vstack([
            hstack([
                vstack([
                    text("Audio").color("#1f2937"),
                    text(&audio_summary).color(if self.media_scan_errors.is_empty() {
                        "#6b7280"
                    } else {
                        "#b54708"
                    }),
                ])
                .grow(1)
                .spacing(3),
                select(scanned_folder_options)
                    .id(AUDIO_SCANNED_FOLDER_FILTER_ID)
                    .svalue(&self.audio_scanned_folder_filter)
                    .width(170)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("↻  Refresh view")
                    .id(REFRESH_MEDIA_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .spacing(10)
            .padding_bottom(10),
            audio_content,
            pagination,
        ]))
        .grow(1)
        .padding(14)
        .overflow("hidden")
    }

    fn virtual_directories_panel(&self) -> Item {
        if let Some(directory) = self.selected_virtual_directory_id.and_then(|id| {
            self.virtual_directories
                .iter()
                .find(|directory| directory.id == id)
        }) {
            return self.virtual_directory_detail_panel(directory);
        }
        let directories = self.virtual_directories.iter().map(|directory| {
            let entries = self
                .virtual_directory_entries
                .iter()
                .filter(|entry| entry.virtual_directory_id == directory.id)
                .map(|entry| {
                    let name = entry
                        .path
                        .as_ref()
                        .and_then(|path| path.file_name())
                        .map_or_else(
                            || "Unavailable file".to_owned(),
                            |name| name.to_string_lossy().into_owned(),
                        );
                    let location = entry.path.as_ref().map_or_else(
                        || "No current location — the file entry is retained.".to_owned(),
                        |path| path.display().to_string(),
                    );
                    let modified = entry
                        .modified_at
                        .and_then(system_time_from_millis)
                        .map_or_else(|| "—".to_owned(), format_modified);
                    hstack([
                        vstack([
                            text(&name).color("#1f2937"),
                            text(&location).color("#6b7280").break_words(true),
                        ])
                        .grow(1)
                        .spacing(2),
                        vstack([
                            text(&format_size(entry.size)).color("#4b5563"),
                            text(entry.mime_type.as_deref().unwrap_or("Unknown type"))
                                .color("#6b7280"),
                            text(&modified).color("#6b7280"),
                        ])
                        .width(150)
                        .spacing(2),
                    ])
                    .spacing(12)
                    .padding(8)
                    .border("1px solid #e4ebed")
                    .background_color("#ffffff")
                })
                .collect::<Vec<_>>();
            vstack([
                hstack([
                    link(
                        &format!("/virtual-directories/{}", directory.id),
                        &directory.name,
                    )
                    .grow(1)
                    .color("#0f6175")
                    .cursor("pointer"),
                    text(&format!("{} files", entries.len())).color("#6b7280"),
                ])
                .padding_bottom(5),
                if entries.is_empty() {
                    text("No files linked yet.").color("#6b7280").padding(8)
                } else {
                    vstack(entries).spacing(4)
                },
            ])
            .spacing(5)
            .padding(12)
            .border("1px solid #dce5e8")
            .background_color("#f8fafb")
        });
        let content = if self.virtual_directories.is_empty() {
            vstack([
                text("No virtual directories yet.").color("#374151"),
                text("Open an indexed file and choose “Virtual directory” to create one and add the file.")
                    .color("#6b7280"),
            ])
            .spacing(4)
            .padding(24)
            .background_color("#f8fafb")
        } else {
            vstack(directories).spacing(12)
        };

        card(vstack([
            hstack([
                vstack([
                    text("Virtual directories").color("#1f2937"),
                    text("Collections of indexed file entries. Adding or forgetting a scanned folder never changes your files.")
                        .color("#6b7280"),
                ])
                .grow(1)
                .spacing(3),
                button("＋ New virtual directory")
                    .id(SHOW_CREATE_VIRTUAL_DIRECTORY_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(10)
            .padding_bottom(10),
            content,
        ]))
        .grow(1)
        .padding(14)
        .overflow("auto")
    }

    fn virtual_directory_detail_panel(&self, directory: &VirtualDirectory) -> Item {
        let entries = self.virtual_listing_entries(directory.id);
        let entry_count = entries.len();
        let content = if entries.is_empty() {
            vstack([
                text("This virtual directory is empty.").color("#374151"),
                text("Open an indexed file and choose “Virtual directory” to add it here.")
                    .color("#6b7280"),
            ])
            .spacing(4)
            .padding(24)
            .background_color("#f8fafb")
        } else {
            self.file_listing(
                &entries,
                &self.virtual_directory_view_mode,
                self.media_thumbnail_size.clamp(140, 320) as u32,
                VIRTUAL_FILE_VIEW_ID,
                0..entries.len(),
                None,
            )
        };
        card(vstack([
            hstack([
                link("/virtual-directories", "←  Virtual directories")
                    .color("#0f6175")
                    .cursor("pointer"),
                text(&directory.name).grow(1).color("#1f2937"),
                select([option("table", "Table"), option("thumbnails", "Thumbnails")])
                    .id(VIRTUAL_DIRECTORY_VIEW_MODE_ID)
                    .svalue(&self.virtual_directory_view_mode)
                    .width(130)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                text(&format!("{entry_count} files")).color("#6b7280"),
            ])
            .spacing(10)
            .padding_bottom(10),
            content,
        ]))
        .grow(1)
        .padding(14)
        .overflow("auto")
    }

    fn transfers_panel(&self) -> Item {
        card(vstack([
            hstack([vstack([
                text("Upload files").color("#1f2937"),
                text("Select an Inbox, then choose one or more files to upload.").color("#6b7280"),
            ])
            .grow(1)
            .spacing(3)])
            .padding_bottom(12),
            custom_component(
                "file-upload",
                self.upload_asset.url(),
                serde_json::json!({
                    "root": self.configured_this_computer_root.display().to_string(),
                    "inboxes": self.config.inboxes.iter().map(|inbox| serde_json::json!({
                        "id": inbox.id,
                        "name": inbox.name,
                        "folder": inbox.folder.display().to_string(),
                    })).collect::<Vec<_>>(),
                }),
            )
            .custom_event("uploaded", UPLOAD_COMPLETE_ID)
            .width(0)
            .grow(1),
        ]))
        .grow(1)
        .padding(18)
        .overflow("auto")
    }

    fn settings_panel(&self) -> Item {
        let media_folders = self.media_folders_settings();
        let inboxes = self.inboxes_settings();

        card(vstack([
            vstack([
                text("Settings").color("#1f2937"),
                text("Changes are applied immediately and saved for this user.").color("#6b7280"),
            ])
            .spacing(3)
            .padding_bottom(8),
            vstack([inboxes, media_folders]).grow(1).spacing(14),
        ]))
        .grow(1)
        .padding(18)
        .overflow("auto")
    }

    fn inboxes_settings(&self) -> Item {
        let rows = self
            .config
            .inboxes
            .iter()
            .enumerate()
            .map(|(index, inbox)| {
                hstack([
                    vstack([
                        text(&inbox.name),
                        text(&inbox.folder.display().to_string()).color("#6b7280"),
                    ])
                    .grow(1)
                    .spacing(2),
                    button("Remove")
                        .id(REMOVE_INBOX_ID)
                        .inx(index as u32)
                        .padding(6)
                        .border("1px solid #dce5e8")
                        .background_color("#ffffff"),
                ])
                .spacing(8)
                .padding(8)
                .border("1px solid #e4ebed")
                .background_color("#ffffff")
            });
        let mut body = vec![
            text("An Inbox is a server folder that accepts uploads. Sharing and upload permissions will be added separately.")
                .color("#6b7280"),
            button("New Inbox")
                .id(SHOW_NEW_INBOX_ID)
                .padding(7)
                .border("1px solid #0f7892")
                .background_color("#0f7892")
                .color("#ffffff")
                .width(120),
        ];
        body.extend(rows);
        if self.config.inboxes.is_empty() {
            body.push(
                text("No Inboxes are configured. Add an existing server folder to enable uploads.")
                    .color("#6b7280"),
            );
        }
        settings_section("Inboxes", body)
    }

    fn new_inbox_modal(&self) -> Item {
        let mut body = vec![
            hstack([
                vstack([
                    text("New Inbox"),
                    text("Choose an existing server folder that will receive uploads.")
                        .color("#6b7280"),
                ])
                .grow(1)
                .spacing(2),
                button("×")
                    .id(CLOSE_NEW_INBOX_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            text("Inbox name").color("#4b5563"),
            text_input()
                .id(INBOX_NAME_INPUT_ID)
                .svalue(&self.new_inbox_name)
                .placeholder("Family photos"),
            text("Folder relative to the server root").color("#4b5563"),
            hstack([
                text_input()
                    .id(INBOX_FOLDER_INPUT_ID)
                    .svalue(&self.new_inbox_folder)
                    .placeholder("Incoming/Family")
                    .grow(1),
                button("Browse…")
                    .id(BROWSE_INBOX_FOLDER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .spacing(8),
        ];
        if let Some(error) = &self.inbox_error {
            body.push(text(error).color("#b42318"));
        }
        body.push(
            hstack([
                button("Cancel")
                    .id(CLOSE_NEW_INBOX_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("Create Inbox")
                    .id(ADD_INBOX_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        );
        modal([card(vstack(body)).width(520).spacing(10).padding(14)])
    }

    fn new_folder_modal(&self) -> Item {
        let mut body = vec![
            hstack([
                vstack([
                    text("New folder"),
                    text("Create a folder in the active filesystem location.").color("#6b7280"),
                ])
                .grow(1)
                .spacing(2),
                button("×")
                    .id(CLOSE_NEW_FOLDER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            text("Location").color("#4b5563"),
            text(&self.new_folder_parent.display().to_string())
                .color("#6b7280")
                .break_words(true),
            text("Folder name").color("#4b5563"),
            text_input()
                .id(NEW_FOLDER_NAME_INPUT_ID)
                .svalue(&self.new_folder_name)
                .placeholder("New folder"),
        ];
        if let Some(error) = &self.new_folder_error {
            body.push(text(error).color("#b42318"));
        }
        body.push(
            hstack([
                button("Cancel")
                    .id(CLOSE_NEW_FOLDER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("Create folder")
                    .id(CREATE_NEW_FOLDER_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        );
        modal([card(vstack(body)).width(480).spacing(10).padding(14)])
    }

    fn media_folders_settings(&self) -> Item {
        let rows = self.media_paths.iter().map(|path| {
            let status = self.scan_status_text(path);
            let scan_active = self
                .index_status
                .get(&path.id)
                .is_some_and(|status| status.scanning || status.queued);
            let action = if scan_active {
                button(
                    self.index_status
                        .get(&path.id)
                        .is_some_and(|status| status.stopping)
                        .then_some("Stopping…")
                        .unwrap_or("Stop"),
                )
                .id(STOP_MEDIA_SCAN_ID)
                .inx(path.id)
                .width(58)
                .padding(6)
                .border("1px solid #b54708")
                .background_color("#ffffff")
                .color("#9a3412")
            } else {
                button("Scan")
                    .id(START_MEDIA_SCAN_ID)
                    .inx(path.id)
                    .width(58)
                    .padding(6)
                    .border("1px solid #0f7892")
                    .background_color("#ffffff")
                    .color("#0f6175")
            };
            hstack([
                checkbox()
                    .id(TOGGLE_MEDIA_PATH_ID)
                    .inx(path.id)
                    .checked(path.enabled)
                    .width(22),
                vstack([
                    text(&path.path).break_words(true),
                    text(&status).color("#6b7280"),
                ])
                .grow(1)
                .spacing(2),
                action,
                link(&format!("/scanned-folders/{}", path.id), "Info")
                    .width(38)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175")
                    .cursor("pointer"),
                button("Forget")
                    .id(REMOVE_MEDIA_PATH_ID)
                    .inx(path.id)
                    .width(62)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ])
            .spacing(8)
            .padding(8)
            .border("1px solid #e4ebed")
            .background_color("#ffffff")
        });
        let mut body = vec![
            text("PuppyDrive only reads and indexes these folders. Forgetting a folder only removes it from PuppyDrive; it never deletes the folder or its files.")
                .color("#6b7280"),
            text("Skip directories (comma-separated)").color("#4b5563"),
            text_input()
                .id(SCAN_IGNORED_DIRECTORIES_INPUT_ID)
                .svalue(&self.scan_ignored_directories)
                .placeholder("node_modules, .git, target"),
            hstack([
                text("Maximum file size (MB, 0 = unlimited)").grow(1).color("#4b5563"),
                text_input()
                    .id(SCAN_MAX_FILE_SIZE_INPUT_ID)
                    .svalue(&self.scan_max_file_size_mb)
                    .width(110),
            ])
            .spacing(8),
            hstack([
                text("Maximum files per scan (0 = unlimited)")
                    .grow(1)
                    .color("#4b5563"),
                text_input()
                    .id(SCAN_MAX_ITEMS_INPUT_ID)
                    .svalue(&self.scan_max_items)
                    .width(110),
            ])
            .spacing(8),
            hstack([
                text("Maximum directories per scan (0 = unlimited)")
                    .grow(1)
                    .color("#4b5563"),
                text_input()
                    .id(SCAN_MAX_DIRECTORIES_INPUT_ID)
                    .svalue(&self.scan_max_directories)
                    .width(110),
            ])
            .spacing(8),
            hstack([
                text_input()
                    .id(ADD_MEDIA_PATH_INPUT_ID)
                    .svalue(&self.new_media_path)
                    .placeholder("/home/me/Photos")
                    .grow(1),
                button("Browse server…")
                    .id(SHOW_MEDIA_FOLDER_PICKER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
                button("Add scanned folder")
                    .id(ADD_MEDIA_PATH_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        ];
        if let Some(error) = &self.media_path_error {
            body.push(text(error).color("#b42318"));
        }
        body.extend(rows);
        if self.media_paths.is_empty() {
            body.push(text("No Scanned folders are configured.").color("#6b7280"));
        }
        settings_section("Scanned folders", body)
    }

    fn scanned_folder_detail_panel(&self) -> Item {
        let Some(folder) = self
            .selected_scanned_folder_id
            .and_then(|id| self.media_paths.iter().find(|folder| folder.id == id))
        else {
            return card(
                vstack([
                    text("Scanned folder not found").color("#374151"),
                    link("/settings", "Back to Settings")
                        .color("#0f6175")
                        .cursor("pointer"),
                ])
                .spacing(8)
                .padding(18),
            )
            .grow(1)
            .padding(14);
        };
        let history = if self.selected_scanned_folder_history.is_empty() {
            vstack([
                text("No scans recorded yet.").color("#374151"),
                text("Completed scans, including automatic rescans, will appear here.")
                    .color("#6b7280"),
            ])
            .spacing(4)
            .padding(18)
            .background_color("#f8fafb")
        } else {
            let header = hstack([
                text("Started").width(125).color("#6b7280"),
                text("Trigger").width(125).color("#6b7280"),
                text("Result").width(95).color("#6b7280"),
                text("Duration").width(90).color("#6b7280"),
                text("Folders").width(70).color("#6b7280"),
                text("Files").width(70).color("#6b7280"),
                text("Details").grow(1).color("#6b7280"),
            ])
            .spacing(8)
            .padding(8)
            .background_color("#f8fafb");
            let rows = self.selected_scanned_folder_history.iter().map(|entry| {
                let (result, result_color) = scan_outcome_label(entry.outcome);
                let started = system_time_from_millis(entry.started_at)
                    .map_or_else(|| "—".to_owned(), format_modified);
                let detail = entry.error_message.as_deref().unwrap_or("—");
                hstack([
                    text(&started).width(125).color("#374151"),
                    text(scan_trigger_label(entry.trigger))
                        .width(125)
                        .color("#374151"),
                    text(result).width(95).color(result_color),
                    text(&format_scan_duration(entry.finished_at - entry.started_at))
                        .width(90)
                        .color("#374151"),
                    text(&entry.directories_scanned.to_string())
                        .width(70)
                        .color("#374151"),
                    text(&entry.files_indexed.to_string())
                        .width(70)
                        .color("#374151"),
                    text(detail).grow(1).break_words(true).color("#6b7280"),
                ])
                .spacing(8)
                .padding(8)
                .border("1px solid #e4ebed")
                .background_color("#ffffff")
            });
            vstack([header, vstack(rows).spacing(2)]).spacing(2)
        };
        let scan_active = self
            .index_status
            .get(&folder.id)
            .is_some_and(|status| status.scanning || status.queued);
        let scan_action = if scan_active {
            button(
                self.index_status
                    .get(&folder.id)
                    .is_some_and(|status| status.stopping)
                    .then_some("Stopping…")
                    .unwrap_or("Stop"),
            )
            .id(STOP_MEDIA_SCAN_ID)
            .inx(folder.id)
            .padding(7)
            .border("1px solid #b54708")
            .background_color("#ffffff")
            .color("#9a3412")
        } else {
            button("Scan")
                .id(START_MEDIA_SCAN_ID)
                .inx(folder.id)
                .padding(7)
                .border("1px solid #0f7892")
                .background_color("#ffffff")
                .color("#0f6175")
        };
        card(
            vstack([
                hstack([
                    vstack([
                        text("Scanned folder").color("#1f2937"),
                        text(&folder.path).color("#6b7280").break_words(true),
                        text(&self.scan_status_text(folder)).color("#6b7280"),
                    ])
                    .grow(1)
                    .spacing(3),
                    scan_action,
                    link("/settings", "Back to Settings")
                        .padding(7)
                        .border("1px solid #dce5e8")
                        .background_color("#ffffff")
                        .color("#0f6175")
                        .cursor("pointer"),
                ])
                .spacing(10)
                .padding_bottom(14),
                text("Scan history").color("#1f2937").padding_bottom(6),
                history.grow(1).overflow("auto"),
            ])
            .spacing(4),
        )
        .grow(1)
        .height(0)
        .padding(14)
        .overflow("hidden")
    }

    fn files_panel(&self) -> Item {
        let entries = &self.filtered_files;
        let page_count = entries.len().div_ceil(FILES_PAGE_SIZE);
        let page = self.files_page.min(page_count.saturating_sub(1));
        let page_start = page.saturating_mul(FILES_PAGE_SIZE);
        let page_end = (page_start + FILES_PAGE_SIZE).min(entries.len());
        let mut scanned_folder_options = vec![option("all", "All scanned folders")];
        scanned_folder_options.extend(self.media_paths.iter().map(|folder| {
            let id = folder.id.to_string();
            option(&id, &folder.path)
        }));
        let file_content = self.file_listing(
            &entries,
            &self.files_view_mode,
            self.media_thumbnail_size.clamp(140, 320) as u32,
            INDEXED_FILE_VIEW_ID,
            page_start..page_end,
            None,
        );
        let pagination = if entries.is_empty() {
            hstack(Vec::<Item>::new())
        } else {
            hstack([
                text(&format!(
                    "Showing {}–{} of {}",
                    page_start + 1,
                    page_end,
                    entries.len()
                ))
                .grow(1)
                .color("#6b7280"),
                button("← Previous")
                    .id(FILES_PREVIOUS_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page > 0 { "#ffffff" } else { "#f3f4f6" })
                    .color(if page > 0 { "#0f6175" } else { "#9ca3af" }),
                button("Next →")
                    .id(FILES_NEXT_PAGE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if page + 1 < page_count {
                        "#ffffff"
                    } else {
                        "#f3f4f6"
                    })
                    .color(if page + 1 < page_count {
                        "#0f6175"
                    } else {
                        "#9ca3af"
                    }),
            ])
            .spacing(6)
            .padding_top(8)
        };
        card(vstack([
            hstack([
                vstack([
                    text("Files").color("#1f2937"),
                    text(&format!("{} indexed files", entries.len())).color("#6b7280"),
                ])
                .grow(1)
                .spacing(3),
                button("New folder")
                    .id(SHOW_NEW_FOLDER_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
                select(scanned_folder_options)
                    .id(FILES_SCANNED_FOLDER_FILTER_ID)
                    .svalue(&self.files_scanned_folder_filter)
                    .width(170)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                select([
                    option("all", "All types"),
                    option("images", "Images"),
                    option("videos", "Videos"),
                    option("other", "Other"),
                ])
                .id(FILES_MIME_FILTER_ID)
                .svalue(&self.files_mime_filter)
                .width(120)
                .padding(7)
                .border("1px solid #dce5e8")
                .background_color("#ffffff"),
                select([
                    option("modified", "Modified"),
                    option("name", "Name"),
                    option("size", "Size"),
                ])
                .id(FILES_SORT_ID)
                .svalue(&self.files_sort)
                .width(110)
                .padding(7)
                .border("1px solid #dce5e8")
                .background_color("#ffffff"),
                select([option("table", "Table"), option("thumbnails", "Thumbnails")])
                    .id(FILES_VIEW_MODE_ID)
                    .svalue(&self.files_view_mode)
                    .width(130)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ])
            .spacing(8)
            .padding_bottom(10),
            file_content,
            pagination,
        ]))
        .grow(1)
        .padding(14)
        .overflow("hidden")
    }

    #[allow(dead_code)]
    fn local_files_panel(&self) -> Item {
        let at_root = self.this_computer_path == self.this_computer_root;
        let relative_path = self
            .this_computer_path
            .strip_prefix(&self.this_computer_root)
            .unwrap_or(Path::new(""));
        let source_name = self
            .active_source_id
            .and_then(|id| self.sources.iter().find(|source| source.id == id))
            .map_or("This Computer", |source| source.name.as_str());
        let mut location = vec![
            source_name.to_owned(),
            self.this_computer_root.display().to_string(),
        ];
        location.extend(
            relative_path
                .components()
                .map(|component| component.as_os_str().to_string_lossy().into_owned()),
        );

        let toolbar = hstack([
            local_breadcrumbs(&location).grow(1),
            button("↑  Up")
                .id(LOCAL_PARENT_ID)
                .padding(3)
                .border("1px solid #dce5e8")
                .background_color(if at_root { "#f8fafb" } else { "#ffffff" })
                .color(if at_root { "#9ca3af" } else { "#0f6175" }),
        ])
        .spacing(6)
        .padding_bottom(6);
        let columns = hstack([
            text("Name").grow(1),
            text("Type").width(140),
            text("Size").width(100),
            text("Modified").width(150),
        ])
        .padding(4)
        .background_color("#f8fafb");

        let nodes = self.tree_nodes();
        let rows = nodes.iter().enumerate().map(|(index, node)| {
            let expanded = self.expanded_local_dirs.contains(&node.entry.path);
            let selected = self.this_computer_path == node.entry.path;
            local_tree_row(
                node,
                index as u32,
                expanded,
                selected,
                self.folder_row_asset.url(),
            )
        });

        card(vstack([
            toolbar,
            columns,
            vstack(rows).grow(1).overflow("auto"),
        ]))
        .spacing(1)
        .padding(6)
        .overflow("hidden")
    }

    fn file_viewer_modal(&self) -> Option<Item> {
        let (name, content) = if let Some(video) = self.selected_video.as_ref() {
            (&video.name, self.video_file_viewer_content(video))
        } else if let Some(image) = self.selected_image.as_ref() {
            (&image.name, self.image_file_viewer_content(image))
        } else if let Some(file) = self.selected_file.as_ref() {
            (&file.name, self.text_file_viewer_content(file))
        } else {
            return None;
        };
        let navigation_index = self.file_viewer_index;
        let can_go_previous = navigation_index.is_some_and(|index| index > 0);
        let can_go_next = navigation_index
            .is_some_and(|index| index + 1 < self.file_viewer_entries.len(&self.filtered_files));
        let expanded = self.file_viewer_expanded;
        let viewer = card(vstack([
            hstack([
                vstack([text("File viewer"), text(name).color("#6b7280")])
                    .grow(1)
                    .spacing(2),
                if self.selected_file_hash.is_some() {
                    button("＋ Virtual directory")
                        .id(SHOW_VIRTUAL_DIRECTORY_PICKER_ID)
                        .padding(6)
                        .border("1px solid #dce5e8")
                        .background_color("#ffffff")
                        .color("#0f6175")
                } else {
                    vstack(Vec::<Item>::new())
                },
                button("←")
                    .id(PREVIOUS_FILE_VIEWER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if can_go_previous {
                        "#ffffff"
                    } else {
                        "#f3f4f6"
                    })
                    .color(if can_go_previous {
                        "#0f6175"
                    } else {
                        "#9ca3af"
                    }),
                button("→")
                    .id(NEXT_FILE_VIEWER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if can_go_next { "#ffffff" } else { "#f3f4f6" })
                    .color(if can_go_next { "#0f6175" } else { "#9ca3af" }),
                button(if expanded { "↙" } else { "⛶" })
                    .id(TOGGLE_FILE_VIEWER_SIZE_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("×")
                    .id(CLOSE_FILE_VIEWER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ])
            .padding_bottom(8),
            content,
        ]))
        .name(if expanded {
            "file-viewer-modal-expanded"
        } else {
            "file-viewer-modal"
        })
        .fill(expanded)
        .width(if expanded { 0 } else { 900 })
        .max_height(if expanded { 0 } else { 900 })
        .spacing(6)
        .padding(14)
        .overflow("hidden");

        Some(modal([viewer]).padding(if expanded { 1 } else { 32 }))
    }

    fn video_file_viewer_content(&self, video: &VideoFile) -> Item {
        vstack([
            custom_component(
                "video-viewer",
                self.video_viewer_asset.url(),
                serde_json::json!({ "src": video.source_url }),
            )
            .fill(true)
            .grow(1)
            .height(440)
            .background_color("#111827"),
            hstack([
                text("Video").grow(1).color("#4b5563"),
                text(&video.size).color("#4b5563"),
                text(&video.modified).color("#4b5563"),
            ])
            .spacing(12)
            .padding_top(8),
        ])
        .grow(1)
        .overflow("hidden")
    }

    fn image_file_viewer_content(&self, image: &ImageFile) -> Item {
        vstack([
            custom_component(
                "image-viewer",
                self.image_viewer_asset.url(),
                serde_json::json!({
                    "src": image.source_url,
                    "previewSrc": image.preview_url,
                    "alt": image.name,
                }),
            )
            .fill(true)
            .grow(1)
            .height(520)
            .background_color("#111827"),
            hstack([
                text("Mouse wheel to zoom  •  Left-drag to pan")
                    .grow(1)
                    .color("#6b7280"),
                text(&image.size).color("#6b7280"),
                text(&image.modified).color("#6b7280"),
            ])
            .spacing(12)
            .padding_top(8),
        ])
        .grow(1)
        .overflow("hidden")
    }

    fn text_file_viewer_content(&self, file: &FileViewer) -> Item {
        let showing_text = matches!(file.mode, FileViewMode::Text);
        let text_content = file
            .text
            .as_deref()
            .unwrap_or("This file is not valid UTF-8. Switch to Hex to inspect its bytes.");
        let editor = {
            let content = if showing_text {
                text_content.to_owned()
            } else {
                format_hex(&file.bytes, file.truncated)
            };
            vstack([text(&content)
                .white_space("pre-wrap")
                .break_words(true)
                .color("#d1d5db")])
            .height(480)
            .padding(12)
            .background_color("#111827")
            .overflow("auto")
        }
        .grow(1);

        vstack([
            hstack([
                button("Text")
                    .id(TEXT_VIEW_MODE_ID)
                    .padding(5)
                    .border("1px solid #dce5e8")
                    .background_color(if showing_text { "#e5f4f7" } else { "#ffffff" })
                    .color("#0f6175"),
                button("Hex")
                    .id(HEX_VIEW_MODE_ID)
                    .padding(5)
                    .border("1px solid #dce5e8")
                    .background_color(if showing_text { "#ffffff" } else { "#e5f4f7" })
                    .color("#0f6175"),
                text(if file.truncated {
                    "Preview limited to the first 1 MiB"
                } else {
                    ""
                })
                .grow(1)
                .color("#6b7280"),
            ])
            .spacing(6)
            .padding_bottom(6),
            editor,
            hstack([
                text(&file.size).grow(1).color("#6b7280"),
                text("Read-only preview").color("#6b7280"),
            ])
            .padding_top(8),
        ])
        .grow(1)
        .overflow("hidden")
    }

    fn folder_context_modal(&self) -> Option<Item> {
        let context = self.folder_context.as_ref()?;
        let is_expanded = self.expanded_local_dirs.contains(&context.path);
        let path = context.path.display().to_string();
        let included_in_media = self
            .media_paths
            .iter()
            .any(|media_path| Path::new(&media_path.path) == context.path);

        Some(modal([card(vstack([
            hstack([
                text(&context.name).grow(1),
                button("×")
                    .id(CLOSE_FOLDER_CONTEXT_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            text(&path).color("#6b7280").break_words(true),
            button("Open folder")
                .id(OPEN_FOLDER_CONTEXT_ID)
                .padding(8)
                .border("1px solid #dce5e8")
                .background_color("#ffffff")
                .text_align("left"),
            button(if is_expanded {
                "Collapse folder"
            } else {
                "Expand folder"
            })
            .id(TOGGLE_FOLDER_CONTEXT_ID)
            .padding(8)
            .border("1px solid #dce5e8")
            .background_color("#ffffff")
            .text_align("left"),
            if included_in_media {
                text("Included in Scanned folders")
                    .padding(8)
                    .color("#16803a")
            } else {
                button("Add to Scanned folders")
                    .id(INCLUDE_FOLDER_MEDIA_ID)
                    .padding(8)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .text_align("left")
            },
        ]))
        .width(360)
        .spacing(8)
        .padding(14)]))
    }

    fn media_folder_picker_modal(&self) -> Item {
        let folders = self.media_folder_picker_entries();
        let folder_rows = folders.iter().enumerate().map(|(index, (name, _))| {
            button(&format!("▰  {name}"))
                .id(MEDIA_FOLDER_PICKER_NAVIGATE_ID)
                .inx(index as u32)
                .padding(7)
                .border("1px solid #e4ebed")
                .background_color("#ffffff")
                .color("#0f6175")
                .text_align("left")
                .cursor("pointer")
        });
        let path = self.media_folder_picker_path.display().to_string();
        let is_inbox = self.folder_picker_target == FolderPickerTarget::Inbox;
        let can_go_up = self.media_folder_picker_path.parent().is_some()
            && (!is_inbox || self.media_folder_picker_path != self.configured_this_computer_root);
        let (title, description) = match self.folder_picker_target {
            FolderPickerTarget::Media => (
                "Choose scanned folder",
                "This is the PuppyDrive server filesystem.",
            ),
            FolderPickerTarget::Inbox => (
                "Choose Inbox folder",
                "Inbox folders must stay inside the configured server root.",
            ),
            FolderPickerTarget::Source => (
                "Choose source folder",
                "This is the PuppyDrive server filesystem.",
            ),
        };
        let error = match self.folder_picker_target {
            FolderPickerTarget::Media => self.media_path_error.as_deref(),
            FolderPickerTarget::Inbox => self.inbox_error.as_deref(),
            FolderPickerTarget::Source => None,
        };
        modal([card(vstack([
            hstack([
                vstack([text(title), text(description).color("#6b7280")])
                    .grow(1)
                    .spacing(2),
                button("×")
                    .id(CLOSE_MEDIA_FOLDER_PICKER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            hstack([
                button("↑  Up")
                    .id(MEDIA_FOLDER_PICKER_PARENT_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color(if can_go_up { "#ffffff" } else { "#f8fafb" })
                    .color(if can_go_up { "#0f6175" } else { "#9ca3af" }),
                text(&path).grow(1).color("#4b5563").break_words(true),
            ])
            .spacing(8),
            if folders.is_empty() {
                vstack([text("No readable subfolders.").color("#6b7280")])
                    .height(340)
                    .padding(10)
            } else {
                vstack(folder_rows).height(340).overflow("auto")
            },
            if let Some(error) = error {
                text(error).color("#b42318")
            } else {
                vstack(Vec::<Item>::new())
            },
            hstack([
                button("Cancel")
                    .id(CLOSE_MEDIA_FOLDER_PICKER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("New folder")
                    .id(SHOW_PICKER_NEW_FOLDER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
                button("Use this folder")
                    .id(SELECT_MEDIA_FOLDER_PICKER_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        ]))
        .width(620)
        .height(560)
        .spacing(10)
        .padding(14)])
    }

    fn virtual_directory_picker_modal(&self) -> Item {
        let directory_rows = self.virtual_directories.iter().map(|directory| {
            button(&directory.name)
                .id(ADD_TO_VIRTUAL_DIRECTORY_ID)
                .inx(directory.id)
                .padding(8)
                .border("1px solid #e4ebed")
                .background_color("#ffffff")
                .color("#0f6175")
                .text_align("left")
        });
        let mut content = vec![
            hstack([
                vstack([
                    text("Add to virtual directory"),
                    text("This creates a database link to the indexed file; it never moves or copies the file.")
                        .color("#6b7280"),
                ])
                .grow(1)
                .spacing(2),
                button("×")
                    .id(CLOSE_VIRTUAL_DIRECTORY_PICKER_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
        ];
        if self.virtual_directories.is_empty() {
            content.push(text("Create the first virtual directory below.").color("#6b7280"));
        } else {
            content.push(text("Choose an existing directory").color("#4b5563"));
            content.push(
                vstack(directory_rows)
                    .spacing(5)
                    .max_height(220)
                    .overflow("auto"),
            );
        }
        content.push(text("New virtual directory").color("#4b5563"));
        content.push(
            hstack([
                text_input()
                    .id(VIRTUAL_DIRECTORY_NAME_INPUT_ID)
                    .svalue(&self.new_virtual_directory_name)
                    .placeholder("e.g. Favourites")
                    .grow(1),
                button("Create and add")
                    .id(CREATE_VIRTUAL_DIRECTORY_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        );
        if let Some(error) = &self.virtual_directory_error {
            content.push(text(error).color("#b42318"));
        }
        content.push(
            button("Cancel")
                .id(CLOSE_VIRTUAL_DIRECTORY_PICKER_ID)
                .padding(7)
                .border("1px solid #dce5e8")
                .background_color("#ffffff"),
        );
        modal([card(vstack(content)).width(500).spacing(10).padding(14)])
    }

    fn create_virtual_directory_modal(&self) -> Item {
        let mut content = vec![
            hstack([
                vstack([
                    text("New virtual directory"),
                    text("Create an empty collection, then add indexed files from their viewer.")
                        .color("#6b7280"),
                ])
                .grow(1)
                .spacing(2),
                button("×")
                    .id(CLOSE_CREATE_VIRTUAL_DIRECTORY_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            text_input()
                .id(VIRTUAL_DIRECTORY_NAME_INPUT_ID)
                .svalue(&self.new_virtual_directory_name)
                .placeholder("e.g. Favourites"),
        ];
        if let Some(error) = &self.virtual_directory_error {
            content.push(text(error).color("#b42318"));
        }
        content.push(
            hstack([
                button("Cancel")
                    .id(CLOSE_CREATE_VIRTUAL_DIRECTORY_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("Create")
                    .id(SAVE_CREATE_VIRTUAL_DIRECTORY_ID)
                    .padding(7)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        );
        modal([card(vstack(content)).width(440).spacing(10).padding(14)])
    }

    fn add_source_modal(&self) -> Item {
        modal([card(vstack([
            hstack([
                text("Add source").grow(1),
                button("×")
                    .id(CLOSE_ADD_SOURCE_ID)
                    .width(40)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
            ]),
            text("Source name").color("#4b5563"),
            text_input()
                .id(ADD_SOURCE_NAME_INPUT_ID)
                .svalue(&self.new_source_name)
                .placeholder("e.g. Archive drive"),
            text("Folder path").color("#4b5563"),
            hstack([
                text_input()
                    .id(ADD_SOURCE_PATH_INPUT_ID)
                    .svalue(&self.new_source_path)
                    .placeholder("e.g. /Volumes/Archive")
                    .grow(1),
                button("Browse server…")
                    .id(BROWSE_SOURCE_PATH_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .spacing(8),
            text("The local source is validated and saved to this user's database.")
                .color("#6b7280"),
            hstack([
                button("Cancel")
                    .id(CLOSE_ADD_SOURCE_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("Add source")
                    .id(SAVE_ADD_SOURCE_ID)
                    .padding(6)
                    .border("1px solid #0f7892")
                    .background_color("#0f7892")
                    .color("#ffffff"),
            ])
            .spacing(8),
        ]))
        .width(440)
        .spacing(8)
        .padding(14)])
    }
}

#[allow(dead_code)]
fn files_panel() -> Item {
    card(vstack([
        hstack([
            breadcrumbs(["Home NAS", "Media", "Family Videos"]).grow(1),
            text("⋮"),
        ])
        .spacing(6)
        .padding_bottom(6),
        hstack([
            text("Name").width(220),
            text("Type").grow(1),
            text("Size").width(80),
            text("Modified").width(140),
        ])
        .padding(4)
        .background_color("#f8fafb"),
        file_row("▰", "2022", "Folder", "—", "May 14, 2022 10:21 AM", false),
        file_row("▰", "2023", "Folder", "—", "Jun 3, 2023 8:15 PM", false),
        file_row("▰", "2024", "Folder", "—", "Feb 11, 2024 9:42 AM", false),
        file_row(
            "▧",
            "birthday-party-2024.mp4",
            "MP4 Video",
            "1.24 GB",
            "Jun 3, 2024 7:12 PM",
            false,
        ),
        file_row(
            "▧",
            "summer-trip-2024.mp4",
            "MP4 Video",
            "2.08 GB",
            "Jun 10, 2024 5:33 PM",
            false,
        ),
        file_row(
            "▧",
            "holiday-video-2025.mp4",
            "MP4 Video",
            "3.45 GB",
            "Apr 21, 2025 10:18 AM",
            true,
        ),
        file_row(
            "▧",
            "ski-day-2025.mp4",
            "MP4 Video",
            "1.18 GB",
            "Mar 2, 2025 3:47 PM",
            false,
        ),
        file_row(
            "▤",
            "travel-itinerary.pdf",
            "PDF Document",
            "1.2 MB",
            "Apr 20, 2025 9:02 AM",
            false,
        ),
        text("1 item selected                                      3.45 GB")
            .padding_top(8)
            .color("#6b7280"),
    ]))
    .spacing(1)
    .padding(6)
}

#[allow(dead_code)]
fn breadcrumbs<'a>(parts: impl IntoIterator<Item = &'a str>) -> Item {
    let parts = parts.into_iter().collect::<Vec<_>>();
    let mut items = Vec::with_capacity(parts.len().saturating_mul(2));

    for (index, part) in parts.iter().enumerate() {
        items.push(text(part).color("#687385"));
        if index + 1 < parts.len() {
            items.push(text("›").color("#687385"));
        }
    }

    hstack(items).spacing(5)
}

#[allow(dead_code)]
fn local_breadcrumbs(parts: &[String]) -> Item {
    let mut items = Vec::with_capacity(parts.len().saturating_mul(2));

    for (index, part) in parts.iter().enumerate() {
        // The source label and its configured root both return to the root;
        // subsequent crumbs map one-to-one to path components below it.
        let depth = index.saturating_sub(1) as u32;
        items.push(
            button(part)
                .id(LOCAL_BREADCRUMB_ID)
                .inx(depth)
                .padding(0)
                .border("1px solid transparent")
                .background_color("transparent")
                .color("#687385")
                .cursor("pointer"),
        );
        if index + 1 < parts.len() {
            items.push(text("›").color("#687385"));
        }
    }

    hstack(items).spacing(5)
}

#[allow(dead_code)]
fn local_tree_row(
    node: &TreeNode,
    index: u32,
    expanded: bool,
    selected: bool,
    folder_row_entry: &str,
) -> Item {
    let background = if selected { "#e5f4f7" } else { "#ffffff" };
    let entry = &node.entry;
    let is_folder = entry.is_directory && !entry.is_symlink;
    let indent = node.depth.saturating_mul(14).min(u32::MAX as usize) as u32;
    let name = format!(
        "{}  {}",
        if entry.is_directory { "▰" } else { "▤" },
        entry.name
    );
    let name = if is_folder {
        custom_component(
            "folder-row",
            folder_row_entry,
            serde_json::json!({
                "label": name,
                "selected": selected,
                "expanded": expanded,
                "indent": indent,
                "index": index,
            }),
        )
        .id(FOLDER_ROW_COMPONENT_ID)
        .inx(index)
        .custom_event("open", LOCAL_TREE_SELECT_ID)
        .custom_event("toggle", LOCAL_TREE_TOGGLE_ID)
        .custom_event("navigate", LOCAL_TREE_NAVIGATE_ID)
        .custom_event("context", FOLDER_CONTEXT_ID)
        .grow(1)
        .min_width(0)
    } else if is_video_file(entry) {
        button(&name)
            .id(LOCAL_VIDEO_VIEW_ID)
            .inx(index)
            .grow(1)
            .min_width(0)
            .padding(2)
            .border("1px solid transparent")
            .background_color(background)
            .color("#0f6175")
            .text_align("left")
            .cursor("pointer")
    } else if is_image_file(entry) {
        button(&name)
            .id(LOCAL_IMAGE_VIEW_ID)
            .inx(index)
            .grow(1)
            .min_width(0)
            .padding(2)
            .border("1px solid transparent")
            .background_color(background)
            .color("#0f6175")
            .text_align("left")
            .cursor("pointer")
    } else {
        button(&name)
            .id(LOCAL_TEXT_VIEW_ID)
            .inx(index)
            .grow(1)
            .min_width(0)
            .padding(2)
            .border("1px solid transparent")
            .background_color(background)
            .color("#0f6175")
            .text_align("left")
            .cursor("pointer")
    };

    hstack([
        if is_folder {
            name
        } else {
            hstack([
                text("").width(indent).min_width(indent),
                text("").width(22),
                name,
            ])
            .grow(1)
        },
        text(entry.kind).width(140),
        text(&entry.size).width(100),
        text(&entry.modified).width(150),
    ])
    .padding(4)
    .background_color(background)
}

fn is_video_file(entry: &LocalEntry) -> bool {
    !entry.is_directory && !entry.is_symlink && video_content_type(&entry.path).is_some()
}

fn is_image_file(entry: &LocalEntry) -> bool {
    !entry.is_directory && !entry.is_symlink && image_content_type(&entry.path).is_some()
}

fn image_content_type(path: &Path) -> Option<&'static str> {
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
    } else {
        None
    }
}

fn video_content_type(path: &Path) -> Option<&'static str> {
    let extension = path.extension()?.to_str()?;
    if extension.eq_ignore_ascii_case("mp4") {
        Some("video/mp4")
    } else if extension.eq_ignore_ascii_case("webm") {
        Some("video/webm")
    } else if extension.eq_ignore_ascii_case("ogv") {
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

fn default_media_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    let mut paths = Vec::new();
    for path in [home.join("Pictures"), home.join("Videos")] {
        let Ok(path) = fs::canonicalize(path) else {
            continue;
        };
        if path.is_dir() && !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

fn media_folder_picker_start_path(fallback: &Path) -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .and_then(|path| fs::canonicalize(path).ok())
        .filter(|path| path.is_dir())
        .unwrap_or_else(|| fallback.to_path_buf())
}

fn managed_folders(roots: &[MediaScanPath]) -> HashMap<u32, ManagedFolder> {
    roots
        .iter()
        .filter_map(|root| {
            ManagedFolder::open(root.id, &root.path)
                .map(|folder| (root.id, folder))
                .map_err(|error| {
                    log::warn!("Scanned folder {} is unavailable: {error:#}", root.path)
                })
                .ok()
        })
        .collect()
}

fn available_media_entries(
    entries: Vec<IndexedMediaFile>,
    folders: &HashMap<u32, ManagedFolder>,
) -> Vec<IndexedMediaFile> {
    entries
        .into_iter()
        .filter(|entry| folders.contains_key(&entry.scanned_folder_id))
        .collect()
}

fn local_entries_from_index(entries: Vec<IndexedMediaFile>) -> Vec<LocalEntry> {
    entries
        .into_iter()
        .map(|entry| {
            let _mime_type = entry.mime_type;
            let modified_at = entry.modified_at.and_then(system_time_from_millis);
            LocalEntry {
                name: entry.path.file_name().map_or_else(
                    || entry.path.display().to_string(),
                    |name| name.to_string_lossy().into_owned(),
                ),
                path: entry.path,
                is_directory: false,
                is_symlink: false,
                kind: "Media",
                size: format_size(entry.size),
                size_bytes: entry.size,
                modified: modified_at.map_or_else(|| "—".to_owned(), format_modified),
                modified_at,
                media_root_id: Some(entry.scanned_folder_id),
            }
        })
        .collect()
}

#[cfg(test)]
fn system_time_millis(time: SystemTime) -> i64 {
    time.duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis()
        .min(i64::MAX as u128) as i64
}

fn system_time_from_millis(millis: i64) -> Option<SystemTime> {
    u64::try_from(millis)
        .ok()
        .map(|millis| std::time::UNIX_EPOCH + Duration::from_millis(millis))
}

#[cfg(test)]
fn scan_media_entries(
    roots: &[MediaScanPath],
    folders: &HashMap<u32, ManagedFolder>,
    max_items: usize,
    max_directories: usize,
) -> MediaScanResult {
    let mut media = Vec::new();
    let mut queue = VecDeque::new();
    let mut errors = Vec::new();
    let mut observations: HashMap<u32, Vec<MediaIndexObservation>> = HashMap::new();
    let mut truncated = false;
    for root in roots
        .iter()
        .filter(|root| root.enabled && root.indexes_media())
    {
        let Some(folder) = folders.get(&root.id) else {
            errors.push(format!("{} is unavailable", root.path));
            continue;
        };
        queue.push_back((folder.clone(), folder.root().to_path_buf()));
        observations.entry(root.id).or_default();
    }
    let mut visited = HashSet::new();

    while let Some((folder, directory)) = queue.pop_front() {
        if (max_items > 0 && media.len() >= max_items)
            || (max_directories > 0 && visited.len() >= max_directories)
        {
            truncated = true;
            break;
        }
        let Ok(directory) = folder.canonicalize(&directory) else {
            continue;
        };
        if !visited.insert(directory.clone()) {
            continue;
        }
        let Ok(entries) = folder.read_dir(&directory) else {
            errors.push(format!("{} cannot be read", directory.display()));
            continue;
        };
        let mut entries = entries;
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        for entry in entries {
            if max_items > 0 && media.len() >= max_items {
                truncated = true;
                break;
            }
            let name = entry.file_name().to_string_lossy().into_owned();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_symlink() {
                continue;
            }
            let path = entry.path();
            if file_type.is_dir() {
                if !name.starts_with('.')
                    && (max_directories == 0 || visited.len() + queue.len() < max_directories)
                {
                    queue.push_back((folder.clone(), path));
                }
                continue;
            }
            if !file_type.is_file()
                || (image_content_type(&path).is_none() && video_content_type(&path).is_none())
            {
                continue;
            }
            let Ok(path) = folder.canonicalize(path) else {
                continue;
            };
            let Ok(metadata) = folder.metadata(&path) else {
                continue;
            };
            let modified_at = metadata.modified().ok();
            let modified = modified_at.map_or_else(|| "—".to_owned(), format_modified);
            media.push(LocalEntry {
                path: path.clone(),
                name,
                is_directory: false,
                is_symlink: false,
                kind: "Media",
                size: format_size(metadata.len()),
                size_bytes: metadata.len(),
                modified,
                modified_at,
                media_root_id: Some(folder.id()),
            });
            let mime_type = image_content_type(&path)
                .or_else(|| video_content_type(&path))
                .map(str::to_owned);
            observations
                .entry(folder.id())
                .or_default()
                .push(MediaIndexObservation {
                    hash: folder.blake3(&path).ok(),
                    path,
                    size: metadata.len(),
                    mime_type,
                    created_at: metadata.created().ok().map(system_time_millis),
                    modified_at: metadata.modified().ok().map(system_time_millis),
                    accessed_at: metadata.accessed().ok().map(system_time_millis),
                });
        }
    }

    media.sort_by_key(|entry| entry.name.to_lowercase());
    MediaScanResult {
        entries: media,
        _observations: observations,
        truncated,
        errors,
    }
}

fn format_hex(bytes: &[u8], truncated: bool) -> String {
    let shown = &bytes[..bytes.len().min(MAX_HEX_PREVIEW_BYTES)];
    let mut output = String::new();

    for (offset, chunk) in shown.chunks(16).enumerate() {
        let address = offset * 16;
        output.push_str(&format!("{address:08x}  "));
        for index in 0..16 {
            if let Some(byte) = chunk.get(index) {
                output.push_str(&format!("{byte:02x} "));
            } else {
                output.push_str("   ");
            }
            if index == 7 {
                output.push(' ');
            }
        }
        output.push(' ');
        for byte in chunk {
            output.push(if byte.is_ascii_graphic() || *byte == b' ' {
                *byte as char
            } else {
                '.'
            });
        }
        output.push('\n');
    }

    if truncated || bytes.len() > shown.len() {
        output.push_str("\n… preview truncated …");
    }
    output
}

fn custom_event_index(payload: &serde_json::Value) -> Option<usize> {
    payload
        .get("index")
        .and_then(serde_json::Value::as_u64)
        .and_then(|index| usize::try_from(index).ok())
}

fn local_source_url(root: &Path, path: &Path) -> Option<String> {
    let path = fs::canonicalize(path).ok()?;
    let relative_path = path.strip_prefix(root).ok()?;
    let segments = relative_path
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!segments.is_empty()).then(|| {
        format!(
            "/source-files/{}",
            segments
                .into_iter()
                .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
                .collect::<Vec<_>>()
                .join("/")
        )
    })
}

fn media_source_url(root: &MediaScanPath, path: &Path) -> Option<String> {
    let root_path = fs::canonicalize(&root.path).ok()?;
    let path = fs::canonicalize(path).ok()?;
    let relative_path = path.strip_prefix(root_path).ok()?;
    let segments = relative_path
        .components()
        .map(|component| match component {
            Component::Normal(segment) => segment.to_str(),
            _ => None,
        })
        .collect::<Option<Vec<_>>>()?;
    (!segments.is_empty()).then(|| {
        format!(
            "/media-files/{}/{}",
            root.id,
            segments
                .into_iter()
                .map(|segment| utf8_percent_encode(segment, NON_ALPHANUMERIC).to_string())
                .collect::<Vec<_>>()
                .join("/")
        )
    })
}

const THUMBNAIL_MAX_DIMENSION: u32 = 512;

fn media_thumbnail_url(entry: &FileListingEntry) -> Option<String> {
    let folder_id = entry.media_root_id?;
    let hash = entry.hash.as_deref()?;
    Some(format!(
        "/media-thumbnails/{folder_id}/{}",
        hex_encode(hash)
    ))
}

fn media_thumbnail_response(
    request: &str,
    database_path: &Path,
    node_id: &[u8],
    folders: &HashMap<u32, ManagedFolder>,
    cache_dir: &Path,
    cache_only: bool,
) -> Option<HttpResponse> {
    let (folder_id, hash) = parse_thumbnail_request(request)?;
    let database = Database::open(database_path).ok()?;
    let source_path = database
        .media_thumbnail_source(node_id, folder_id, &hash)
        .ok()??;
    let folder = folders.get(&folder_id)?;
    let source_path = folder.canonicalize(&source_path).ok()?;
    let cache_path = cache_dir.join(format!(
        "{}-{THUMBNAIL_MAX_DIMENSION}.png",
        hex_encode(&hash)
    ));
    if let Ok(bytes) = fs::read(&cache_path) {
        return Some(
            HttpResponse::new(200, bytes)
                .header("content-type", "image/png")
                .header("cache-control", "public, max-age=31536000, immutable"),
        );
    }
    if cache_only {
        return Some(HttpResponse::new(404, "thumbnail not cached"));
    }

    let source_bytes = folder.read(&source_path, None).ok()?;
    let image = image::load_from_memory(&source_bytes).ok()?;
    let thumbnail = image.thumbnail(THUMBNAIL_MAX_DIMENSION, THUMBNAIL_MAX_DIMENSION);
    let mut bytes = Vec::new();
    thumbnail
        .write_to(&mut Cursor::new(&mut bytes), image::ImageFormat::Png)
        .ok()?;
    if fs::create_dir_all(cache_dir).is_err() {
        return None;
    }
    let temporary = cache_path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
    if fs::write(&temporary, &bytes).is_ok() {
        if fs::rename(&temporary, &cache_path).is_err() && !cache_path.exists() {
            let _ = fs::remove_file(&temporary);
            return None;
        }
    }
    Some(
        HttpResponse::new(200, bytes)
            .header("content-type", "image/png")
            .header("cache-control", "public, max-age=31536000, immutable"),
    )
}

fn parse_thumbnail_request(request: &str) -> Option<(u32, Vec<u8>)> {
    let (folder_id, hash) = request.split_once('/')?;
    let folder_id = folder_id.parse().ok()?;
    if hash.len() != 64 || hash.contains('/') {
        return None;
    }
    let mut bytes = Vec::with_capacity(32);
    for offset in (0..hash.len()).step_by(2) {
        bytes.push(u8::from_str_radix(&hash[offset..offset + 2], 16).ok()?);
    }
    Some((folder_id, bytes))
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn build_media_watcher(
    roots: &[MediaScanPath],
    debounce: Duration,
    ignored_directory_names: &[String],
) -> Result<(
    RecommendedWatcher,
    tokio::sync::mpsc::Receiver<u32>,
    Vec<PathBuf>,
)> {
    let (raw_tx, raw_rx) = std::sync::mpsc::channel();
    let watched_roots = roots.to_vec();
    let ignored_directory_names = ignored_directory_names.to_vec();
    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) => {
                if let Some(folder_id) =
                    watched_folder_for_event(&event, &watched_roots, &ignored_directory_names)
                {
                    let _ = raw_tx.send(folder_id);
                }
            }
            Err(error) => log::warn!("Media watcher error: {error}"),
        })
        .context("failed creating Media filesystem watcher")?;
    let mut watched = Vec::new();
    for root in roots
        .iter()
        .filter(|root| root.enabled && root.indexes_media())
    {
        let path = PathBuf::from(&root.path);
        if !path.is_dir() {
            continue;
        }
        match watcher.watch(&path, RecursiveMode::Recursive) {
            Ok(()) => watched.push(path),
            Err(error) => log::warn!("failed watching Media folder {}: {error}", path.display()),
        }
    }
    let (change_tx, change_rx) = tokio::sync::mpsc::channel(16);
    std::thread::Builder::new()
        .name("puppydrive-media-debounce".to_owned())
        .spawn(move || {
            while let Ok(folder_id) = raw_rx.recv() {
                let mut changed_folders = HashSet::from([folder_id]);
                while let Ok(folder_id) = raw_rx.recv_timeout(debounce) {
                    changed_folders.insert(folder_id);
                }
                for folder_id in changed_folders {
                    if change_tx.blocking_send(folder_id).is_err() {
                        return;
                    }
                }
            }
        })
        .context("failed starting Media watcher debounce thread")?;
    Ok((watcher, change_rx, watched))
}

fn watched_folder_for_event(
    event: &notify::Event,
    roots: &[MediaScanPath],
    ignored_directory_names: &[String],
) -> Option<u32> {
    if !matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
    ) {
        return None;
    }
    event
        .paths
        .iter()
        .flat_map(|path| {
            roots.iter().filter_map(move |root| {
                let root_path = Path::new(&root.path);
                (root.enabled
                    && root.indexes_media()
                    && path.starts_with(root_path)
                    && !is_ignored_watched_path(path, root_path, ignored_directory_names))
                .then_some((root.id, root_path.components().count()))
            })
        })
        .max_by_key(|(_, depth)| *depth)
        .map(|(id, _)| id)
}

fn is_ignored_watched_path(path: &Path, root: &Path, ignored_directory_names: &[String]) -> bool {
    path.strip_prefix(root)
        .ok()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|component| match component {
            Component::Normal(name) => name.to_str(),
            _ => None,
        })
        .any(|name| {
            name.starts_with('.')
                || ignored_directory_names
                    .iter()
                    .any(|ignored| ignored.eq_ignore_ascii_case(name))
        })
}

fn create_folder_at(root: &Path, parent: &Path, name: &str) -> Result<PathBuf> {
    let root = fs::canonicalize(root).context("active file root is unavailable")?;
    let parent = fs::canonicalize(parent).context("current folder is unavailable")?;
    if !parent.starts_with(&root) || !parent.is_dir() {
        anyhow::bail!("current folder is outside the active file root");
    }
    let name = Path::new(name.trim());
    let mut components = name.components();
    let Some(Component::Normal(name)) = components.next() else {
        anyhow::bail!("folder name must be a single name");
    };
    if components.next().is_some() {
        anyhow::bail!("folder name cannot contain a path");
    }
    let folder = parent.join(name);
    fs::create_dir(&folder)
        .with_context(|| format!("could not create folder {}", folder.display()))?;
    let folder = fs::canonicalize(folder).context("created folder is unavailable")?;
    if !folder.starts_with(&root) {
        anyhow::bail!("created folder is outside the active file root");
    }
    Ok(folder)
}

fn upload_response(
    request: &wgui::HttpRequest,
    root: &Path,
    inboxes: &Arc<RwLock<Vec<InboxConfig>>>,
) -> HttpResponse {
    if request.method != "POST" {
        return HttpResponse::new(405, "uploads require POST").header("allow", "POST");
    }
    if request.body.len() > MAX_UPLOAD_BYTES {
        return HttpResponse::new(413, "uploaded file exceeds the 1 GiB limit");
    }

    let Some(inbox_id) = request.query.get("inbox") else {
        return HttpResponse::new(400, "missing Inbox");
    };
    let Some(filename) = request.headers.get("x-puppydrive-filename") else {
        return HttpResponse::new(400, "missing upload filename");
    };
    let inbox_id = match percent_decode_str(inbox_id).decode_utf8() {
        Ok(id) => id,
        Err(_) => return HttpResponse::new(400, "invalid Inbox"),
    };
    let inbox = inboxes
        .read()
        .ok()
        .and_then(|inboxes| inboxes.iter().find(|inbox| inbox.id == inbox_id).cloned());
    let Some(inbox) = inbox else {
        return HttpResponse::new(404, "Inbox not found");
    };
    let destination = match resolve_upload_folder(root, &inbox.folder) {
        Ok(destination) => destination,
        Err(error) => {
            return HttpResponse::new(409, format!("Inbox folder is unavailable: {error}"));
        }
    };
    let filename = match upload_filename(filename) {
        Ok(filename) => filename,
        Err(error) => return HttpResponse::new(400, format!("invalid upload filename: {error}")),
    };
    let target = destination.join(&filename);

    let mut file = match fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&target)
    {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            return HttpResponse::new(409, "a file with that name already exists");
        }
        Err(error) => {
            log::warn!("unable to create upload '{}': {error}", target.display());
            return HttpResponse::new(500, "unable to create upload");
        }
    };
    if let Err(error) = file.write_all(&request.body).and_then(|_| file.sync_all()) {
        log::warn!("unable to write upload '{}': {error}", target.display());
        drop(file);
        let _ = fs::remove_file(&target);
        return HttpResponse::new(500, "unable to write upload");
    }

    HttpResponse::new(201, format!("uploaded {}", filename.display()))
}

fn inbox_folder_path(folder: &str) -> Result<PathBuf> {
    let folder = folder.trim();
    if folder.is_empty() {
        anyhow::bail!("choose an existing folder");
    }
    let folder = Path::new(folder);
    let mut normalized = PathBuf::new();
    for component in folder.components() {
        let Component::Normal(segment) = component else {
            anyhow::bail!("folder must be relative and cannot traverse directories");
        };
        normalized.push(segment);
    }
    Ok(normalized)
}

fn resolve_upload_folder(root: &Path, folder: &Path) -> Result<PathBuf> {
    if folder.as_os_str().is_empty() {
        anyhow::bail!("folder is not configured");
    }
    let mut path = root.to_path_buf();
    for component in folder.components() {
        let Component::Normal(segment) = component else {
            anyhow::bail!("folder must be relative and cannot traverse directories");
        };
        path.push(segment);
    }
    let path = fs::canonicalize(path).context("folder does not exist or is not accessible")?;
    if !path.starts_with(root) || !path.is_dir() {
        anyhow::bail!("folder is outside the configured server root");
    }
    Ok(path)
}

fn upload_filename(encoded_filename: &str) -> Result<PathBuf> {
    let filename = percent_decode_str(encoded_filename)
        .decode_utf8()
        .context("filename is not valid UTF-8")?;
    let path = Path::new(filename.as_ref());
    let mut components = path.components();
    let Some(Component::Normal(filename)) = components.next() else {
        anyhow::bail!("filename must be a single file name");
    };
    if components.next().is_some() {
        anyhow::bail!("filename must not contain folders");
    }
    Ok(PathBuf::from(filename))
}

fn local_media_response(relative_path: &str, root: &Path) -> Option<HttpResponse> {
    let relative_path = percent_decode_str(relative_path).decode_utf8().ok()?;
    let mut path = root.to_path_buf();
    for component in Path::new(relative_path.as_ref()).components() {
        let Component::Normal(segment) = component else {
            return Some(HttpResponse::new(400, "invalid media path"));
        };
        path.push(segment);
    }

    let Ok(path) = fs::canonicalize(path) else {
        return Some(HttpResponse::new(404, "media not found"));
    };
    if !path.starts_with(root) || !path.is_file() {
        return Some(HttpResponse::new(404, "media not found"));
    }

    let content_type = if let Some(content_type) = video_content_type(&path) {
        content_type
    } else if let Some(content_type) = image_content_type(&path) {
        content_type
    } else {
        return Some(HttpResponse::new(404, "media not found"));
    };

    Some(stream_media_response(&path, content_type, 403))
}

fn managed_media_response(relative_path: &str, folder: &ManagedFolder) -> Option<HttpResponse> {
    let relative_path = percent_decode_str(relative_path).decode_utf8().ok()?;
    let path = folder
        .resolve_relative(Path::new(relative_path.as_ref()))
        .ok()?;
    if !path.is_file() {
        return Some(HttpResponse::new(404, "media not found"));
    }
    let content_type = video_content_type(&path).or_else(|| image_content_type(&path))?;
    Some(stream_media_response(&path, content_type, 404))
}

fn stream_media_response(path: &Path, content_type: &str, error_status: u16) -> HttpResponse {
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(_) => return HttpResponse::new(error_status, "media cannot be read"),
    };
    let stream = futures_util::stream::unfold(file, |mut file| async move {
        let mut bytes = vec![0; 64 * 1024];
        match file.read(&mut bytes) {
            Ok(0) | Err(_) => None,
            Ok(read) => {
                bytes.truncate(read);
                Some((Ok(bytes), file))
            }
        }
    });
    HttpResponse::stream(200, stream)
        .header("content-type", content_type)
        .header("accept-ranges", "bytes")
}

fn format_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

fn replica_label(count: usize) -> String {
    if count == 1 {
        "1 replica".to_owned()
    } else {
        format!("{count} replicas")
    }
}

fn scan_trigger_label(trigger: ScanTrigger) -> &'static str {
    match trigger {
        ScanTrigger::ManualFolder => "Manual folder scan",
        ScanTrigger::ManualRefresh => "Manual refresh",
        ScanTrigger::FilesystemChange => "Filesystem change",
    }
}

fn scan_outcome_label(outcome: ScanOutcome) -> (&'static str, &'static str) {
    match outcome {
        ScanOutcome::Completed => ("Completed", "#16794b"),
        ScanOutcome::Incomplete => ("Incomplete", "#b54708"),
        ScanOutcome::Failed => ("Failed", "#b42318"),
    }
}

fn format_scan_duration(millis: i64) -> String {
    let millis = millis.max(0) as u64;
    if millis < 1_000 {
        format!("{millis} ms")
    } else if millis < 60_000 {
        format!("{:.1} s", millis as f64 / 1_000.0)
    } else {
        format!("{}m {}s", millis / 60_000, (millis / 1_000) % 60)
    }
}

fn format_modified(time: SystemTime) -> String {
    let Ok(elapsed) = SystemTime::now().duration_since(time) else {
        return "Just now".to_owned();
    };
    let seconds = elapsed.as_secs();
    if seconds < 60 {
        "Just now".to_owned()
    } else if seconds < 3_600 {
        relative_age(seconds / 60, "min")
    } else if seconds < 86_400 {
        relative_age(seconds / 3_600, "hr")
    } else {
        let days = seconds / 86_400;
        if days < 30 {
            relative_age(days, "day")
        } else if days < 365 {
            relative_age(days / 30, "month")
        } else {
            relative_age(days / 365, "year")
        }
    }
}

fn relative_age(value: u64, unit: &str) -> String {
    format!("{value} {unit}{} ago", if value == 1 { "" } else { "s" })
}

#[allow(dead_code)]
fn file_row(
    icon: &str,
    name: &str,
    kind: &str,
    size: &str,
    modified: &str,
    selected: bool,
) -> Item {
    let background = if selected { "#e5f4f7" } else { "#ffffff" };
    hstack([
        hstack([text(icon).width(28), text(name)]).width(220),
        text(kind).grow(1),
        text(size).width(80),
        text(modified).width(140),
    ])
    .padding(4)
    .background_color(background)
    .color("#374151")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temporary_directory(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("puppydrive-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn scans_multiple_media_roots_and_respects_item_limit() {
        let first = temporary_directory("media-first");
        let second = temporary_directory("media-second");
        fs::write(first.join("one.jpg"), b"one").unwrap();
        fs::write(second.join("two.mp4"), b"two").unwrap();
        let roots = vec![
            MediaScanPath {
                id: 1,
                path: first.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            },
            MediaScanPath {
                id: 2,
                path: second.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            },
        ];
        let folders = managed_folders(&roots);
        let scan = scan_media_entries(&roots, &folders, 10, 10);
        assert_eq!(scan.entries.len(), 2);
        assert!(
            scan.entries
                .iter()
                .any(|entry| entry.media_root_id == Some(1))
        );
        assert!(
            scan.entries
                .iter()
                .any(|entry| entry.media_root_id == Some(2))
        );
        assert!(!scan.truncated);
        assert!(scan.errors.is_empty());
        let unlimited_scan = scan_media_entries(&roots, &folders, 0, 0);
        assert_eq!(unlimited_scan.entries.len(), 2);
        assert!(!unlimited_scan.truncated);
        assert_eq!(scan_media_entries(&roots, &folders, 1, 10).entries.len(), 1);
        assert!(scan_media_entries(&roots, &folders, 1, 10).truncated);
        let _ = fs::remove_dir_all(first);
        let _ = fs::remove_dir_all(second);
    }

    #[tokio::test]
    async fn media_thumbnails_are_cached_by_file_hash() {
        let directory = temporary_directory("thumbnail-cache");
        let source_path = directory.join("large.png");
        image::RgbaImage::from_pixel(1_200, 800, image::Rgba([20, 120, 180, 255]))
            .save(&source_path)
            .unwrap();
        let database_path = directory.join("puppydrive.db");
        let database = Database::open(&database_path).unwrap();
        let folder = database
            .save_scanned_folder(MediaScanPath {
                id: 0,
                path: directory.to_string_lossy().into_owned(),
                enabled: true,
                indexers: r#"["media"]"#.to_owned(),
            })
            .await
            .unwrap();
        let node_id = database.local_node_id("PuppyDrive").unwrap();
        let hash = vec![9; 32];
        database
            .sync_media_scan(
                &node_id,
                folder.id,
                &[MediaIndexObservation {
                    path: source_path.clone(),
                    hash: Some(hash.clone()),
                    size: fs::metadata(&source_path).unwrap().len(),
                    mime_type: Some("image/png".to_owned()),
                    created_at: None,
                    modified_at: None,
                    accessed_at: None,
                }],
                true,
            )
            .unwrap();
        let managed = ManagedFolder::open(folder.id, &folder.path).unwrap();
        let folders = HashMap::from([(folder.id, managed)]);
        let cache_dir = directory.join("thumbnails");
        let request = format!("{}/{}", folder.id, hex_encode(&hash));
        assert!(
            media_thumbnail_response(
                &request,
                &database_path,
                &node_id,
                &folders,
                &cache_dir,
                false,
            )
            .is_some()
        );
        let cache_path = cache_dir.join(format!(
            "{}-{THUMBNAIL_MAX_DIMENSION}.png",
            hex_encode(&hash)
        ));
        let thumbnail = image::open(&cache_path).unwrap();
        assert!(thumbnail.width() <= THUMBNAIL_MAX_DIMENSION);
        assert!(thumbnail.height() <= THUMBNAIL_MAX_DIMENSION);
        drop(database);
        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn media_response_rejects_traversal() {
        let root = temporary_directory("media-response");
        fs::write(root.join("photo.jpg"), b"photo").unwrap();
        assert_eq!(
            local_media_response("../photo.jpg", &root).unwrap().status,
            400
        );
        assert_eq!(
            local_media_response("photo.jpg", &root).unwrap().status,
            200
        );
        let _ = fs::remove_dir_all(root);
    }

    fn upload_request(inbox: &str, filename: &str, body: &[u8]) -> wgui::HttpRequest {
        wgui::HttpRequest {
            method: "POST".to_owned(),
            path: "/uploads".to_owned(),
            query: HashMap::from([("inbox".to_owned(), inbox.to_owned())]),
            headers: HashMap::from([("x-puppydrive-filename".to_owned(), filename.to_owned())]),
            body: body.to_vec(),
        }
    }

    #[test]
    fn creates_folder_inside_active_root() {
        let root = temporary_directory("create-folder");
        let created = create_folder_at(&root, &root, "Receipts").unwrap();
        assert_eq!(created, root.join("Receipts"));
        assert!(created.is_dir());
        assert!(create_folder_at(&root, &root, "Receipts").is_err());
        assert!(create_folder_at(&root, &root, "../outside").is_err());
        assert!(create_folder_at(&root, &root, "nested/folder").is_err());
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn create_folder_rejects_symlinked_parent_outside_root() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("create-folder-root");
        let outside = temporary_directory("create-folder-outside");
        symlink(&outside, root.join("escape")).unwrap();
        assert!(create_folder_at(&root, &root.join("escape"), "New").is_err());
        assert!(!outside.join("New").exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    fn test_inboxes(folder: impl Into<PathBuf>) -> Arc<RwLock<Vec<InboxConfig>>> {
        Arc::new(RwLock::new(vec![InboxConfig {
            id: "test-inbox".to_owned(),
            name: "Test Inbox".to_owned(),
            folder: folder.into(),
        }]))
    }

    #[test]
    fn upload_writes_selected_file_to_existing_relative_folder() {
        let root = temporary_directory("upload");
        fs::create_dir(root.join("Documents")).unwrap();
        let inboxes = test_inboxes("Documents");
        let request = upload_request("test-inbox", "notes.txt", b"hello");
        assert_eq!(upload_response(&request, &root, &inboxes).status, 201);
        assert_eq!(
            fs::read(root.join("Documents/notes.txt")).unwrap(),
            b"hello"
        );
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn upload_rejects_traversal_and_existing_files() {
        let root = temporary_directory("upload-reject");
        fs::create_dir(root.join("Inbox")).unwrap();
        fs::write(root.join("Inbox/existing.txt"), b"old").unwrap();
        let inboxes = test_inboxes("../");
        assert_eq!(
            upload_response(
                &upload_request("test-inbox", "new.txt", b"new"),
                &root,
                &inboxes
            )
            .status,
            409
        );
        let inboxes = test_inboxes("Inbox");
        assert_eq!(
            upload_response(
                &upload_request("test-inbox", "nested/new.txt", b"new"),
                &root,
                &inboxes,
            )
            .status,
            400
        );
        assert_eq!(
            upload_response(
                &upload_request("test-inbox", "existing.txt", b"new"),
                &root,
                &inboxes,
            )
            .status,
            409
        );
        assert_eq!(fs::read(root.join("Inbox/existing.txt")).unwrap(), b"old");
        let _ = fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn upload_rejects_symlinked_destination_outside_root() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("upload-symlink-root");
        let outside = temporary_directory("upload-symlink-outside");
        symlink(&outside, root.join("escape")).unwrap();
        let inboxes = test_inboxes("escape");
        assert_eq!(
            upload_response(
                &upload_request("test-inbox", "new.txt", b"new"),
                &root,
                &inboxes,
            )
            .status,
            409
        );
        assert!(!outside.join("new.txt").exists());
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn media_response_rejects_symlink_escape() {
        use std::os::unix::fs::symlink;

        let root = temporary_directory("media-symlink-root");
        let outside = temporary_directory("media-symlink-outside");
        fs::write(outside.join("outside.jpg"), b"outside").unwrap();
        symlink(outside.join("outside.jpg"), root.join("escape.jpg")).unwrap();
        assert_eq!(
            local_media_response("escape.jpg", &root).unwrap().status,
            404
        );
        let _ = fs::remove_dir_all(root);
        let _ = fs::remove_dir_all(outside);
    }
}
