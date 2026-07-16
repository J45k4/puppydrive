use std::collections::{HashSet, VecDeque};
use std::env;
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use wgui::{
    ClientEvent, HttpResponse, Item, StaticAsset, Wgui, button, checkbox, custom_component, hstack,
    link, modal, option, select, slider, text, text_input, textarea, vstack,
};

const THIS_COMPUTER_SOURCE_ID: u32 = 20;
const LOCAL_PARENT_ID: u32 = 21;
const LOCAL_BREADCRUMB_ID: u32 = 23;
const LOCAL_TREE_TOGGLE_ID: u32 = 24;
const LOCAL_TREE_SELECT_ID: u32 = 25;
const LOCAL_VIDEO_VIEW_ID: u32 = 26;
const CLOSE_FILE_VIEWER_ID: u32 = 27;
const LOCAL_TEXT_VIEW_ID: u32 = 28;
const TOGGLE_FILE_VIEWER_SIZE_ID: u32 = 29;
const SAVE_TEXT_VIEWER_ID: u32 = 30;
const TEXT_VIEW_MODE_ID: u32 = 31;
const HEX_VIEW_MODE_ID: u32 = 32;
const TEXT_EDITOR_ID: u32 = 33;
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
const MAX_FILE_PREVIEW_BYTES: u64 = 1_048_576;
const MAX_HEX_PREVIEW_BYTES: usize = 65_536;
const MAX_MEDIA_ITEMS: usize = 1_000;
const MAX_MEDIA_DIRECTORIES: usize = 512;
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
    client_ids: HashSet<usize>,
    this_computer_root: PathBuf,
    this_computer_path: PathBuf,
    tree_view_root: PathBuf,
    expanded_local_dirs: HashSet<PathBuf>,
    video_viewer_asset: StaticAsset,
    image_viewer_asset: StaticAsset,
    folder_row_asset: StaticAsset,
    media_tile_asset: StaticAsset,
    mobile_nav_asset: StaticAsset,
    media_entries: Vec<LocalEntry>,
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
    additional_sources: Vec<AddedSource>,
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
    path: PathBuf,
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

