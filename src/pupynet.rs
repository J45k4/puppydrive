use std::collections::HashMap;
use std::collections::VecDeque;
use std::sync::Arc;

use anyhow::bail;
use anyhow::Ok;
use tokio::net::UdpSocket;
use tokio::sync::mpsc;
use crate::protocol::PeerCmd;
use crate::protocol::PupynetProtocol;
use crate::types::Event;
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
	async fn bind<A: AsRef<str>>(&mut self, addr: A) -> anyhow::Result<()>;
	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()>;
	async fn udp_broadcast(&mut self, addr: &str, cmd: PeerCmd);
	async fn wait(&mut self) -> Option<Event>;
}

pub struct PupynetImpl {
	event_tx: mpsc::UnboundedSender<PupynetEvent>,
	event_rx: mpsc::UnboundedReceiver<PupynetEvent>,
	peers: HashMap<String, mpsc::UnboundedSender<PeerConnCmd>>,
	protocols: HashMap<String, PupynetProtocol>,
	new_events: VecDeque<Event>,
	udb_sockets: HashMap<u16, Arc<UdpSocket>>,
	broadcast_socket: Option<Arc<UdpSocket>>
}

impl PupynetImpl {
	pub async fn new() -> Self {
		let (tx, rx) = mpsc::unbounded_channel();
		let broadcast_socket = match UdpSocket::bind("0.0.0.0:7764").await {
			Result::Ok(socket) => {
				log::info!("bound broadcast socket");
				let socket = Arc::new(socket);
				let tx = tx.clone();
				socket.set_broadcast(true);
				{
					let broadcast_socket = socket.clone();
					tokio::spawn(async move {
						loop {
							let mut buf = [0; 1024];
							let (len, addr) = broadcast_socket.recv_from(&mut buf).await.unwrap();
							log::info!("received {} bytes from {}", len, addr);
							let addr = format!("udp://{}", addr);
							tx.send(PupynetEvent::PeerData { addr, data: buf[0..len].to_vec() }).unwrap();
						}
					});
				}
				Some(socket)
			},
			Err(err) => {
				log::error!("error binding broadcast socket: {}", err);
				let socket = UdpSocket::bind("0.0.0.0:0").await.unwrap();
				socket.set_broadcast(true).unwrap();
				Some(Arc::new(socket))
			},
		};


		Self {
			event_tx: tx,
			event_rx: rx,
			peers: HashMap::new(),
			protocols: HashMap::new(),
			new_events: VecDeque::new(),
			udb_sockets: HashMap::new(),
			broadcast_socket
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

	async fn bind<A: AsRef<str>>(&mut self, addr: A) -> anyhow::Result<()> {
		let addr = addr.as_ref().to_string();
		let event_tx = self.event_tx.clone();
		if addr.starts_with("ws://") {
			tokio::spawn(async move {
				ws::bind(addr, event_tx).await;
			});
			return Ok(());
		}

		if addr.starts_with("udp://") {
			let port= addr.split(":").last().unwrap().parse::<u16>()?;
			let socket = UdpSocket::bind(addr.replace("udp://", "")).await?;
			let socket = Arc::new(socket);
			self.udb_sockets.insert(port, socket.clone());
		}
		
		bail!("unsupported protocol {}", addr);
	}

	fn send(&mut self, addr: &str, cmd: PeerCmd) -> anyhow::Result<()> {
		if addr.starts_with("udp://") {
			return Ok(());
		}

		log::info!("[{}] sending cmd: {:?}", addr, cmd);
		let tx = match self.peers.get_mut(addr) {
			Some(tx) => tx,
			None => bail!("peer not found: {}", addr)
		};
		let data = cmd.serialize();
		let cmd = PeerConnCmd::Send(data);
		tx.send(cmd).map_err(|err| anyhow::anyhow!(err))?;
		Ok(())
	}

	async fn udp_broadcast(&mut self, addr: &str, cmd: PeerCmd) {
		let socket = match self.broadcast_socket {
			Some(ref socket) => socket,
			None => return
		};
		log::info!("[{}] broadcasting cmd: {:?}", addr, cmd);
		let data = cmd.serialize();
		socket.send_to(&data, addr).await;
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
								self.new_events.push_back(Event::PeerCmd { addr: addr.clone(), cmd });
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