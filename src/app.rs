use std::collections::HashSet;
use std::time::Duration;
use crate::protocol::Introduce;
use crate::protocol::PeerCmd;
use crate::pupynet::Pupynet;
use crate::timer::Timer;
use crate::types::*;
use wgui::*;
use crate::ui::*;

pub struct App<P> {
	wgui: Wgui,
	pupynet: P,
	ui_clients: HashSet<usize>,
	state: State,
	udp_broadcast_timer: Timer,
} 

impl<P: Pupynet> App<P> {
	pub fn new(state: State, wgui: Wgui, pupynet: P) -> Self {
		Self {
			state,
			wgui,
			pupynet,
			ui_clients: HashSet::new(),
			udp_broadcast_timer: Timer::new().repeat(true).time(Duration::from_secs(5)),
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
				log::info!("[{}] connected", addr);
				let peer = self.state.get_peer_with_addr(&addr);
				peer.introduced = true;
				let cmd = PeerCmd::Introduce(Introduce {
					id: self.state.me.id.clone(),
					name: self.state.me.name.clone(),
					owner: self.state.me.owner.clone().unwrap_or_default(),
				});
				self.pupynet.send(&addr, cmd).await.unwrap();
			}
			Event::PeerDisconnected { addr } => {
				let peer = self.state.get_peer_with_addr(&addr);
				peer.addr = None;
			}
			Event::PeerCmd {
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
						if introduce.id == self.state.me.id {
							log::info!("it is me");
							return;
						}

						let peer = self.state.get_peer_with_addr(&addr);
						peer.id = introduce.id;
						peer.name = introduce.name;
						if !peer.introduced {
							peer.introduced = true;
							let cmd = PeerCmd::Introduce(Introduce {
								id: self.state.me.id.clone(),
								name: self.state.me.name.clone(),
								owner: self.state.me.owner.clone().unwrap_or_default(),
							});
							self.pupynet.send(&addr, cmd).await.unwrap();
						}
					}
					PeerCmd::Hello => {
						log::info!("[{}] received hello", addr);
					}
				};
			}
		}
	}

	pub async fn run(mut self) {
		loop {
			tokio::select! {
				event = self.wgui.next() => {
					match event {
						Some(e) => {
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
				_ = self.udp_broadcast_timer.wait() => {
					let introduce = Introduce {
						id: self.state.me.id.clone(),
						name: self.state.me.name.clone(),
						owner: self.state.me.owner.clone().unwrap_or_default(),
					};
					let cmd = PeerCmd::Introduce(introduce);
					self.pupynet.send("udp://255.255.255.255:7764", cmd).await;
				}
			}
			self.render_ui().await;
		}
	}
}
