use futures_util::StreamExt;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use futures_util::SinkExt;

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

    pub async fn run(mut self) {
        let mut ws_stream = self.ws;
        while let Some(msg) = ws_stream.next().await {
            match msg {
                Ok(msg) => {
                    match msg {
                        Message::Text(text) => {
                            log::debug!("received text message: {}", text);
                        },
                        Message::Binary(data) => {
                            log::debug!("received binary message: {} bytes", data.len());
                        },
                        Message::Ping(_) => {
                            // Send pong response
                            if let Err(e) = ws_stream.send(Message::Pong(vec![])).await {
                                log::error!("error sending pong: {}", e);
                                break;
                            }
                        },
                        Message::Close(_) => {
                            log::debug!("received close message");
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
