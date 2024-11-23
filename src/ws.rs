use futures_util::StreamExt;
use tokio::net::TcpListener;
use tokio::sync::broadcast;
use tokio::sync::mpsc;
use tokio::task::spawn_local;
use tokio_tungstenite::accept_async;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::WebSocketStream;
use futures_util::SinkExt;
use crate::pupynet::PupynetEvent;
use crate::types::Event;
use crate::types::PeerConnCmd;
use crate::types::PeerMsg;
use crate::types::SharedState;

async fn handle_ws<T>(mut ws: WebSocketStream<T>, addr: String, event_tx: mpsc::UnboundedSender<PupynetEvent>, mut peer_rx: mpsc::UnboundedReceiver<PeerConnCmd>)
where 
	T: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin
{
	loop {
		tokio::select! {
			msg = ws.next() => {
				match msg {
					Some(msg) => {
						match msg {
							Ok(msg) => {
								match msg {
									Message::Text(text) => log::info!("received text message: {}", text),
									Message::Binary(data) => {
										log::info!("received binary message: {:?}", data);
										match event_tx.send(PupynetEvent::PeerData { addr: addr.clone(), data }) {
											Ok(_) => {},
											Err(err) => {
												log::error!("error sending event: {}", err);
												break;
											}
										}
									},
									Message::Ping(vec) => { log::info!("received ping"); },
									Message::Pong(vec) => { log::info!("received pong"); },
									Message::Close(close_frame) => {
										log::info!("received close message: {:?}", close_frame);
										break;
									},
									Message::Frame(frame) => {},
								}
							},
							Err(err) => {
								log::error!("error receiving message: {}", err);
								break;
							},
						}
					}
					None => {
						log::error!("connection closed");
					}
				}
			}
			cmd = peer_rx.recv() => {
				match cmd {
					Some(cmd) => {
						match cmd {
							PeerConnCmd::Send(msg) => {
								log::info!("sending message: {:?}", msg);
								match ws.send(Message::Binary(msg)).await {
									Ok(_) => {},
									Err(err) => {
										log::error!("error sending message: {}", err);
										break;
									}
								}
							}
							PeerConnCmd::Close => {
								log::info!("closing connection");
								break;
							}
						}
					},
					None => {
						log::error!("peer conn rx closed");
						break;
					}
				}
			}
		}
	}

	if let Err(err) = event_tx.send(PupynetEvent::PeerDisconnected { addr }) {
		log::error!("error sending event: {}", err);
	}
}

pub async fn connect(addr: String, event_tx: mpsc::UnboundedSender<PupynetEvent>, peer_rx: mpsc::UnboundedReceiver<PeerConnCmd>) -> anyhow::Result<()> {
	log::info!("connecting to peer: {}", addr);
	let (ws, _) = connect_async(&addr).await?;
	log::info!("connected to peer: {}", addr);
	handle_ws(ws, addr, event_tx, peer_rx).await;
	Ok(())
}

pub async fn bind(addr: String, event_tx: mpsc::UnboundedSender<PupynetEvent>) -> anyhow::Result<()> {
	log::info!("binding to peer: {}", addr);
    let listener = TcpListener::bind(&addr.replace("ws://", "")).await.expect("Failed to bind");
    log::info!("WebSocket server listening on {}", addr);
    while let Ok((stream, _)) = listener.accept().await {
		let addr = stream.peer_addr().expect("Connected streams should have a peer address");
		log::info!("New connection from {}", addr);
		let ws = accept_async(stream).await.expect("Error during the websocket handshake");
		log::info!("WebSocket connection established: {}", addr);
		let (tx, rx) = mpsc::unbounded_channel();
		let event_tx = event_tx.clone();
		let addr = format!("ws://{}", addr);
		if let Err(err) = event_tx.send(PupynetEvent::PeerConnected { addr: addr.clone(), tx: tx }) {
			log::error!("error sending event: {}", err);
			break;
		}
		tokio::spawn(async move {
			handle_ws(ws, addr.to_string(), event_tx, rx).await;
		});
    }
	Ok(())
}