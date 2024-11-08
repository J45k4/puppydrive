use tokio_tungstenite::{connect_async, WebSocketStream, MaybeTlsStream};
use tokio::net::TcpStream;

use crate::{types::PeerReq, ws::WsWorker};

struct WsClientWorker {
    ws: WebSocketStream<MaybeTlsStream<TcpStream>>
}

impl WsClientWorker {
    pub fn from_ws(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self { ws }
    }

    pub async fn run(mut self) {
        // TODO: Implement websocket message handling
    }
}

pub struct Peer {
    
}

impl Peer {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let (ws, _) = connect_async(addr).await?;
        let worker = WsWorker::new(ws);
        tokio::spawn(async move {
            worker.run().await;
        });
        Ok(Self {})
    }

    pub fn send(&self, req: PeerReq) {

    }
}