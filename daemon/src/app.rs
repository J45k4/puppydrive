#[cfg(test)]
use std::collections::VecDeque;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::{Duration, SystemTime};

use anyhow::{Context, Result};
use notify::{EventKind, RecommendedWatcher, RecursiveMode, Watcher, event::ModifyKind};
use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use wgui::{
    ClientEvent, HttpResponse, Item, StaticAsset, Wgui, button, checkbox, custom_component, hstack,
    link, modal, option, select, slider, text, text_input, vstack,
};

use crate::config::{self, AppConfig, BackupSchedule, Theme};
#[cfg(test)]
use crate::database::MediaIndexObservation;
use crate::database::{
    Database, IndexedMediaFile, LocalSourceConfig, MediaScanPath, Source, local_source_path,
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
const DEVICE_NAME_INPUT_ID: u32 = 46;
const START_ON_LOGIN_ID: u32 = 47;
const NOTIFICATIONS_ID: u32 = 48;
const METERED_BACKUPS_ID: u32 = 49;
const BACKUP_SCHEDULE_ID: u32 = 50;
const THEME_ID: u32 = 51;
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
const MAX_FILE_PREVIEW_BYTES: u64 = 1_048_576;
const MAX_HEX_PREVIEW_BYTES: usize = 65_536;
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
    folder_row_asset: StaticAsset,
    media_tile_asset: StaticAsset,
    mobile_nav_asset: StaticAsset,
    media_entries: Vec<LocalEntry>,
    media_scan_truncated: bool,
    media_scan_errors: Vec<String>,
    media_paths: Vec<MediaScanPath>,
    served_media_paths: Arc<RwLock<Vec<MediaScanPath>>>,
    managed_folders: HashMap<u32, ManagedFolder>,
    served_managed_folders: Arc<RwLock<HashMap<u32, ManagedFolder>>>,
    local_node_id: Vec<u8>,
    indexer: IndexerWorker,
    indexer_events: tokio::sync::mpsc::Receiver<IndexerEvent>,
    index_status: HashMap<u32, FolderIndexStatus>,
    media_watcher: RecommendedWatcher,
    watched_media_paths: Vec<PathBuf>,
    media_change_rx: tokio::sync::mpsc::Receiver<()>,
    new_media_path: String,
    media_path_error: Option<String>,
    show_media_folder_picker: bool,
    media_folder_picker_path: PathBuf,
    media_thumbnail_size: i32,
    media_view_mode: String,
    media_sort_key: MediaSortKey,
    media_sort_descending: bool,
    selected_video: Option<VideoFile>,
    selected_image: Option<ImageFile>,
    selected_file: Option<FileViewer>,
    file_viewer_entries: Vec<LocalEntry>,
    file_viewer_index: Option<usize>,
    file_viewer_expanded: bool,
    folder_context: Option<FolderContext>,
    show_add_source: bool,
    new_source_name: String,
    new_source_path: String,
    sources: Vec<Source>,
    active_source_id: Option<u32>,
    _session_secrets: SessionSecretStore,
    viewing_this_computer: bool,
    active_page: AppPage,
    device_name: String,
    start_on_login: bool,
    notifications_enabled: bool,
    metered_backups: bool,
    backup_schedule: String,
    theme: String,
}

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
    directories_scanned: usize,
    media_files_indexed: usize,
    message: Option<String>,
}

impl LocalEntry {
    fn size_bytes(&self) -> u64 {
        self.size_bytes
    }
}

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
    Transfers,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MediaSortKey {
    Name,
    Type,
    Size,
    Modified,
}

