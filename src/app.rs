use std::collections::HashSet;

use wgui::text;
use wgui::ClientEvent;
use wgui::Wgui;
use tokio::sync::mpsc;
use libp2p::{Multiaddr, PeerId};

use crate::network::{NetworkManager, NetworkEvent, NetworkCommand};
use crate::protocol::{PeerCmd, Introduce};

pub struct App {
    wgui: Wgui,
    clients: HashSet<usize>,
    network_event_receiver: Option<mpsc::UnboundedReceiver<NetworkEvent>>,
    network_command_sender: Option<mpsc::UnboundedSender<NetworkCommand>>,
    connected_peers: HashSet<PeerId>,
}

impl App {
    pub fn new() -> Self {
        log::info!("http://localhost:5613");
        Self {
            wgui: Wgui::new("127.0.0.1:5613".parse().unwrap()),
            clients: HashSet::new(),
            network_event_receiver: None,
            network_command_sender: None,
            connected_peers: HashSet::new(),
        }
    }

    pub fn set_network_channels(
        &mut self, 
        event_receiver: mpsc::UnboundedReceiver<NetworkEvent>,
        command_sender: mpsc::UnboundedSender<NetworkCommand>,
    ) {
        self.network_event_receiver = Some(event_receiver);
        self.network_command_sender = Some(command_sender);
    }

    async fn handle_event(&mut self, event: ClientEvent) {
        match event {
            ClientEvent::Disconnected { id } => { self.clients.remove(&id); },
            ClientEvent::Connected { id } => { self.clients.insert(id); },
            ClientEvent::PathChanged(_path_changed) => {},
            ClientEvent::Input(_input_query) => {},
            ClientEvent::OnClick(_on_click) => {},
            ClientEvent::OnTextChanged(_on_text_changed) => {},
            ClientEvent::OnSliderChange(_on_slider_change) => {},
            ClientEvent::OnSelect(_on_select) => {}
        };
        self.render().await;
    }

    async fn handle_network_event(&mut self, event: NetworkEvent) {
        match event {
            NetworkEvent::PeerConnected(peer_id) => {
                log::info!("Peer connected: {}", peer_id);
                self.connected_peers.insert(peer_id);
                
                // Send introduction to new peer
                if let Some(sender) = &self.network_command_sender {
                    let introduce = Introduce {
                        id: "puppydrive-node".to_string(),
                        name: gethostname::gethostname().to_string_lossy().to_string(),
                        owner: "anonymous".to_string(),
                    };
                    let _ = sender.send(NetworkCommand::Introduce(introduce));
                }
            },
            NetworkEvent::PeerDisconnected(peer_id) => {
                log::info!("Peer disconnected: {}", peer_id);
                self.connected_peers.remove(&peer_id);
            },
            NetworkEvent::MessageReceived(peer_id, cmd) => {
                log::info!("Received message from {}: {:?}", peer_id, cmd);
                // Handle different command types here
                match cmd {
                    PeerCmd::Introduce(intro) => {
                        log::info!("Peer {} introduced as: {} (owner: {})", 
                                 peer_id, intro.name, intro.owner);
                    },
                    _ => {
                        log::info!("Unhandled command: {:?}", cmd);
                    }
                }
            },
            NetworkEvent::PeerIntroduced(peer_id, intro) => {
                log::info!("Peer {} introduced: {:?}", peer_id, intro);
            },
            NetworkEvent::DiscoveredPeer(peer_id, _addrs) => {
                log::info!("Discovered peer: {}", peer_id);
            },
        }
        self.render().await;
    }

    async fn render(&mut self) {
        for &client_id in &self.clients {
            let peer_count = self.connected_peers.len();
            let status_text = if peer_count > 0 {
                format!("PuppyDrive - Connected to {} peer(s)", peer_count)
            } else {
                "PuppyDrive - No peers connected".to_string()
            };
            self.wgui.render(client_id, text(&status_text)).await;
        }
    }

    pub async fn run(&mut self) {
        tokio::select! {
            event = self.wgui.next() => {
                match event {
                    Some(event) => {
                        log::info!("event: {:?}", event);
                        self.handle_event(event).await;
                    }
                    None => {
                        log::warn!("wgui event stream ended");
                    }
                }
            }
            network_event = async {
                if let Some(ref mut receiver) = self.network_event_receiver {
                    receiver.recv().await
                } else {
                    // If no network receiver, wait indefinitely
                    std::future::pending().await
                }
            } => {
                if let Some(event) = network_event {
                    self.handle_network_event(event).await;
                }
            }
        }
    }
}