use tokio_tungstenite::connect_async;
use tokio::sync::mpsc;
use crate::types::*;
use crate::ws::WsWorker;

#[derive(Debug)]
pub struct Peer {
    pub name: String,
    pub ip: String,
    pub tx: mpsc::UnboundedSender<PeerReq>,
    pub rx: mpsc::UnboundedReceiver<PeerRes>,
}

impl Peer {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        log::info!("connecting to peer: {}", addr);
        let (ws, _) = connect_async(addr).await?;
        log::info!("connected to peer: {}", addr);
        let (sender_tx, sender_rx) = mpsc::unbounded_channel();
        let (receiver_tx, receiver_rx) = mpsc::unbounded_channel();
        let mut worker = WsWorker::new(ws);
        tokio::spawn(async move {
            while let Some(msg) = worker.recv().await {

            }
        });
        Ok(Self {
            name: "jyrki".to_string(),
            ip: addr.to_string(),
            tx: sender_tx,
            rx: receiver_rx,
        })
    }

    pub fn send(&self, req: PeerReq) {
        self.tx.send(req).unwrap();
    }
}