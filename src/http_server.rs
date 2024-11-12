use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Request;
use hyper::Response;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

use crate::peer::Peer;
use crate::server_manager::ServerCmd;
use crate::server_manager::ServerManagerEvent;
use crate::ws::WsWorker;

static CLIENT_ID: AtomicU64 = AtomicU64::new(1);

const INDEX_HTML: &str = "<html><body><h1>PuppyDrive</h1></body></html>";

pub enum HttpServerEvent {
    PeerConnected(Peer),
}

struct Ctx {
    tx: mpsc::UnboundedSender<ServerManagerEvent>,
}

async fn handle_req(mut req: Request<hyper::body::Incoming>, ctx: Ctx) -> Result<Response<Full<Bytes>>, hyper::Error> {
    log::info!("{} {}", req.method(), req.uri().path());

    if hyper_tungstenite::is_upgrade_request(&req) {
        let id = CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as usize;
        log::info!("[{}] new websocket connection", id);
        let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None).unwrap();
        log::info!("[{}] websocket upgrade complete", id);
        let (sender_tx, sender_rx) = mpsc::unbounded_channel();
        let (receiver_tx, receiver_rx) = mpsc::unbounded_channel();
        tokio::spawn(async move {
            let ws = websocket.await.unwrap();
            log::info!("[{}] websocket connection established", id);
            let mut worker = WsWorker::new(ws);
            while let Some(res) = worker.recv().await {
                
            }
        });
        let peer = Peer {
            name: "MIKKO".to_string(),
            ip: "qwert".to_string(),
            tx: sender_tx,
            rx: receiver_rx,
        };
        ctx.tx.send(ServerManagerEvent::PeerConnected(peer)).unwrap();
        return Ok(response);
    }

    match req.uri().path() {
        _ => Ok(Response::new(Full::new(Bytes::from(INDEX_HTML))))
    }
}

pub struct HttpServer {
    listener: TcpListener,
    tx: mpsc::UnboundedSender<ServerManagerEvent>,
}

impl HttpServer {
    pub async fn new(addr: SocketAddr, tx: mpsc::UnboundedSender<ServerManagerEvent>) -> Self {
        let listener = TcpListener::bind(addr).await.unwrap();
		log::info!("listening on {}", addr);

        Self {
            listener,
            tx,
        }
    }

    pub async fn run(self) {
        loop {
            log::info!("loop");
            tokio::select! {
                res = self.listener.accept() => {
                    match res {
                        Ok((socket, addr)) => {
                            log::info!("accepted connection from {}", addr);
                            let io = TokioIo::new(socket);
                            let tx = self.tx.clone();
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    let tx = tx.clone();
                                    handle_req(req, Ctx { tx })
                                });

                                if let Err(err) = http1::Builder::new()
                                    .serve_connection(io, service)
                                    .with_upgrades()
                                    .await {

                                    log::error!("server error: {:?}", err);
                                }
                            });
                        },
                        Err(err) => {
                            log::error!("accept error: {:?}", err);
                        }
                    }
                } 
            }
        }
    }
}