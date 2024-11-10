use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use futures_util::SinkExt;
use crate::types::PeerMsg;

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
        loop {
            self.ws.send(Message::Text("Hello world".to_string())).await.unwrap();
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
        }
    }
}


pub fn start_ws_worker() {

}