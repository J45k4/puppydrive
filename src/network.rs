use std::collections::HashMap;
use std::time::Duration;

use libp2p::{
    gossipsub, mdns, noise, identify,
    swarm::{NetworkBehaviour, SwarmEvent},
    tcp, yamux, Multiaddr, PeerId, Swarm, SwarmBuilder, Transport,
};
use tokio::sync::mpsc;
use futures::stream::StreamExt;

use crate::protocol::{PeerCmd, Introduce};
use crate::types::Peer;

// Network events that can be sent to the application
#[derive(Debug)]
pub enum NetworkEvent {
    PeerConnected(PeerId),
    PeerDisconnected(PeerId),
    MessageReceived(PeerId, PeerCmd),
    PeerIntroduced(PeerId, Introduce),
    DiscoveredPeer(PeerId, Vec<Multiaddr>),
}

// Commands that can be sent to the network
#[derive(Debug)]
pub enum NetworkCommand {
    SendMessage(PeerId, PeerCmd),
    ConnectToPeer(Multiaddr),
    Introduce(Introduce),
    Broadcast(PeerCmd),
}

// Custom network behaviour combining different protocols
#[derive(NetworkBehaviour)]
pub struct PuppyDriveBehaviour {
    pub gossipsub: gossipsub::Behaviour,
    pub mdns: mdns::tokio::Behaviour,
    pub identify: identify::Behaviour,
}

impl PuppyDriveBehaviour {
    pub fn new(local_key: &libp2p::identity::Keypair) -> Result<Self, Box<dyn std::error::Error>> {
        // Create a gossipsub topic for puppydrive messages
        let gossipsub_topic = gossipsub::IdentTopic::new("puppydrive");
        
        // Configure gossipsub
        let gossipsub_config = gossipsub::ConfigBuilder::default()
            .heartbeat_interval(Duration::from_secs(10))
            .validation_mode(gossipsub::ValidationMode::Strict)
            .build()
            .expect("Valid config");
        
        let mut gossipsub = gossipsub::Behaviour::new(
            gossipsub::MessageAuthenticity::Signed(local_key.clone()),
            gossipsub_config,
        )?;
        
        gossipsub.subscribe(&gossipsub_topic)?;
        
        // Configure mDNS for local peer discovery
        let mdns = mdns::tokio::Behaviour::new(mdns::Config::default(), local_key.public().to_peer_id())?;
        
        // Configure identify protocol
        let identify = identify::Behaviour::new(identify::Config::new(
            "/puppydrive/1.0.0".to_string(),
            local_key.public(),
        ));
        
        Ok(PuppyDriveBehaviour {
            gossipsub,
            mdns,
            identify,
        })
    }
}

pub struct NetworkManager {
    swarm: Swarm<PuppyDriveBehaviour>,
    event_sender: mpsc::UnboundedSender<NetworkEvent>,
    command_receiver: mpsc::UnboundedReceiver<NetworkCommand>,
    peers: HashMap<PeerId, Peer>,
}

impl NetworkManager {
    pub fn new() -> Result<
        (
            Self,
            mpsc::UnboundedReceiver<NetworkEvent>,
            mpsc::UnboundedSender<NetworkCommand>,
        ),
        Box<dyn std::error::Error>,
    > {
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let (command_sender, command_receiver) = mpsc::unbounded_channel();

        // Build the swarm using the new builder API
        let swarm = SwarmBuilder::with_new_identity()
            .with_tokio()
            .with_tcp(
                tcp::Config::default(),
                noise::Config::new,
                yamux::Config::default,
            )?
            .with_behaviour(|key| {
                // Create behaviour with the key from the builder
                PuppyDriveBehaviour::new(key).unwrap()
            })?
            .with_swarm_config(|cfg| cfg.with_idle_connection_timeout(Duration::from_secs(60)))
            .build();

        let manager = NetworkManager {
            swarm,
            event_sender,
            command_receiver,
            peers: HashMap::new(),
        };

        Ok((manager, event_receiver, command_sender))
    }

    pub async fn start_listening(&mut self, addr: Multiaddr) -> Result<(), Box<dyn std::error::Error>> {
        self.swarm.listen_on(addr)?;
        Ok(())
    }

