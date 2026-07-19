use log::Level;
use puppydrive_daemon::App as DaemonApp;
use tauri::{WebviewUrl, WebviewWindowBuilder};

fn main() {
    simple_logger::init_with_level(Level::Info).expect("failed to initialize logger");

    tauri::Builder::default()
        .setup(|app| {
            let (address_sender, address_receiver) = std::sync::mpsc::sync_channel(1);
            std::thread::Builder::new()
                .name("puppydrive-daemon".into())
                .spawn(move || {
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to create daemon runtime");

                    runtime.block_on(async {
                        match DaemonApp::new() {
                            Ok(mut daemon) => {
                                let _ = address_sender.send(Ok(daemon.bind_address()));
                                daemon.run().await;
                            }
                            Err(error) => {
                                let _ = address_sender.send(Err(format!("{error:#}")));
                            }
                        }
                    });
                })?;

            let daemon_addr = address_receiver
                .recv()
                .map_err(|error| std::io::Error::other(format!("daemon failed to start: {error}")))?
                .map_err(std::io::Error::other)?;
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
