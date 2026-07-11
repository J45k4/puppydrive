use crate::app::App;
use log::Level;

mod app;

#[tokio::main]
async fn main() {
    simple_logger::init_with_level(Level::Info).expect("failed to initialize logger");

    let mut app = App::new();
    app.run().await;
}
