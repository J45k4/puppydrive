use futures_util::StreamExt;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task::spawn_local;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use futures_util::SinkExt;
use crate::types::Event;
use crate::types::PeerMsg;
use crate::types::SharedState;

pub struct WsWorker<T> {
    ws: WebSocketStream<T>
}

impl<T> WsWorker<T>
where
    T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin,
{
    pub fn new(ws: WebSocketStream<T>) -> Self {
        Self { ws }
    }

    pub async fn recv(&mut self) -> Option<PeerMsg> {
        while let Some(msg) = self.ws.next().await {
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            log::info!("received text message: {}", text);
                        },
                        Message::Binary(data) => {
                            log::info!("received binary message: {} bytes", data.len());
                        },
						Message::Frame(f) => {}
                        Message::Ping(_) => {
                            log::info!("received ping");
                            // Send pong response
                            if let Err(e) = self.ws.send(Message::Pong(vec![])).await {
                                log::error!("error sending pong: {}", e);
                                break;
                            }
                        },
                        Message::Close(_) => {
                            log::info!("received close message");
                            break;
                        },
                        _ => {}
                    }
                },
                Err(e) => {
                    log::error!("error receiving message: {}", e);
                    break;
                }
            }
        }
        log::info!("ws worker stopped");
        None
    }
}


pub fn start_ws_worker(tx: tokio::sync::mpsc::UnboundedSender<PeerMsg>, rx: mpsc::UnboundedReceiver<PeerMsg>) {
    tokio::spawn(async move {
        // let mut worker = WsWorker::new(ws);
        // while let Some(msg) = worker.recv().await {
        //     tx.send(msg).unwrap();
        // }
    });

}


pub async fn connect(addr: &str) -> anyhow::Result<()> {
	log::info!("connecting to peer: {}", addr);
	let (ws, _) = connect_async(addr).await?;
	log::info!("connected to peer: {}", addr);
	// let (sender_tx, sender_rx) = mpsc::unbounded_channel();
	// let (receiver_tx, receiver_rx) = mpsc::unbounded_channel();
	let mut worker = WsWorker::new(ws);
	todo!()
}

pub async fn start_ws(url: &str, rx: broadcast::Receiver<Event>, state: SharedState) {
	// let (ws, _) = connect_async(url).await.unwrap();
	// let (tx, rx) = mpsc::unbounded_channel();
	// let mut worker = WsWorker::new(ws);
	let url = url.to_string();
	spawn_local(async move {
		// while let Some(msg) = worker.recv().await {
		// 	match msg {
		// 		PeerMsg::NodeMessageReq(req) => {
		// 			log::info!("node message req: {:?}", req);
		// 		},
		// 		PeerMsg::NodeMessageRes(res) => {
		// 			log::info!("node message res: {:?}", res);
		// 		},
		// 		_ => {}
		// 	}
		// }
		
		loop {
			let (ws, _) = connect_async(&url).await.unwrap();

		}
	});
}