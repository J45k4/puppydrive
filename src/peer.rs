use tokio_tungstenite::connect_async;
use tokio::sync::mpsc;
use crate::types::*;
use crate::ws::WsWorker;

#[derive(Debug)]
pub struct Peer {
    pub tx: mpsc::UnboundedSender<PeerReq>,
    pub rx: mpsc::UnboundedReceiver<PeerRes>,
}

impl Peer {
    pub async fn connect(addr: &str) -> anyhow::Result<Self> {
        let (ws, _) = connect_async(addr).await?;
        let (sender_tx, sender_rx) = mpsc::unbounded_channel();
        let (receiver_tx, receiver_rx) = mpsc::unbounded_channel();
        let worker = WsWorker::new(ws);
        tokio::spawn(async move {
            worker.run().await;
        });
        Ok(Self {
            tx: sender_tx,
            rx: receiver_rx,
        })
    }

    pub fn send(&self, req: PeerReq) {
        self.tx.send(req).unwrap();
    }
}