impl App {
    pub fn new() -> Result<Self> {
        let (mut config, paths) = config::load()?;
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
        let mobile_nav_asset = wgui.mount_static_file(
            "/mobile-nav.js",
            concat!(env!("CARGO_MANIFEST_DIR"), "/ui/mobile-nav.js"),
        );
        let active_files_root = Arc::new(RwLock::new(this_computer_root.clone()));
        let served_media_paths = Arc::new(RwLock::new(media_paths.clone()));
        let served_managed_folders = Arc::new(RwLock::new(managed_folders.clone()));
        let handler_files_root = active_files_root.clone();
        let handler_media_paths = served_media_paths.clone();
        let handler_managed_folders = served_managed_folders.clone();
        wgui.set_http_handler(move |request| {
            let files_root = handler_files_root.clone();
            let media_paths = handler_media_paths.clone();
            let managed_folders = handler_managed_folders.clone();
            async move {
                if request.path == "/favicon.ico" {
                    return Some(
                        HttpResponse::new(200, FAVICON_BYTES.to_vec())
                            .header("content-type", "image/png"),
                    );
                }
                if let Some(relative_path) = request.path.strip_prefix("/source-files/") {
                    let root = files_root.read().ok()?.clone();
                    return local_media_response(relative_path, &root);
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
        let cached_media = database.cached_media(&local_node_id)?;
        let (media_watcher, media_change_rx, watched_media_paths) = build_media_watcher(
            &media_paths,
            Duration::from_millis(config.media.watch_debounce_ms.max(1)),
        )?;
        let media_folder_picker_path = media_folder_picker_start_path(&this_computer_root);

        let mut app = Self {
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
            this_computer_root,
            expanded_local_dirs,
            video_viewer_asset,
            image_viewer_asset,
            folder_row_asset,
            media_tile_asset,
            mobile_nav_asset,
            media_entries: local_entries_from_index(cached_media),
            media_scan_truncated: false,
            media_scan_errors: Vec::new(),
            media_paths,
            served_media_paths,
            managed_folders,
            served_managed_folders,
            local_node_id,
            indexer,
            indexer_events,
            index_status: HashMap::new(),
            media_watcher,
            watched_media_paths,
            media_change_rx,
            new_media_path: String::new(),
            media_path_error: None,
            show_media_folder_picker: false,
            media_folder_picker_path,
            media_thumbnail_size: 220,
            media_view_mode: "thumbnails".to_owned(),
            media_sort_key: MediaSortKey::Name,
            media_sort_descending: false,
            selected_video: None,
            selected_image: None,
            selected_file: None,
            file_viewer_entries: Vec::new(),
            file_viewer_index: None,
            file_viewer_expanded: false,
            folder_context: None,
            show_add_source: false,
            new_source_name: String::new(),
            new_source_path: String::new(),
            sources,
            active_source_id: None,
            _session_secrets: SessionSecretStore::default(),
            viewing_this_computer: true,
            active_page: AppPage::Files,
            device_name: String::new(),
            start_on_login: false,
            notifications_enabled: false,
            metered_backups: false,
            backup_schedule: String::new(),
            theme: String::new(),
        }
        .with_configured_settings();
        app.queue_media_index();
        Ok(app)
    }

    fn with_configured_settings(mut self) -> Self {
        self.device_name = self.config.general.device_name.clone();
        self.start_on_login = self.config.general.start_on_login;
        self.notifications_enabled = self.config.general.notifications_enabled;
        self.metered_backups = self.config.backup.metered_connections;
        self.backup_schedule = self.config.backup.schedule.as_str().to_owned();
        self.theme = self.config.appearance.theme.as_str().to_owned();
        self
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
            AppPage::Settings => self.settings_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::Media => self.media_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::Transfers => self.transfers_panel().grow(1).padding(6).overflow("hidden"),
            AppPage::Files => content,
        };

        let page_title = match self.active_page {
            AppPage::Files => "Files",
            AppPage::Media => "Media",
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
        if self.show_media_folder_picker {
            shell.push(self.media_folder_picker_modal());
        }

        hstack(shell).fill(true).overflow("hidden")
    }

    pub async fn run(&mut self) {
        loop {
            let message = tokio::select! {
                message = self.wgui.next() => message,
                changed = self.media_change_rx.recv() => {
                    if changed.is_none() {
                        break;
                    }
                    self.queue_media_index();
                    self.render_all_clients().await;
                    continue;
                }
                event = self.indexer_events.recv() => {
                    if let Some(event) = event {
                        self.handle_indexer_event(event);
                        self.render_all_clients().await;
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
                    self.active_page = match change.path.trim_end_matches('/') {
                        "/media" => AppPage::Media,
                        "/transfers" => AppPage::Transfers,
                        "/settings" => AppPage::Settings,
                        _ => AppPage::Files,
                    };
                    if self.active_page != AppPage::Files {
                        self.close_file_viewer();
                        self.folder_context = None;
                        self.show_add_source = false;
                    }
                }
                ClientEvent::OnTextChanged(change) if change.id == DEVICE_NAME_INPUT_ID => {
                    self.device_name = change.value;
                    self.config.general.device_name = self.device_name.clone();
                    self.save_config();
                }
                ClientEvent::OnSelect(change) if change.id == BACKUP_SCHEDULE_ID => {
                    if let Some(schedule) = BackupSchedule::parse(&change.value) {
                        self.backup_schedule = change.value;
                        self.config.backup.schedule = schedule;
                        self.save_config();
                    }
                }
                ClientEvent::OnSelect(change) if change.id == THEME_ID => {
                    if let Some(theme) = Theme::parse(&change.value) {
                        self.theme = change.value;
                        self.config.appearance.theme = theme;
                        self.save_config();
                    }
                }
                ClientEvent::OnSelect(change) if change.id == MEDIA_VIEW_MODE_ID => {
                    self.media_view_mode = change.value;
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
                ClientEvent::OnKeyDown(key) if key.keycode == "ArrowLeft" => {
                    self.navigate_file_viewer(-1);
                }
                ClientEvent::OnKeyDown(key) if key.keycode == "ArrowRight" => {
                    self.navigate_file_viewer(1);
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
                        self.open_media(index);
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
                    PREVIOUS_FILE_VIEWER_ID => self.navigate_file_viewer(-1),
                    NEXT_FILE_VIEWER_ID => self.navigate_file_viewer(1),
                    TOGGLE_FILE_VIEWER_SIZE_ID => {
                        self.file_viewer_expanded = !self.file_viewer_expanded;
                    }
                    START_ON_LOGIN_ID => {
                        self.start_on_login = !self.start_on_login;
                        self.config.general.start_on_login = self.start_on_login;
                        self.save_config();
                    }
                    NOTIFICATIONS_ID => {
                        self.notifications_enabled = !self.notifications_enabled;
                        self.config.general.notifications_enabled = self.notifications_enabled;
                        self.save_config();
                    }
                    METERED_BACKUPS_ID => {
                        self.metered_backups = !self.metered_backups;
                        self.config.backup.metered_connections = self.metered_backups;
                        self.save_config();
                    }
                    LOCAL_MEDIA_VIEW_ID => {
                        if let Some(index) = click.inx {
                            self.open_media(index as usize);
                        }
                    }
                    MEDIA_SORT_NAME_ID => self.toggle_media_sort(MediaSortKey::Name),
                    MEDIA_SORT_TYPE_ID => self.toggle_media_sort(MediaSortKey::Type),
                    MEDIA_SORT_SIZE_ID => self.toggle_media_sort(MediaSortKey::Size),
                    MEDIA_SORT_MODIFIED_ID => self.toggle_media_sort(MediaSortKey::Modified),
                    REFRESH_MEDIA_ID => {
                        self.queue_media_index();
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
                    SHOW_MEDIA_FOLDER_PICKER_ID => self.open_media_folder_picker(),
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
        self.file_viewer_entries = entries;
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
        let indices = if self.media_view_mode == "table" {
            self.sorted_media_indices()
        } else {
            (0..self.media_entries.len()).collect()
        };
        let Some(viewer_index) = indices.iter().position(|candidate| *candidate == index) else {
            return;
        };
        self.file_viewer_entries = indices
            .into_iter()
            .filter_map(|index| self.media_entries.get(index).cloned())
            .collect();
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
        self.selected_image = Some(ImageFile {
            name: entry.name.clone(),
            size: entry.size.clone(),
            modified: entry.modified.clone(),
            source_url,
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
        if !path.starts_with(&self.this_computer_root) {
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
                .filter(|index| *index < self.file_viewer_entries.len())
        };
        let Some(entry) = next_index
            .and_then(|index| self.file_viewer_entries.get(index))
            .cloned()
        else {
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
        self.file_viewer_entries.clear();
        self.file_viewer_index = None;
        self.file_viewer_expanded = false;
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

    fn queue_media_index(&mut self) {
        self.indexer.request_scan(
            self.media_paths.clone(),
            self.local_node_id.clone(),
            self.config.media.max_items,
            self.config.media.max_directories,
        );
    }

    fn handle_indexer_event(&mut self, event: IndexerEvent) {
        match event {
            IndexerEvent::Started { folder_ids } => {
                for folder_id in folder_ids {
                    self.index_status.insert(
                        folder_id,
                        FolderIndexStatus {
                            scanning: true,
                            ..Default::default()
                        },
                    );
                }
            }
            IndexerEvent::Progress {
                folder_id,
                directories_scanned,
                media_files_indexed,
            } => {
                let status = self.index_status.entry(folder_id).or_default();
                status.scanning = true;
                status.directories_scanned = directories_scanned;
                status.media_files_indexed = media_files_indexed;
            }
            IndexerEvent::FolderFinished { folder_id } => {
                if let Some(status) = self.index_status.get_mut(&folder_id) {
                    status.scanning = false;
                }
            }
            IndexerEvent::Finished { truncated, errors } => {
                self.media_scan_truncated = truncated;
                self.media_scan_errors = errors;
                match self.database.cached_media(&self.local_node_id) {
                    Ok(entries) => self.media_entries = local_entries_from_index(entries),
                    Err(error) => log::error!("failed loading persistent Media index: {error:#}"),
                }
            }
            IndexerEvent::Failed { message } => {
                log::error!("Media indexer: {message}");
                for status in self
                    .index_status
                    .values_mut()
                    .filter(|status| status.scanning)
                {
                    status.scanning = false;
                    status.message = Some(message.clone());
                }
            }
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

    fn open_media_folder_picker(&mut self) {
        self.media_path_error = None;
        if !self.media_folder_picker_path.is_dir() {
            self.media_folder_picker_path =
                media_folder_picker_start_path(&self.configured_this_computer_root);
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
                path.is_dir()
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
            self.media_folder_picker_path = parent;
        }
    }

    async fn select_media_folder_picker(&mut self) {
        let path = self.media_folder_picker_path.clone();
        self.add_media_path(path).await;
        if self.media_path_error.is_none() {
            self.show_media_folder_picker = false;
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
        self.queue_media_index();
    }

    fn reconfigure_media_watcher(&mut self) {
        for path in self.watched_media_paths.drain(..) {
            if let Err(error) = self.media_watcher.unwatch(&path) {
                log::debug!(
                    "failed unwatching Scanned folder {}: {error}",
                    path.display()
                );
            }
        }
        for media_path in self
            .media_paths
            .iter()
            .filter(|path| path.enabled && path.indexes_media())
        {
            let path = PathBuf::from(&media_path.path);
            if !path.is_dir() {
                continue;
            }
            match self.media_watcher.watch(&path, RecursiveMode::Recursive) {
                Ok(()) => self.watched_media_paths.push(path),
                Err(error) => {
                    log::warn!("failed watching Scanned folder {}: {error}", path.display())
                }
            }
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

fn media_sort_button(label: &str, id: u32, active: bool, descending: bool) -> Item {
    let label = if active {
        format!("{label} {}", if descending { "↓" } else { "↑" })
    } else {
        label.to_owned()
    };
    button(&label)
        .id(id)
        .padding(3)
        .border("1px solid transparent")
        .background_color("#f8fafb")
        .color(if active { "#0f6175" } else { "#6b7280" })
        .text_align("left")
        .cursor("pointer")
}

fn settings_row(title: &str, description: &str, control: Item) -> Item {
    hstack([
        vstack([
            text(title).color("#1f2937"),
            text(description).color("#6b7280"),
        ])
        .grow(1)
        .spacing(2),
        control,
    ])
    .spacing(16)
    .padding(10)
    .border("1px solid #e4ebed")
    .background_color("#ffffff")
}

fn settings_section(title: &str, rows: impl IntoIterator<Item = Item>) -> Item {
    vstack([
        text(title).color("#0f6175").padding_bottom(2),
        vstack(rows).spacing(6),
    ])
    .spacing(4)
}

impl App {
    fn media_panel(&self) -> Item {
        let image_count = self
            .media_entries
            .iter()
            .filter(|entry| is_image_file(entry))
            .count();
        let video_count = self.media_entries.len().saturating_sub(image_count);
        let mut media_summary = format!(
            "{image_count} images  •  {video_count} videos from {} folders",
            self.media_paths.iter().filter(|path| path.enabled).count()
        );
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
        let tile_height = thumbnail_size.saturating_mul(3) / 4 + 60;
        let tiles = self
            .media_entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let root_id = entry.media_root_id?;
                let root = self.media_paths.iter().find(|root| root.id == root_id)?;
                let source_url = media_source_url(root, &entry.path)?;
                Some(
                    custom_component(
                        "media-tile",
                        self.media_tile_asset.url(),
                        serde_json::json!({
                            "index": index,
                            "name": entry.name,
                            "kind": if is_image_file(entry) { "image" } else { "video" },
                            "src": source_url,
                            "size": entry.size,
                            "modified": entry.modified,
                            "thumbnailSize": thumbnail_size,
                        }),
                    )
                    .custom_event("open", LOCAL_MEDIA_VIEW_ID)
                    .width(thumbnail_size)
                    .height(tile_height),
                )
            })
            .collect::<Vec<_>>();

        let media_content = if tiles.is_empty() {
            vstack([
                text("No media found").color("#374151"),
                text("Images and videos from active Scanned folders will appear here.")
                    .color("#6b7280"),
            ])
            .grow(1)
            .spacing(4)
            .padding(24)
            .background_color("#f8fafb")
        } else if showing_thumbnails {
            hstack(tiles)
                .wrap(true)
                .spacing(12)
                .grow(1)
                .padding(4)
                .overflow("auto")
        } else {
            self.media_table()
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
                size_control,
                select([option("thumbnails", "Thumbnails"), option("table", "Table")])
                    .id(MEDIA_VIEW_MODE_ID)
                    .svalue(&self.media_view_mode)
                    .width(130)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                button("↻  Refresh")
                    .id(REFRESH_MEDIA_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff")
                    .color("#0f6175"),
            ])
            .spacing(10)
            .padding_bottom(10),
            media_content,
        ]))
        .grow(1)
        .padding(14)
        .overflow("hidden")
    }

    fn media_table(&self) -> Item {
        let rows = self.sorted_media_indices().into_iter().map(|index| {
            let entry = &self.media_entries[index];
            hstack([
                text(if is_image_file(entry) { "▧" } else { "▣" }).width(26),
                button(&entry.name)
                    .id(LOCAL_MEDIA_VIEW_ID)
                    .inx(index as u32)
                    .grow(1)
                    .min_width(0)
                    .padding(3)
                    .border("1px solid transparent")
                    .background_color("#ffffff")
                    .color("#0f6175")
                    .text_align("left"),
                text(media_kind(entry)).width(90),
                text(&entry.size).width(100),
                text(&entry.modified).width(150),
            ])
            .padding(5)
            .border("1px solid #edf1f2")
            .background_color("#ffffff")
        });

        vstack([
            hstack([
                text("").width(26),
                media_sort_button(
                    "Name",
                    MEDIA_SORT_NAME_ID,
                    self.media_sort_key == MediaSortKey::Name,
                    self.media_sort_descending,
                )
                .grow(1),
                media_sort_button(
                    "Type",
                    MEDIA_SORT_TYPE_ID,
                    self.media_sort_key == MediaSortKey::Type,
                    self.media_sort_descending,
                )
                .width(90),
                media_sort_button(
                    "Size",
                    MEDIA_SORT_SIZE_ID,
                    self.media_sort_key == MediaSortKey::Size,
                    self.media_sort_descending,
                )
                .width(100),
                media_sort_button(
                    "Modified",
                    MEDIA_SORT_MODIFIED_ID,
                    self.media_sort_key == MediaSortKey::Modified,
                    self.media_sort_descending,
                )
                .width(150),
            ])
            .padding(2)
            .background_color("#f8fafb")
            .color("#6b7280"),
            vstack(rows).grow(1).overflow("auto"),
        ])
        .grow(1)
        .overflow("hidden")
    }

    fn transfers_panel(&self) -> Item {
        card(vstack([
            hstack([
                vstack([
                    text("Transfers").color("#1f2937"),
                    text("Files currently moving between your devices and storage.")
                        .color("#6b7280"),
                ])
                .grow(1)
                .spacing(3),
                vstack([
                    text("2 active").color("#0f6175"),
                    text("2.4 MB/s total").color("#6b7280"),
                ])
                .spacing(2),
            ])
            .padding_bottom(12),
            vstack([
                transfer_row(
                    "▣",
                    "Laptop / Documents",
                    "Backing up to Home NAS",
                    "1.35 GB / 2.00 GB (67%)",
                ),
                transfer_row(
                    "▤",
                    "Camera SD Card",
                    "Importing new photos",
                    "4.12 GB / 7.91 GB (52%)",
                ),
            ])
            .spacing(8)
            .padding(8)
            .border("1px solid #e4ebed")
            .background_color("#ffffff"),
            vstack([
                text("Transfer activity").color("#374151"),
                text("Completed transfers will appear here after the active queue finishes.")
                    .color("#6b7280"),
            ])
            .spacing(3)
            .padding(12)
            .margin_top(12)
            .background_color("#f8fafb"),
        ]))
        .grow(1)
        .padding(18)
        .overflow("auto")
    }

    fn settings_panel(&self) -> Item {
        let general = settings_section(
            "General",
            [
                settings_row(
                    "Device name",
                    "The name shown to other PuppyDrive devices.",
                    text_input()
                        .id(DEVICE_NAME_INPUT_ID)
                        .svalue(&self.device_name)
                        .width(240)
                        .padding(6)
                        .border("1px solid #dce5e8")
                        .background_color("#ffffff"),
                ),
                settings_row(
                    "Launch at sign in",
                    "Start PuppyDrive automatically when you sign in.",
                    checkbox()
                        .id(START_ON_LOGIN_ID)
                        .checked(self.start_on_login)
                        .width(22),
                ),
                settings_row(
                    "Desktop notifications",
                    "Show backup, transfer, and connection notifications.",
                    checkbox()
                        .id(NOTIFICATIONS_ID)
                        .checked(self.notifications_enabled)
                        .width(22),
                ),
            ],
        );

        let backup = settings_section(
            "Backup and sync",
            [
                settings_row(
                    "Backup schedule",
                    "Choose how often changed files are backed up.",
                    select([
                        option("continuous", "Continuous"),
                        option("hourly", "Hourly"),
                        option("daily", "Daily"),
                    ])
                    .id(BACKUP_SCHEDULE_ID)
                    .svalue(&self.backup_schedule)
                    .width(180)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
                ),
                settings_row(
                    "Metered connections",
                    "Allow backups when the current network is metered.",
                    checkbox()
                        .id(METERED_BACKUPS_ID)
                        .checked(self.metered_backups)
                        .width(22),
                ),
                settings_row(
                    "Default destination",
                    "New backup policies use this destination.",
                    text("Home NAS").color("#0f6175").padding(6),
                ),
            ],
        );

        let appearance = settings_section(
            "Appearance",
            [settings_row(
                "Theme",
                "Choose the preferred interface appearance.",
                select([
                    option("system", "System"),
                    option("light", "Light"),
                    option("dark", "Dark"),
                ])
                .id(THEME_ID)
                .svalue(&self.theme)
                .width(180)
                .padding(6)
                .border("1px solid #dce5e8")
                .background_color("#ffffff"),
            )],
        );
        let media_folders = self.media_folders_settings();

        let about = settings_section(
            "About",
            [
                settings_row(
                    "PuppyDrive",
                    "Private file sync and backup across your devices.",
                    text(&format!("v{BUILD_VERSION}"))
                        .color("#6b7280")
                        .padding(6),
                ),
                settings_row(
                    "Remote credentials",
                    "Credentials are session-only in v1. Entering them over LAN HTTP does not encrypt them in transit.",
                    text("Not stored").color("#b54708").padding(6),
                ),
            ],
        );

        card(vstack([
            vstack([
                text("Settings").color("#1f2937"),
                text("Changes are applied immediately and saved for this user.").color("#6b7280"),
            ])
            .spacing(3)
            .padding_bottom(8),
            hstack([
                vstack([general, media_folders, appearance])
                    .grow(1)
                    .spacing(14),
                vstack([backup, about]).grow(1).spacing(14),
            ])
            .grow(1)
            .spacing(14),
        ]))
        .grow(1)
        .padding(18)
        .overflow("auto")
    }

    fn media_folders_settings(&self) -> Item {
        let rows = self.media_paths.iter().map(|path| {
            let status = if !Path::new(&path.path).is_dir() {
                "Folder is unavailable".to_owned()
            } else if !path.enabled {
                "Media indexing paused".to_owned()
            } else if let Some(status) = self.index_status.get(&path.id) {
                if status.scanning {
                    format!(
                        "Scanning • {} folders • {} media files",
                        status.directories_scanned, status.media_files_indexed
                    )
                } else if let Some(message) = &status.message {
                    format!("Indexing error • {message}")
                } else {
                    "Media indexing active".to_owned()
                }
            } else {
                "Media indexing active".to_owned()
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
                button("Forget")
                    .id(REMOVE_MEDIA_PATH_ID)
                    .inx(path.id)
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

    fn files_panel(&self) -> Item {
        if self.viewing_this_computer {
            return self.local_files_panel();
        }

        files_panel()
    }

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
        let can_go_next =
            navigation_index.is_some_and(|index| index + 1 < self.file_viewer_entries.len());
        let expanded = self.file_viewer_expanded;
        let viewer = card(vstack([
            hstack([
                vstack([text("File viewer"), text(name).color("#6b7280")])
                    .grow(1)
                    .spacing(2),
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
                button(if expanded {
                    "↙  Restore"
                } else {
                    "⛶  Maximize"
                })
                .id(TOGGLE_FILE_VIEWER_SIZE_ID)
                .width(112)
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
        let hex_content = format_hex(&file.bytes, file.truncated);
        let editor = {
            let content = if showing_text {
                text_content.to_owned()
            } else {
                hex_content
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
        let can_go_up = self.media_folder_picker_path.parent().is_some();
        modal([card(vstack([
            hstack([
                vstack([
                    text("Choose server folder"),
                    text("This is the PuppyDrive server filesystem.").color("#6b7280"),
                ])
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
                    .grow(1)
                    .padding(10)
            } else {
                vstack(folder_rows).grow(1).overflow("auto")
            },
            hstack([
                button("Cancel")
                    .id(CLOSE_MEDIA_FOLDER_PICKER_ID)
                    .padding(7)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
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
            text_input()
                .id(ADD_SOURCE_PATH_INPUT_ID)
                .svalue(&self.new_source_path)
                .placeholder("e.g. /Volumes/Archive"),
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
        if media.len() >= max_items || visited.len() >= max_directories {
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
            if media.len() >= max_items {
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
                if !name.starts_with('.') && visited.len() + queue.len() < max_directories {
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

fn build_media_watcher(
    roots: &[MediaScanPath],
    debounce: Duration,
) -> Result<(
    RecommendedWatcher,
    tokio::sync::mpsc::Receiver<()>,
    Vec<PathBuf>,
)> {
    let (raw_tx, raw_rx) = std::sync::mpsc::channel();
    let mut watcher =
        notify::recommended_watcher(move |event: notify::Result<notify::Event>| match event {
            Ok(event) if should_reindex_for_event(&event) => {
                let _ = raw_tx.send(());
            }
            Ok(_) => {}
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
    let (change_tx, change_rx) = tokio::sync::mpsc::channel(1);
    std::thread::Builder::new()
        .name("puppydrive-media-debounce".to_owned())
        .spawn(move || {
            while raw_rx.recv().is_ok() {
                while raw_rx.recv_timeout(debounce).is_ok() {}
                if change_tx.blocking_send(()).is_err() {
                    break;
                }
            }
        })
        .context("failed starting Media watcher debounce thread")?;
    Ok((watcher, change_rx, watched))
}

fn should_reindex_for_event(event: &notify::Event) -> bool {
    matches!(
        event.kind,
        EventKind::Create(_)
            | EventKind::Remove(_)
            | EventKind::Modify(ModifyKind::Data(_))
            | EventKind::Modify(ModifyKind::Name(_))
    )
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

    Some(match fs::read(path) {
        Ok(bytes) => HttpResponse::new(200, bytes)
            .header("content-type", content_type)
            .header("accept-ranges", "bytes"),
        Err(_) => HttpResponse::new(403, "media cannot be read"),
    })
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
    Some(match folder.read(&path, None) {
        Ok(bytes) => HttpResponse::new(200, bytes)
            .header("content-type", content_type)
            .header("accept-ranges", "bytes"),
        Err(_) => HttpResponse::new(404, "media not found"),
    })
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

fn transfer_row(icon: &str, title: &str, subtitle: &str, progress: &str) -> Item {
    hstack([
        text(icon).padding(6).color("#0f6175"),
        vstack([text(title), text(subtitle).color("#6b7280")])
            .spacing(2)
            .width(220),
        text("━━━━━━━━").color("#0f7892").grow(1),
        vstack([
            text(progress),
            text("28 MB/s  •  00:23 remaining").color("#6b7280"),
        ])
        .spacing(2)
        .width(210),
        button("Ⅱ").width(36),
        button("×").width(36),
    ])
    .spacing(4)
    .padding(4)
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
        assert_eq!(scan_media_entries(&roots, &folders, 1, 10).entries.len(), 1);
        assert!(scan_media_entries(&roots, &folders, 1, 10).truncated);
        let _ = fs::remove_dir_all(first);
        let _ = fs::remove_dir_all(second);
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
