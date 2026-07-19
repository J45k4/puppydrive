use log::Level;

use puppydrive_daemon::App;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    simple_logger::init_with_level(Level::Info).expect("failed to initialize logger");

    let mut app = App::new()?;
    app.run().await;
    Ok(())
}
