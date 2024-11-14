use std::net::SocketAddr;
use clap::Parser;
use gethostname::gethostname;
use pupynet::Pupynet;
use pupynet::PupynetImpl;
use types::Event;
use types::PeerCmd;
use types::State;
use uuid::Uuid;
use wgui::Wgui;

mod args;
mod types;
mod ui;
mod ws;
mod tcp;
mod protocol;
mod pupynet;

#[tokio::main]
async fn main() {
	simple_logger::init_with_level(log::Level::Info).unwrap();
	let args = args::Args::parse();
	let mut state = State::default();
	state.me.name = Some(gethostname().to_string_lossy().to_string());
	if state.me.id.is_none() {
		state.me.id = Some(Uuid::new_v4().to_string());
	}

	let ui_bind = args.ui_bind.parse::<SocketAddr>().unwrap();
	let mut wgui = Wgui::new(ui_bind);
	let mut pupynet = PupynetImpl::new();

	let hostname = gethostname().to_string_lossy().to_string();
	log::info!("hostname: {}", hostname);

	for peer_addr in &args.peer {
		pupynet.connect(peer_addr).unwrap();
	}

	for bind_addr in &args.bind {
		pupynet.bind(bind_addr).unwrap();
	}

	while let Some(event) = pupynet.wait().await {
		match event {
			Event::ConnectFailed { addr, err } => {
				log::error!("connect failed: addr: {}, err: {}", addr, err);
			}
			Event::PeerConnected(addr) => {
				state.get_peer_with_addr(&addr);
				let cmd = PeerCmd::Introduce { 
					name: state.me.name.clone().unwrap_or_default(),
					owner: state.me.owner.clone().unwrap_or_default(),
				};
				pupynet.send(&addr, cmd);
			}
			Event::PeerDisconnected(addr) => {
				let peer = state.get_peer_with_addr(&addr);
				peer.addr = None;
			}
			Event::PeerCmd {
				addr,
				cmd
			} => {

			}
		}
	}
}