    pub async fn run(&mut self) {
        loop {
            tokio::select! {
                event = self.swarm.select_next_some() => {
                    self.handle_swarm_event(event).await;
                }
                command = self.command_receiver.recv() => {
                    if let Some(command) = command {
                        self.handle_command(command).await;
                    } else {
                        break;
                    }
                }
            }
        }
    }

    async fn handle_swarm_event(&mut self, event: SwarmEvent<PuppyDriveBehaviourEvent>) {
        match event {
            SwarmEvent::NewListenAddr { address, .. } => {
                println!("Listening on {address}");
            }
            SwarmEvent::Behaviour(PuppyDriveBehaviourEvent::Mdns(mdns::Event::Discovered(list))) => {
                for (peer_id, multiaddr) in list {
                    println!("Discovered peer: {peer_id} at {multiaddr}");
                    let _ = self.event_sender.send(NetworkEvent::DiscoveredPeer(peer_id, vec![multiaddr]));
                    
                    // Automatically connect to discovered peers
                    if let Err(e) = self.swarm.dial(peer_id) {
                        println!("Failed to dial {peer_id}: {e}");
                    }
                }
            }
            SwarmEvent::Behaviour(PuppyDriveBehaviourEvent::Mdns(mdns::Event::Expired(list))) => {
                for (peer_id, _multiaddr) in list {
                    println!("mDNS record expired for peer: {peer_id}");
                }
            }
            SwarmEvent::Behaviour(PuppyDriveBehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message,
                ..
            })) => {
                println!("Received message from {peer_id}: {:?}", String::from_utf8_lossy(&message.data));
                
                // Try to parse the message as a PeerCmd
                if let Ok(cmd) = serde_json::from_slice::<PeerCmd>(&message.data) {
                    let _ = self.event_sender.send(NetworkEvent::MessageReceived(peer_id, cmd));
                }
            }
            SwarmEvent::Behaviour(PuppyDriveBehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                println!("Identified peer {peer_id}: {info:?}");
            }
            SwarmEvent::ConnectionEstablished { peer_id, .. } => {
                println!("Connected to {peer_id}");
                let _ = self.event_sender.send(NetworkEvent::PeerConnected(peer_id));
                
                // Add peer to our list
                let peer = Peer {
                    id: peer_id.to_string(),
                    name: String::new(),
                    owner: None,
                    addr: None,
                    introduced: false,
                };
                self.peers.insert(peer_id, peer);
            }
            SwarmEvent::ConnectionClosed { peer_id, .. } => {
                println!("Disconnected from {peer_id}");
                let _ = self.event_sender.send(NetworkEvent::PeerDisconnected(peer_id));
                self.peers.remove(&peer_id);
            }
            SwarmEvent::OutgoingConnectionError { peer_id, error, .. } => {
                if let Some(peer_id) = peer_id {
                    println!("Failed to connect to {peer_id}: {error}");
                }
            }
            SwarmEvent::IncomingConnectionError { .. } => {}
            _ => {}
        }
    }

    async fn handle_command(&mut self, command: NetworkCommand) {
        match command {
            NetworkCommand::SendMessage(_peer_id, cmd) => {
                if let Ok(message) = serde_json::to_vec(&cmd) {
                    if let Err(e) = self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new("puppydrive"), message)
                    {
                        println!("Failed to publish message: {e}");
                    }
                }
            }
            NetworkCommand::ConnectToPeer(addr) => {
                if let Err(e) = self.swarm.dial(addr) {
                    println!("Failed to dial address: {e}");
                }
            }
            NetworkCommand::Introduce(introduce) => {
                if let Ok(message) = serde_json::to_vec(&introduce) {
                    if let Err(e) = self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new("puppydrive-intro"), message)
                    {
                        println!("Failed to publish introduction: {e}");
                    }
                }
            }
            NetworkCommand::Broadcast(cmd) => {
                if let Ok(message) = serde_json::to_vec(&cmd) {
                    if let Err(e) = self.swarm
                        .behaviour_mut()
                        .gossipsub
                        .publish(gossipsub::IdentTopic::new("puppydrive"), message)
                    {
                        println!("Failed to broadcast message: {e}");
                    }
                }
            }
        }
    }

    pub fn get_local_peer_id(&self) -> PeerId {
        *self.swarm.local_peer_id()
    }

    pub fn connected_peers(&self) -> Vec<PeerId> {
        self.peers.keys().cloned().collect()
    }
}