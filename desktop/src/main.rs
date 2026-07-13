use log::Level;
use puppydrive_daemon::App as DaemonApp;
use tauri::{WebviewUrl, WebviewWindowBuilder};

const DEFAULT_DAEMON_ADDR: &str = "127.0.0.1:5777";

fn main() {
    simple_logger::init_with_level(Level::Info).expect("failed to initialize logger");

    tauri::Builder::default()
        .setup(|app| {
            std::thread::Builder::new()
                .name("puppydrive-daemon".into())
                .spawn(|| {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to create daemon runtime");

                    runtime.block_on(async {
                        let mut daemon = DaemonApp::new();
                        daemon.run().await;
                    });
                })?;

            let daemon_addr =
                std::env::var("BIND_ADDR").unwrap_or_else(|_| DEFAULT_DAEMON_ADDR.into());
            let daemon_url = format!("http://{daemon_addr}")
                .parse()
                .expect("daemon URL must be a valid URL");
            WebviewWindowBuilder::new(app, "main", WebviewUrl::External(daemon_url))
                .title("PuppyDrive")
                .inner_size(1440.0, 900.0)
                .min_inner_size(960.0, 640.0)
                .build()?;

            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running PuppyDrive desktop app");
}
