use std::sync::Arc;

use axum::body::Body;
use axum::extract::Path;
use axum::extract::State;
use axum::http::header;
use axum::http::StatusCode;
use axum::routing::get;
use axum::Json;
use axum::Router;
use clap::Parser;
use db::get_file_entry;
use db::get_file_location;
use db::list_files;
use db::open_db;
use db::run_migrations;
use db::ListArgs;
use http_body_util::StreamBody;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use tokio::sync::Mutex;
use base64::{engine::general_purpose::URL_SAFE, Engine as _};
use axum::response::{IntoResponse, Response};
use tokio_util::io::ReaderStream;

mod args;
mod types;
mod protocol;
mod timer;
mod scan;
mod db;

pub struct Context {
	pub db: Mutex<Connection>
}

#[tokio::main]
async fn main() {
	simple_logger::init_with_level(log::Level::Info).unwrap();
	let args = args::Args::parse();

	#[cfg(feature = "rayon")]
	log::info!("rayon enabled");
	#[cfg(feature = "ring")]
	log::info!("ring enabled");

	run_migrations().unwrap();

	if let Some(command) = args.command {
		match command {
			args::Command::Copy { src, dest } => {
				log::info!("copying {} to {}", src, dest);
			}
			args::Command::Scan { path } => {
				log::info!("scanning {}", path);
				let conn = open_db();
				let res = scan::scan(123456, &path, conn).unwrap();
				log::info!("inserted {} files, updated {} and removed {} files in {:?}", res.inserted_count, res.updated_count, res.removed_count, res.duration);
			}
		}
		return;
	}

	let ctx = Context {
		db: Mutex::new(open_db())
	};
	let ctx = Arc::new(ctx);

	let app = Router::new()
		.route("/api/v1/mime_types", get(get_mime_types)).with_state(ctx.clone())
		.route("/api/v1/files", get(search_files)).with_state(ctx.clone())
		.route("/api/v1/file/{hash}/data", get(get_file_data)).with_state(ctx.clone());

	let listener = tokio::net::TcpListener::bind("0.0.0.0:5225").await.unwrap();
	log::info!("Listening on {}", listener.local_addr().unwrap());
	axum::serve(listener, app).await.unwrap();
}

async fn get_file_data(State(ctx): State<Arc<Context>>, Path(hash): Path<String>) -> impl IntoResponse {
    let db = ctx.db.lock().await;
    let hash_bytes = URL_SAFE.decode(hash.as_bytes()).unwrap();

    // Retrieve file entry and location
    let file_entry = get_file_entry(&db, &hash_bytes).unwrap().unwrap();
    let file_location = get_file_location(&db, &123456u128.to_le_bytes(), &hash_bytes).unwrap().unwrap();
    let path = file_location.path;

    // Stream the file if it exists
    match tokio::fs::File::open(&path).await {
        Ok(file) => {
            let content_type = file_entry.mime_type.unwrap_or("".to_string());
            let stream = ReaderStream::new(file);
            // let body = StreamBody::new(stream);
			let body = Body::from_stream(stream);
            Response::builder()
                .header(header::CONTENT_TYPE, content_type)
                .body(body)
                .unwrap()
                .into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "File not found").into_response(),
    }
}

async fn get_mime_types(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let db = ctx.db.lock().await;
	let mut stmt = db.prepare("SELECT DISTINCT mime_type FROM file_entries WHERE mime_type IS NOT NULL").unwrap();
	let rows = stmt.query_map((), |row| row.get::<_, String>(0)).unwrap();

	let mut mime_types = Vec::new();
	for mime_type in rows {
		mime_types.push(mime_type.unwrap());
	}
	Json(json!(mime_types))
}

async fn search_files(State(ctx): State<Arc<Context>>) -> Json<Value> {
	let conn = ctx.db.lock().await;
	let files = db::list_files(&conn, ListArgs::default()).unwrap(); 
	let res = files.iter().map(|file| {
		let hash = URL_SAFE.encode(&file.hash);
		let size = file.size;
		let mime_type = file.mime_type.clone();
		json!({
			"hash": hash,
			"size": size,
			"mime_type": mime_type,
			"first_datetime": file.first_datetime,
			"latest_datetime": file.latest_datetime,
		})
	}).collect::<Vec<_>>();
	Json(json!(res))
}