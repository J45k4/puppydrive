use std::collections::HashSet;
use std::net::SocketAddr;
use crate::types::*;
use tokio::sync::mpsc::UnboundedReceiver;
use tokio::sync::mpsc::UnboundedSender;
use wgui::*;
use crate::ui::*;

pub struct App {
	wgui: Wgui,
	event_rx: UnboundedReceiver<Event>,
	ui_clients: HashSet<usize>,
	state: State,
} 

impl App {
	pub fn new(event_rx: UnboundedReceiver<Event>, ui_bind: SocketAddr, peers_tx: Vec<UnboundedSender<PeerCmd>>) -> Self {
		Self {
			wgui: Wgui::new(ui_bind),
			event_rx,
			ui_clients: HashSet::new(),
			state: State::default()
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

	async fn handle_event(&mut self, event: Event) {
		match event {
			Event::PeerConnected(_) => {},
			Event::PeerDisconnected(_) => {},
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
							println!("No event");
							break;
						},
					}
				}
				event = self.event_rx.recv() => {
					match event {
						Some(e) => {
							self.handle_event(e).await;
						},
						None => {
							log::error!("event_rx closed");
							break;
						},
					}
				}
			}
			self.render_ui().await;
		}
	}
}
