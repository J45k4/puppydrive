use std::net::SocketAddr;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::server::conn::http1;
use hyper::service::service_fn;
use hyper::Request;
use hyper::Response;
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;
use tokio::sync::mpsc;

static CLIENT_ID: AtomicU64 = AtomicU64::new(1);

const INDEX_HTML: &str = "<html><body><h1>PuppyDrive</h1></body></html>";

struct Ctx {
}

async fn handle_req(mut req: Request<hyper::body::Incoming>, ctx: Ctx) -> Result<Response<Full<Bytes>>, hyper::Error> {
    log::info!("{} {}", req.method(), req.uri().path());

    if req.uri().path() == "/ws" && hyper_tungstenite::is_upgrade_request(&req) {
        let (response, websocket) = hyper_tungstenite::upgrade(&mut req, None).unwrap();
        let id = CLIENT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed) as usize;
        log::debug!("websocket worker created {}", id);
        tokio::spawn(async move {

        });
        return Ok(response);
    }

    match req.uri().path() {
        _ => Ok(Response::new(Full::new(Bytes::from(INDEX_HTML))))
    }
}

pub struct HttpServer {
    listener: TcpListener
}

impl HttpServer {
    pub async fn new(addr: SocketAddr) -> Self {
        let listener = TcpListener::bind(addr).await.unwrap();
		log::info!("listening on {}", addr);

        Self {
            listener
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                res = self.listener.accept() => {
                    match res {
                        Ok((socket, addr)) => {
                            log::info!("accepted connection from {}", addr);
                            let io = TokioIo::new(socket);
                            tokio::spawn(async move {
                                let service = service_fn(move |req| {
                                    handle_req(req, Ctx {})
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