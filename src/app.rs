use std::collections::HashSet;
use crate::protocol::Introduce;
use crate::pupynet::Pupynet;
use crate::types::*;
use wgui::*;
use crate::ui::*;

pub struct App<P> {
	wgui: Wgui,
	pupynet: P,
	ui_clients: HashSet<usize>,
	state: State,
} 

impl<P: Pupynet> App<P> {
	pub fn new(state: State, wgui: Wgui, pupynet: P) -> Self {
		Self {
			state,
			wgui,
			pupynet,
			ui_clients: HashSet::new(),
		}
	}

	async fn render_ui(&mut self) {
		let item = render_ui(&self.state);
		for client_id in &self.ui_clients {
			self.wgui.render(*client_id, item.clone()).await;
		}
	}

	async fn handle_wgui_event(&mut self, event: ClientEvent) {
		match event {
			ClientEvent::Disconnected { id } => { self.ui_clients.remove(&id); },
			ClientEvent::Connected { id } => { self.ui_clients.insert(id); },
			_ => {}
		};
	}

	async fn handle_pupynet_event(&mut self, event: Event) {
		match event {
			Event::ConnectFailed { addr, err } => {
				log::error!("connect failed: addr: {}, err: {}", addr, err);
			}
			Event::PeerConnected { addr } => {
				let peer = self.state.get_peer_with_addr(&addr);
				peer.introduced = true;
				let cmd = PeerCmd::Introduce(Introduce {
					id: self.state.me.id.clone().unwrap_or_default(),
					name: self.state.me.name.clone().unwrap_or_default(),
					owner: self.state.me.owner.clone().unwrap_or_default(),
				});
				self.pupynet.send(&addr, cmd).unwrap();
			}
			Event::PeerDisconnected { addr } => {
				let peer = self.state.get_peer_with_addr(&addr);
				peer.addr = None;
			}
			Event::PeerData {
				addr,
				cmd
			} => {
				log::info!("[{}] received cmd: {:?}", addr, cmd);

				match cmd {
					PeerCmd::ReadFile { node_id, path, offset, length } => todo!(),
					PeerCmd::WriteFile { node_id, path, offset, data } => todo!(),
					PeerCmd::RemoveFile { node_id, path } => todo!(),
					PeerCmd::CreateFolder { node_id, path } => todo!(),
					PeerCmd::RenameFolder { node_id, path, new_name } => todo!(),
					PeerCmd::RemoveFolder { node_id, path } => todo!(),
					PeerCmd::ListFolderContents { node_id, path, offset, length, recursive } => todo!(),
					PeerCmd::Introduce(introduce) => {
						let peer = self.state.get_peer_with_addr(&addr);
						if !peer.introduced {
							peer.introduced = true;
							let cmd = PeerCmd::Introduce(Introduce {
								id: self.state.me.id.clone().unwrap_or_default(),
								name: self.state.me.name.clone().unwrap_or_default(),
								owner: self.state.me.owner.clone().unwrap_or_default(),
							});
							self.pupynet.send(&addr, cmd).unwrap();
						}
					}
				}
			}
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.wgui.next() => {
					match event {
						Some(e) => {
							println!("Event: {:?}", e);
							self.handle_wgui_event(e).await;
						},
						None => {
							log::error!("wgui closed");
							break;
						},
					}
				}
				event = self.pupynet.wait() => {
					match event {
						Some(e) => {
							self.handle_pupynet_event(e).await;
						},
						None => {
							log::error!("pupynet closed");
							break;
						},
					}
				}
			}
			self.render_ui().await;
		}
	}
}
