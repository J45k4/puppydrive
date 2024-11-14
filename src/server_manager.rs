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
    PeerConnected(Peer),
	PeerDisconnected(Peer),
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
        for bind in binds {
            log::info!("starting http server on {}", bind);
            let tx = tx.clone();
            tokio::spawn(async move {
                HttpServer::new(bind, tx).await.run().await;
                log::info!("http server stopped on {}", bind);
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
