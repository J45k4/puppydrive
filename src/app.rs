use std::collections::HashSet;
use std::env;
use std::net::SocketAddr;

use wgui::{ClientEvent, Item, Wgui, button, hstack, text, text_input, vstack};

const SEARCH_INPUT_ID: u32 = 10;

pub struct App {
    wgui: Wgui,
    client_ids: HashSet<usize>,
}

impl App {
    pub fn new() -> Self {
        let bind_addr = env::var("BIND_ADDR").unwrap_or_else(|_| "127.0.0.1:5777".into());
        let bind_addr: SocketAddr = bind_addr.parse().unwrap_or_else(|error| {
            panic!("invalid BIND_ADDR '{bind_addr}': {error}");
        });

        Self {
            wgui: Wgui::new(bind_addr),
            client_ids: HashSet::new(),
        }
    }

    fn render(&self) -> Item {
        let sidebar = vstack([
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
            text("Sources").padding_top(6).color("#6b7280"),
            source_nav_item("▣", "This Computer", false, true),
            source_nav_item("▦", "Home NAS", true, true),
            source_nav_item("☁", "PuppyCloud", false, true),
            source_nav_item("▰", "Laptop", false, true),
            source_nav_item("▤", "Camera SD Card", false, false),
            vstack([
                text("●  All systems healthy").color("#16803a"),
                text("PuppyDrive daemon").color("#6b7280"),
                text("v1.4.2").color("#6b7280"),
            ])
            .spacing(3)
            .padding(8)
            .border("1px solid #dfe8eb")
            .background_color("#ffffff"),
        ])
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

        let content = hstack([files_panel().grow(1), details_panel()])
            .grow(1)
            .spacing(6)
            .padding_left(6)
            .padding_right(6);

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

        hstack([
            sidebar,
            vstack([header, summary, content, transfers])
                .grow(1)
                .fill(true)
                .background_color("#f4f7f8"),
        ])
        .fill(true)
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
                event => {
                    log::debug!("wgui client {client_id} event: {event:?}");
                }
            }
        }
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

fn files_panel() -> Item {
    card(vstack([
        hstack([
            text("Home NAS  ›  Media  ›").grow(1).color("#6b7280"),
            text("Family Videos"),
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

fn details_panel() -> Item {
    card(vstack([
        hstack([text("holiday-video-2025.mp4").grow(1), text("×")]).padding_bottom(10),
        text("▶  Family beach holiday\n     00:00 / 01:32")
            .padding(8)
            .color("#ffffff")
            .background_color("#1f2937"),
        text("Type       MP4 Video"),
        text("Size       3.45 GB").color("#4b5563"),
        text("Modified   Apr 21, 2025 10:18 AM").color("#4b5563"),
        text("Resolution 3840 × 2160 (4K)").color("#4b5563"),
        text("Locations").padding_top(6),
        text("▦  Home NAS       ●  Complete copy").color("#4b5563"),
        text("☁  PuppyCloud     ●  Backup copy").color("#4b5563"),
        text("Actions").padding_top(6),
        hstack([button("▷  Play").grow(1), button("＋  Add backup").grow(1)]).spacing(8),
        hstack([
            button("⇩  Download").grow(1),
            button("□  Open location").grow(1),
        ])
        .spacing(8),
    ]))
    .width(260)
    .spacing(4)
    .padding(6)
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
