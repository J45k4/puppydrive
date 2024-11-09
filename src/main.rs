use std::net::SocketAddr;
use app::App;
use clap::Parser;
use peer::Peer;

mod args;
mod http_server;
mod server_manager;
mod types;
mod app;
mod ui;
mod ws;
mod peer;

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

	for peer_addr in &args.peer {
		let peer = Peer::connect(peer_addr).await.unwrap();
		log::info!("peer connected: {}", peer_addr);
	}

	let binds: Vec<SocketAddr> = args.bind.iter().map(|bind| bind.parse::<SocketAddr>().unwrap()).collect();

	log::info!("peers: {:?}", args.peer);

	let ui_bind = args.ui_bind.parse::<SocketAddr>().unwrap();
	App::new(args.peer, binds, ui_bind).run().await;
}
