use clap::Parser;
use db::open_db;
use db::run_migrations;

mod args;
mod types;
mod protocol;
mod timer;
mod scan;
mod db;

#[tokio::main]
async fn main() {
	simple_logger::init_with_level(log::Level::Info).unwrap();
	let args = args::Args::parse();

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
}
