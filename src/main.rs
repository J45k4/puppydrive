use std::collections::HashSet;
use clap::Parser;
use server_manager::ServerManager;
use server_manager::ServerManagerEvent;
use types::Node;
use wgui::*;

mod args;
mod http_server;
mod server_manager;
mod types;

#[derive(Debug, Default)]
struct State {
	nodes: Vec<Node>,
	peers: Vec<String>,
	binds: Vec<String>,
}

fn td2(t: &str) -> Item {
	td(text(t)).text_align("center")
}

fn nodes_table(state: &State) -> Item {
	table([
		thead([
			tr([
				th(text("ID")),
				th(text("NAME")),
				th(text("TRAFFIC")),
				th(text("STATUS")),
			])
		]),
		tbody(
			state.nodes.iter().map(|node| {
				tr([
					td2(&node.id.to_string()),
					td2(&node.name),
					td2(&node.traffic.to_string()),
					td2(&node.status.to_string()),
				])
			})
		)
	])
}

fn navigation_bar() -> Item {
	hstack([
		hstack([
			text("Nodes").cursor("pointer"),
			text("Files").cursor("pointer"),
			text("Virtual folders").cursor("pointer")
		]).padding(10)
			.grow(1)
			.spacing(20),
		text("Settings"),
	])
}

struct App {
	wgui: Wgui,
	ui_clients: HashSet<usize>,
	state: State,
	server_manager: ServerManager,
}

impl App {
	pub fn new(peers: Vec<String>, binds: Vec<String>) -> App {
		App {
			wgui: Wgui::new("0.0.0.0:8832".parse().unwrap()),
			ui_clients: HashSet::new(),
			server_manager: ServerManager::new(binds.clone()),
			state: State { peers, binds, ..Default::default() },
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
			ServerManagerEvent::NewNodeConnected(node) => {
				self.state.nodes.push(node);
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

	async fn run(mut self) {
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

fn validate_peer(peer: &str) -> bool {
	if peer.starts_with("https://") {
		return true;
	}

	false
}

#[tokio::main]
async fn main() {
	simple_logger::init_with_level(log::Level::Info).unwrap();
	let args = args::Args::parse();

	for peer in &args.peer {
		if !validate_peer(peer) {
			log::error!("Invalid peer: {}", peer);
			return;
		}
	}

	log::info!("peers: {:?}", args.peer);
	App::new(args.peer, args.bind).run().await;
}
