mod gui;

use std::error::Error;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    soundboard::utils::logging::init();
    gui::run().await
}
