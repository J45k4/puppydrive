use std::sync::Arc;

use axum::extract::State;
use axum::routing::get;
use axum::Json;
use axum::Router;
use clap::Parser;
use db::open_db;
use db::run_migrations;
use rusqlite::Connection;
use serde_json::json;
use serde_json::Value;
use tokio::sync::Mutex;

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
		.route("/api/v1/files", get(search_files)).with_state(ctx.clone());

	let listener = tokio::net::TcpListener::bind("0.0.0.0:5225").await.unwrap();
	axum::serve(listener, app).await.unwrap();
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
	let _db = ctx.db.lock().await;
	Json(json!([]))
}