struct AddedSource {
    name: String,
    path: String,
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
    pub fn new() -> Self {
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:5777".into());
        let bind_addr: SocketAddr = bind_addr.parse().unwrap_or_else(|error| {
            panic!("invalid BIND_ADDR '{bind_addr}': {error}");
        });

        // This source intentionally starts at the computer's filesystem root. An
        // administrator can limit it to a particular directory with
        // THIS_COMPUTER_ROOT, and navigation is kept inside that root.
        let this_computer_root = env::var_os("THIS_COMPUTER_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("/"));
        let this_computer_root = fs::canonicalize(&this_computer_root).unwrap_or_else(|error| {
            panic!(
                "unable to access THIS_COMPUTER_ROOT '{}': {error}",
                this_computer_root.display()
            );
        });

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
        let media_root = this_computer_root.clone();
        wgui.set_http_handler(move |request| {
            let media_root = media_root.clone();
            async move {
                if request.path == "/favicon.ico" {
                    return Some(
                        HttpResponse::new(200, FAVICON_BYTES.to_vec())
                            .header("content-type", "image/png"),
                    );
                }
                request
                    .path
                    .strip_prefix("/source-files/")
                    .and_then(|relative_path| local_media_response(relative_path, &media_root))
            }
        });

        let mut expanded_local_dirs = HashSet::new();
        expanded_local_dirs.insert(this_computer_root.clone());
        let media_entries = scan_media_entries(&this_computer_root);

        Self {
            wgui,
            client_ids: HashSet::new(),
            this_computer_path: this_computer_root.clone(),
            tree_view_root: this_computer_root.clone(),
            this_computer_root,
            expanded_local_dirs,
            video_viewer_asset,
            image_viewer_asset,
            folder_row_asset,
            media_tile_asset,
            mobile_nav_asset,
            media_entries,
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
            additional_sources: Vec::new(),
            viewing_this_computer: false,
            active_page: AppPage::Files,
            device_name: "PuppyDrive".to_owned(),
            start_on_login: true,
            notifications_enabled: true,
            metered_backups: false,
            backup_schedule: "continuous".to_owned(),
            theme: "system".to_owned(),
        }
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
                self.viewing_this_computer && self.active_page == AppPage::Files,
            ),
            source_nav_item(
                "▦",
                "Home NAS",
                !self.viewing_this_computer && self.active_page == AppPage::Files,
                true,
            ),
            source_nav_item("☁", "PuppyCloud", false, true),
            source_nav_item("▰", "Laptop", false, true),
            source_nav_item("▤", "Camera SD Card", false, false),
        ];
        sidebar_items.extend(self.additional_sources.iter().map(|source| {
            let label = if source.path.is_empty() {
                source.name.clone()
            } else {
                format!("{}  •", source.name)
            };
            source_nav_item("□", &label, false, true)
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

        hstack(shell).fill(true).overflow("hidden")
    }

    pub async fn run(&mut self) {
        while let Some(message) = self.wgui.next().await {
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
                    if self.active_page == AppPage::Media {
                        self.media_entries = scan_media_entries(&self.this_computer_root);
                    }
                    if self.active_page != AppPage::Files {
                        self.close_file_viewer();
                        self.folder_context = None;
                        self.show_add_source = false;
                    }
                }
                ClientEvent::OnTextChanged(change) if change.id == DEVICE_NAME_INPUT_ID => {
                    self.device_name = change.value;
                }
                ClientEvent::OnSelect(change) if change.id == BACKUP_SCHEDULE_ID => {
                    self.backup_schedule = change.value;
                }
                ClientEvent::OnSelect(change) if change.id == THEME_ID => {
                    self.theme = change.value;
                }
                ClientEvent::OnSelect(change) if change.id == MEDIA_VIEW_MODE_ID => {
                    self.media_view_mode = change.value;
                }
                ClientEvent::OnSliderChange(change) if change.id == MEDIA_THUMBNAIL_SIZE_ID => {
                    self.media_thumbnail_size = change.value.clamp(140, 320);
                }
                ClientEvent::OnTextChanged(change) if change.id == TEXT_EDITOR_ID => {
                    if let Some(viewer) = &mut self.selected_file {
                        viewer.text = Some(change.value);
                    }
                }
                ClientEvent::OnTextChanged(change) if change.id == ADD_SOURCE_NAME_INPUT_ID => {
                    self.new_source_name = change.value;
                }
                ClientEvent::OnTextChanged(change) if change.id == ADD_SOURCE_PATH_INPUT_ID => {
                    self.new_source_path = change.value;
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
                        self.active_page = AppPage::Files;
                        self.viewing_this_computer = true;
                        self.this_computer_path = self.this_computer_root.clone();
                        self.tree_view_root = self.this_computer_root.clone();
                        self.wgui.handle().push_state(client_id, "/").await;
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
                    START_ON_LOGIN_ID => self.start_on_login = !self.start_on_login,
                    NOTIFICATIONS_ID => {
                        self.notifications_enabled = !self.notifications_enabled;
                    }
                    METERED_BACKUPS_ID => self.metered_backups = !self.metered_backups,
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
                        self.media_entries = scan_media_entries(&self.this_computer_root);
                    }
                    TEXT_VIEW_MODE_ID => self.set_file_view_mode(FileViewMode::Text),
                    HEX_VIEW_MODE_ID => self.set_file_view_mode(FileViewMode::Hex),
                    SAVE_TEXT_VIEWER_ID => self.save_text_file(),
                    CLOSE_FOLDER_CONTEXT_ID => self.folder_context = None,
                    OPEN_FOLDER_CONTEXT_ID => self.open_context_folder(),
                    TOGGLE_FOLDER_CONTEXT_ID => self.toggle_context_folder(),
                    SHOW_ADD_SOURCE_ID => self.show_add_source = true,
                    CLOSE_ADD_SOURCE_ID => self.show_add_source = false,
                    SAVE_ADD_SOURCE_ID => self.save_added_source(),
                    _ => {}
                },
                event => {
                    log::debug!("wgui client {client_id} event: {event:?}");
                }
            }

            for client_id in &self.client_ids {
                self.wgui.render(*client_id, self.render()).await;
            }
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
        let Ok(path) = fs::canonicalize(&entry.path) else {
            return false;
        };
        let Some(source_url) = media_source_url(&self.this_computer_root, &path) else {
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
        let Ok(path) = fs::canonicalize(&entry.path) else {
            return false;
        };
        let Some(source_url) = media_source_url(&self.this_computer_root, &path) else {
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

    fn select_file(&mut self, entry: &LocalEntry) -> bool {
        if entry.is_directory || entry.is_symlink {
            return false;
        }

        let Ok(path) = fs::canonicalize(&entry.path) else {
            return false;
        };
        if !path.starts_with(&self.this_computer_root) {
            return false;
        }
        let Ok(file) = fs::File::open(&path) else {
            return false;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_FILE_PREVIEW_BYTES)
            .read_to_end(&mut bytes)
            .is_err()
        {
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
            path,
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

    fn save_text_file(&mut self) {
        let Some(viewer) = &mut self.selected_file else {
            return;
        };
        let Some(text) = viewer.text.as_ref() else {
            return;
        };
        if viewer.truncated {
            return;
        }
        if fs::write(&viewer.path, text).is_ok() {
            viewer.bytes = text.as_bytes().to_vec();
        }
    }

    fn save_added_source(&mut self) {
        let name = self.new_source_name.trim();
        if name.is_empty() {
            return;
        }
        self.additional_sources.push(AddedSource {
            name: name.to_owned(),
            path: self.new_source_path.trim().to_owned(),
        });
        self.new_source_name.clear();
        self.new_source_path.clear();
        self.show_add_source = false;
    }
}

fn card(body: Item) -> Item {
    body.border("1px solid #dfe7e9").background_color("#ffffff")
}

fn source_nav_item(icon: &str, name: &str, active: bool, online: bool) -> Item {
    let background = if active { "#e5f4f7" } else { "#f8fbfc" };
    let color = if active { "#0f6175" } else { "#374151" };
    let status_color = if online { "#16803a" } else { "#9ca3af" };

    hstack([
        text(icon).color(color),
        text(name).grow(1).color(color),
        text("●").color(status_color),
    ])
    .width(180)
    .spacing(5)
    .padding(5)
    .background_color(background)
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
        let showing_thumbnails = self.media_view_mode != "table";
        let thumbnail_size = self.media_thumbnail_size.clamp(140, 320) as u32;
        let tile_height = thumbnail_size.saturating_mul(3) / 4 + 60;
        let tiles = self
            .media_entries
            .iter()
            .enumerate()
            .filter_map(|(index, entry)| {
                let source_url = media_source_url(&self.this_computer_root, &entry.path)?;
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
                text("Images and videos under This Computer will appear here.").color("#6b7280"),
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
                    text(&format!(
                        "{image_count} images  •  {video_count} videos from This Computer"
                    ))
                    .color("#6b7280"),
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

        let about = settings_section(
            "About",
            [settings_row(
                "PuppyDrive",
                "Private file sync and backup across your devices.",
                text(&format!("v{BUILD_VERSION}"))
                    .color("#6b7280")
                    .padding(6),
            )],
        );

        card(vstack([
            vstack([
                text("Settings").color("#1f2937"),
                text("Changes are applied immediately for this session.").color("#6b7280"),
            ])
            .spacing(3)
            .padding_bottom(8),
            hstack([
                vstack([general, appearance]).grow(1).spacing(14),
                vstack([backup, about]).grow(1).spacing(14),
            ])
            .grow(1)
            .spacing(14),
        ]))
        .grow(1)
        .padding(18)
        .overflow("auto")
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
        let mut location = vec![
            "This Computer".to_owned(),
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
        let editor = if showing_text && file.text.is_some() {
            textarea()
                .id(TEXT_EDITOR_ID)
                .svalue(text_content)
                .height(480)
                .padding(12)
                .background_color("#111827")
                .color("#d1d5db")
        } else {
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
                button("Save")
                    .id(SAVE_TEXT_VIEWER_ID)
                    .padding(6)
                    .border("1px solid #dce5e8")
                    .background_color("#ffffff"),
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
        ]))
        .width(360)
        .spacing(8)
        .padding(14)]))
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
            text("The source is added to this session's sidebar.").color("#6b7280"),
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

fn media_scan_roots(root: &Path) -> Vec<PathBuf> {
    if root != Path::new("/") {
        return vec![root.to_path_buf()];
    }

    let Some(home) = env::var_os("HOME").map(PathBuf::from) else {
        return Vec::new();
    };
    [home.join("Pictures"), home.join("Videos")]
        .into_iter()
        .filter_map(|path| fs::canonicalize(path).ok())
        .filter(|path| path.starts_with(root) && path.is_dir())
        .collect()
}

fn scan_media_entries(root: &Path) -> Vec<LocalEntry> {
    let mut media = Vec::new();
    let mut queue = VecDeque::from(media_scan_roots(root));
    let mut visited = HashSet::new();

    while let Some(directory) = queue.pop_front() {
        if media.len() >= MAX_MEDIA_ITEMS || visited.len() >= MAX_MEDIA_DIRECTORIES {
            break;
        }
        let Ok(directory) = fs::canonicalize(directory) else {
            continue;
        };
        if !directory.starts_with(root) || !visited.insert(directory.clone()) {
            continue;
        }
        let Ok(entries) = fs::read_dir(directory) else {
            continue;
        };
        let mut entries = entries.filter_map(Result::ok).collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_lowercase());

        for entry in entries {
            if media.len() >= MAX_MEDIA_ITEMS {
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
                if !name.starts_with('.') && visited.len() + queue.len() < MAX_MEDIA_DIRECTORIES {
                    queue.push_back(path);
                }
                continue;
            }
            if !file_type.is_file()
                || (image_content_type(&path).is_none() && video_content_type(&path).is_none())
            {
                continue;
            }
            let Ok(path) = fs::canonicalize(path) else {
                continue;
            };
            if !path.starts_with(root) {
                continue;
            }
            let Ok(metadata) = fs::metadata(&path) else {
                continue;
            };
            let modified_at = metadata.modified().ok();
            let modified = modified_at.map_or_else(|| "—".to_owned(), format_modified);
            media.push(LocalEntry {
                path,
                name,
                is_directory: false,
                is_symlink: false,
                kind: "Media",
                size: format_size(metadata.len()),
                size_bytes: metadata.len(),
                modified,
                modified_at,
            });
        }
    }

    media.sort_by_key(|entry| entry.name.to_lowercase());
    media
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

fn media_source_url(root: &Path, path: &Path) -> Option<String> {
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
