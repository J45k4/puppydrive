use std::collections::HashSet;
use std::env;
use std::fs;
use std::io::Read;
use std::net::SocketAddr;
use std::path::{Component, Path, PathBuf};
use std::time::SystemTime;

use percent_encoding::{NON_ALPHANUMERIC, percent_decode_str, utf8_percent_encode};
use wgui::{
    ClientEvent, HttpResponse, Item, StaticAsset, Wgui, button, custom_component, hstack, modal,
    text, text_input, textarea, vstack,
};

const SEARCH_INPUT_ID: u32 = 10;
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
    selected_video: Option<VideoFile>,
    selected_image: Option<ImageFile>,
    selected_file: Option<FileViewer>,
    file_viewer_expanded: bool,
    folder_context: Option<FolderContext>,
    show_add_source: bool,
    new_source_name: String,
    new_source_path: String,
    additional_sources: Vec<AddedSource>,
    viewing_this_computer: bool,
}

struct LocalEntry {
    path: PathBuf,
    name: String,
    is_directory: bool,
    is_symlink: bool,
    kind: &'static str,
    size: String,
    size_bytes: u64,
    modified: String,
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
            selected_video: None,
            selected_image: None,
            selected_file: None,
            file_viewer_expanded: false,
            folder_context: None,
            show_add_source: false,
            new_source_name: String::new(),
            new_source_path: String::new(),
            additional_sources: Vec::new(),
            viewing_this_computer: false,
        }
    }

    fn render(&self) -> Item {
        let version = format!("v{BUILD_VERSION}");
        let mut sidebar_items = vec![
            text("◕  PuppyDrive").color("#0f6175").padding(6),
            nav_item("▦  Overview", 1, false),
            nav_item("□  Files", 2, true),
            nav_item("▧  Photos", 3, false),
            nav_item("▣  Videos", 4, false),
            nav_item("◈  Backups", 5, false),
            nav_item("⇄  Transfers", 6, false),
            nav_item("◇  Nodes", 7, false),
            nav_item("⇩  Imports", 8, false),
            nav_item("⚙  Settings", 9, false),
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
            this_computer_nav_item(self.viewing_this_computer),
            source_nav_item("▦", "Home NAS", !self.viewing_this_computer, true),
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
            text(&version).padding(8).color("#6b7280"),
        ]);
        let sidebar = vstack(sidebar_items)
            .width(210)
            .spacing(4)
            .padding(8)
            .background_color("#f8fbfc");

        let header = hstack([
            hstack([
                text_input()
                    .id(SEARCH_INPUT_ID)
                    .placeholder("Search across all nodes")
                    .grow(1),
                text("Ctrl + K")
                    .padding(4)
                    .border("1px solid #dce5e8")
                    .color("#6b7280"),
            ])
            .grow(1)
            .padding(8)
            .border("1px solid #dce5e8")
            .background_color("#ffffff"),
            vstack([
                text("●   5 nodes online").color("#15803d"),
                text("All synced").color("#6b7280"),
            ])
            .spacing(2)
            .padding(6)
            .width(135)
            .border("1px solid #dce5e8")
            .background_color("#ffffff"),
            button("Alex ⌄")
                .width(80)
                .padding(8)
                .border("1px solid #dce5e8")
                .background_color("#ffffff"),
        ])
        .spacing(6)
        .padding(4)
        .border("1px solid #e4ebed")
        .background_color("#ffffff");

        let summary = hstack([
            stat_card("▰", "Total files", "128,732", "Across 5 nodes"),
            stat_card("▤", "Online nodes", "5 / 5", "All systems online"),
            stat_card("⇄", "Active transfers", "2", "2.4 MB/s"),
            stat_card("✓", "Backup health", "Healthy", "All backups OK"),
        ])
        .spacing(6)
        .padding_left(6)
        .padding_right(6);

        let content = self
            .files_panel()
            .grow(1)
            .padding_left(6)
            .padding_right(6)
            .overflow("hidden");

        let transfers = card(vstack([
            hstack([
                text("Recent transfers"),
                text("Backup policies").grow(1).color("#6b7280"),
                text("View all transfers").color("#0f7892"),
            ])
            .spacing(8)
            .padding_bottom(4),
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
        ]))
        .padding(6)
        .margin_left(4)
        .margin_right(4)
        .margin_bottom(4);

        let mut shell = vec![
            sidebar,
            vstack([header, summary, content, transfers])
                .grow(1)
                .fill(true)
                .overflow("hidden")
                .background_color("#f4f7f8"),
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
                ClientEvent::OnTextChanged(change) if change.id == SEARCH_INPUT_ID => {
                    log::debug!("search query changed: {:?}", change.value);
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
                ClientEvent::OnClick(click) => match click.id {
                    THIS_COMPUTER_SOURCE_ID => {
                        self.viewing_this_computer = true;
                        self.this_computer_path = self.this_computer_root.clone();
                        self.tree_view_root = self.this_computer_root.clone();
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
                            self.open_video(index as usize);
                        }
                    }
                    LOCAL_IMAGE_VIEW_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.open_image(index as usize);
                        }
                    }
                    LOCAL_TEXT_VIEW_ID if self.viewing_this_computer => {
                        if let Some(index) = click.inx {
                            self.open_text_file(index as usize);
                        }
                    }
                    CLOSE_FILE_VIEWER_ID => self.close_file_viewer(),
                    TOGGLE_FILE_VIEWER_SIZE_ID => {
                        self.file_viewer_expanded = !self.file_viewer_expanded;
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
                let modified = metadata
                    .and_then(|metadata| metadata.modified().ok())
                    .map_or_else(|| "—".to_owned(), format_modified);

                LocalEntry {
                    path,
                    name,
                    is_directory,
                    is_symlink,
                    kind,
                    size,
                    size_bytes,
                    modified,
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

    fn open_video(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if is_video_file(entry) {
            let Some(source_url) = media_source_url(&self.this_computer_root, &entry.path) else {
                return;
            };
            self.selected_video = Some(VideoFile {
                name: entry.name.clone(),
                size: entry.size.clone(),
                modified: entry.modified.clone(),
                source_url,
            });
            self.selected_image = None;
            self.selected_file = None;
            self.file_viewer_expanded = false;
        }
    }

    fn open_image(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if is_image_file(entry) {
            let Some(source_url) = media_source_url(&self.this_computer_root, &entry.path) else {
                return;
            };
            self.selected_image = Some(ImageFile {
                name: entry.name.clone(),
                size: entry.size.clone(),
                modified: entry.modified.clone(),
                source_url,
            });
            self.selected_video = None;
            self.selected_file = None;
            self.file_viewer_expanded = false;
        }
    }

    fn open_text_file(&mut self, index: usize) {
        let nodes = self.tree_nodes();
        let Some(entry) = nodes.get(index).map(|node| &node.entry) else {
            return;
        };
        if entry.is_directory || entry.is_symlink {
            return;
        }

        let Ok(path) = fs::canonicalize(&entry.path) else {
            return;
        };
        if !path.starts_with(&self.this_computer_root) {
            return;
        }
        let Ok(file) = fs::File::open(&path) else {
            return;
        };
        let mut bytes = Vec::new();
        if file
            .take(MAX_FILE_PREVIEW_BYTES)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return;
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
        self.file_viewer_expanded = false;
    }

    fn close_file_viewer(&mut self) {
        self.selected_video = None;
        self.selected_image = None;
        self.selected_file = None;
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

fn nav_item(label: &str, id: u32, active: bool) -> Item {
    let (background, color) = if active {
        ("#e5f4f7", "#0f6175")
    } else {
        ("#f8fbfc", "#1f2937")
    };

    button(label)
        .id(id)
        .width(180)
        .padding(5)
        .border("1px solid transparent")
        .background_color(background)
        .color(color)
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

fn stat_card(icon: &str, title: &str, value: &str, subtitle: &str) -> Item {
    card(hstack([
        text(icon)
            .padding(6)
            .color("#0f7892")
            .background_color("#e8f5f7"),
        vstack([
            hstack([text(title).grow(1), text(value)]),
            text(subtitle).color("#6b7280"),
        ])
        .spacing(1)
        .padding(4)
        .grow(1),
    ]))
    .grow(1)
    .max_width(280)
    .padding(0)
}

impl App {
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
        let expanded = self.file_viewer_expanded;
        let viewer = card(vstack([
            hstack([
                vstack([text("File viewer"), text(name).color("#6b7280")])
                    .grow(1)
                    .spacing(2),
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
                text("MP4 video").grow(1).color("#4b5563"),
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
    !entry.is_directory
        && !entry.is_symlink
        && entry
            .path
            .extension()
            .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
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

    let content_type = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("mp4"))
    {
        "video/mp4"
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
        format!("{} min ago", seconds / 60)
    } else if seconds < 86_400 {
        format!("{} hr ago", seconds / 3_600)
    } else {
        let days = seconds / 86_400;
        format!("{days} days ago")
    }
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
