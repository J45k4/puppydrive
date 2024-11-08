use std::collections::HashMap;
use std::net::SocketAddr;
use tokio::sync::mpsc;
use crate::http_server::HttpServer;
use crate::types::*;

#[derive(Debug)]
pub struct NodeConnected {
    pub node: Node,
    pub server: SocketAddr,
}

#[derive(Debug)]
pub enum ServerManagerEvent {
    NodeConnected(NodeConnected),
    NodeDisconnected(String),
    NodeMessageReq(PeerReq),
    NodeMessageRes(PeerRes),
}

pub enum ServerCmd {

}

pub struct ServerManager {
    servers: HashMap<SocketAddr, mpsc::UnboundedSender<ServerCmd>>,
    rx: mpsc::UnboundedReceiver<ServerManagerEvent>,
}

impl ServerManager {
    pub fn new(binds: Vec<SocketAddr>) -> Self {
        let (tx, rx) = mpsc::unbounded_channel();

        let mut servers = HashMap::new();
        for bind in binds {
            let (tx, rx) = mpsc::unbounded_channel();
            servers.insert(bind, tx);
            tokio::spawn(async move {
                HttpServer::new(bind, rx).await.run().await;
            });
        }

        Self { 
            servers: HashMap::new(), 
            rx 
        }
    }

    pub async fn next_event(&mut self) -> Option<ServerManagerEvent> {
        self.rx.recv().await
    }
}
