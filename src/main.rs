mod gui;

#[tokio::main]
async fn main() -> soundboard::error::Result<()> {
    soundboard::utils::logging::init();
    gui::run().await
}
