use std::net::SocketAddr;
use app::App;
use clap::Parser;
use gethostname::gethostname;
use pupynet::Pupynet;
use pupynet::PupynetImpl;
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
mod app;
mod timer;

#[tokio::main]
async fn main() {
	simple_logger::init_with_level(log::Level::Info).unwrap();
	let args = args::Args::parse();
	let mut state = State::default();
	state.me.name = gethostname().to_string_lossy().to_string();
	if state.me.id.is_empty() {
		state.me.id = Uuid::new_v4().to_string();
	}
	log::info!("my id: {}", state.me.id);

	let ui_bind = args.ui_bind.parse::<SocketAddr>().unwrap();
	let wgui = Wgui::new(ui_bind);
	let mut pupynet = PupynetImpl::new().await;

	let hostname = gethostname().to_string_lossy().to_string();
	log::info!("hostname: {}", hostname);

	for peer_addr in &args.peer {
		pupynet.connect(peer_addr).unwrap();
	}

	for bind_addr in &args.bind {
		pupynet.bind(bind_addr).await.unwrap();
	}

	App::new(state, wgui, pupynet).run().await;
}
