use clap::Parser;
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
				//scan::scan(&path);
			}
		}
		return;
	}
}
