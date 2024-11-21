use std::collections::HashMap;

use anyhow::bail;
use anyhow::Ok;
use tokio::sync::mpsc;

use crate::types::Event;
use crate::types::PeerCmd;
use crate::types::PeerConnCmd;
use crate::ws;

pub enum PupynetEvent {
	PeerConnected {
		addr: String,
		tx: mpsc::UnboundedSender<PeerConnCmd>
	},
	PeerDisconnected {
		addr: String
	},
	PeerData {
		addr: String,
		data: Vec<u8>
	}
}

pub trait Pupynet {
	fn connect(&mut self, addr: &str) -> anyhow::Result<()>;
	fn bind<A: AsRef<str>>(&mut self, addr: A) -> anyhow::Result<()>;
	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()>;
	async fn wait(&mut self) -> Option<Event>;	
	fn poll(&mut self, timeout: std::time::Duration) -> Option<Event>;
}

pub struct PupynetImpl {
	event_tx: mpsc::UnboundedSender<PupynetEvent>,
	event_rx: mpsc::UnboundedReceiver<PupynetEvent>,
	peers: HashMap<String, mpsc::UnboundedSender<PeerConnCmd>>
}

impl PupynetImpl {
	pub fn new() -> Self {
		let (tx, rx) = mpsc::unbounded_channel();

		Self {
			event_tx: tx,
			event_rx: rx,
			peers: HashMap::new()
		}
	}
}

impl Pupynet for PupynetImpl {
	fn connect(&mut self, addr: &str) -> anyhow::Result<()> {
		let addr = addr.to_string();
		let event_tx = self.event_tx.clone();
		if addr.starts_with("ws://") || addr.starts_with("wss://") {
			let (tx, rx) = mpsc::unbounded_channel();
			self.peers.insert(addr.to_string(), tx);	
			tokio::spawn(async move {
				ws::connect(addr, event_tx, rx).await;
			});
			return Ok(());
		}

		bail!("unsupported protocol {}", addr);
	}

	fn bind<A: AsRef<str>>(&mut self, addr: A) -> anyhow::Result<()> {
		let addr = addr.as_ref().to_string();
		let event_tx = self.event_tx.clone();
		if addr.starts_with("ws://") {
			tokio::spawn(async move {
				ws::bind(addr, event_tx).await;
			});
			return Ok(());
		}
		
		bail!("unsupported protocol {}", addr);
	}

	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()> {
		Ok(())
	}

	async fn wait(&mut self) -> Option<Event> {
		loop {
			match self.event_rx.recv().await {
				Some(event) => {
					match event {
						PupynetEvent::PeerConnected { addr, tx } => {
							self.peers.insert(addr.to_string(), tx);
							return Some(Event::PeerConnected { addr });
						}
						PupynetEvent::PeerDisconnected { addr } => {
							return Some(Event::PeerDisconnected { addr });
						}
						PupynetEvent::PeerData { addr, data } => {
							return Some(Event::PeerData { addr, data });
						}
					}
				}
				None => {
					return None;
				}
			}
		}
	}

	fn poll(&mut self, timeout: std::time::Duration) -> Option<Event> {
		todo!()
	}
}