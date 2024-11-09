use std::collections::HashSet;
use std::net::SocketAddr;
use crate::peer::Peer;
use crate::server_manager::ServerManager;
use crate::server_manager::ServerManagerEvent;
use crate::types::*;
use wgui::*;
use crate::ui::*;

pub struct App {
	wgui: Wgui,
	ui_clients: HashSet<usize>,
	state: State,
	server_manager: ServerManager,
    peers: Vec<Peer>,
}

impl App {
	pub fn new(peers: Vec<String>, binds: Vec<SocketAddr>, ui_bind: SocketAddr) -> App {
		App {
			wgui: Wgui::new(ui_bind),
			ui_clients: HashSet::new(),
			server_manager: ServerManager::new(binds.clone()),
			state: State { peers, binds, ..Default::default() },
			peers: Vec::new(),
		}
	}

	async fn render_ui(&mut self) {
		let item = vstack([
			navigation_bar(),
			nodes_table(&self.state),
		]);

		for client_id in &self.ui_clients {
			self.wgui.render(*client_id, item.clone()).await;
		}
	}

	async fn handle_event(&mut self, event: ClientEvent) {
		match event {
			ClientEvent::Disconnected { id } => { self.ui_clients.remove(&id); },
			ClientEvent::Connected { id } => { self.ui_clients.insert(id); },
			_ => {}
		};

		self.render_ui().await;
	}

	async fn handle_server_manager_event(&mut self, event: ServerManagerEvent) {
		match event {
			ServerManagerEvent::PeerConnected(peer) => {
				log::info!("new peer connected 🥳");
				peer.send(PeerReq {
					id: "qwerty".to_string(),
					cmd: NodeCmd::ListFolderContents {
						node_id: "qwerty".to_string(),
						path: "/".to_string(),
						offset: 0,
						length: 1024,
						recursive: false,
					},
				});
			},
			ServerManagerEvent::NodeDisconnected(id) => {
				self.state.nodes.retain(|node| node.id != id);
			},
			ServerManagerEvent::NodeMessageReq(req) => {

			},
			ServerManagerEvent::NodeMessageRes(res) => {

			},
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.wgui.next() => {
					match event {
						Some(e) => {
							println!("Event: {:?}", e);
							self.handle_event(e).await;
						},
						None => {
							println!("No event");
							break;
						},
					}
				}
				server_event = self.server_manager.next_event() => {
					match server_event {
						Some(e) => {
							log::info!("Server event: {:?}", e);
							self.handle_server_manager_event(e).await;
						},
						None => {}
					}
				}
			}
		}
	}
}
