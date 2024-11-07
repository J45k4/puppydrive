use crate::http_server::HttpServer;
use crate::types::*;

#[derive(Debug)]
pub enum ServerManagerEvent {
    NewNodeConnected(Node),
    NodeDisconnected(String),
    NodeMessageReq(NodeMessageReq),
    NodeMessageRes(NodeMessageRes),
}

pub struct ServerManager {
    pub http_servers: Vec<HttpServer>,
}

impl ServerManager {
    pub fn new(binds: Vec<String>) -> Self {
        Self { http_servers: vec![] }
    }

    pub async fn next_event(&mut self) -> Option<ServerManagerEvent> {
        None
    }
}
