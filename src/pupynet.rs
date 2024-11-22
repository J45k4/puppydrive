use std::collections::HashMap;
use std::collections::VecDeque;

use anyhow::bail;
use anyhow::Ok;
use tokio::sync::mpsc;

use crate::protocol::PupynetProtocol;
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
}

pub struct PupynetImpl {
	event_tx: mpsc::UnboundedSender<PupynetEvent>,
	event_rx: mpsc::UnboundedReceiver<PupynetEvent>,
	peers: HashMap<String, mpsc::UnboundedSender<PeerConnCmd>>,
	protocols: HashMap<String, PupynetProtocol>,
	new_events: VecDeque<Event>
}

impl PupynetImpl {
	pub fn new() -> Self {
		let (tx, rx) = mpsc::unbounded_channel();

		Self {
			event_tx: tx,
			event_rx: rx,
			peers: HashMap::new(),
			protocols: HashMap::new(),
			new_events: VecDeque::new()
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
		log::info!("[{}] sending cmd: {:?}", addr, cmd);
		let tx = match self.peers.get_mut(addr) {
			Some(tx) => tx,
			None => bail!("peer not found: {}", addr)
		};
		let protocol = self.protocols.entry(addr.to_string()).or_insert(PupynetProtocol::new());
		let data = protocol.encode(cmd);
		let cmd = PeerConnCmd::Send(data);
		tx.send(cmd).map_err(|err| anyhow::anyhow!(err))?;
		Ok(())
	}

	async fn wait(&mut self) -> Option<Event> {
		loop {
			if let Some(event) = self.new_events.pop_front() {
				return Some(event);
			}

			match self.event_rx.recv().await {
				Some(event) => {
					match event {
						PupynetEvent::PeerConnected { addr, tx } => {
							self.peers.insert(addr.to_string(), tx);
							self.new_events.push_back(Event::PeerConnected { addr });
						}
						PupynetEvent::PeerDisconnected { addr } => {
							self.peers.remove(&addr);
							self.new_events.push_back(Event::PeerDisconnected { addr });
						}
						PupynetEvent::PeerData { addr, data } => {
							let protocol = self.protocols.entry(addr.to_string()).or_insert(PupynetProtocol::new());
							protocol.parse(&data);
							while let Some(cmd) = protocol.next() {
								self.new_events.push_back(Event::PeerData { addr: addr.clone(), cmd });
							}
						}
					}
				}
				None => {
					return None;
				}
			}
		}
	}
}