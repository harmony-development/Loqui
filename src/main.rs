use client::content::ContentStore;
use iced::{Application, Settings};
use tracing::Level;
use ui::screen::ScreenManager;

pub mod client;
pub mod ui;

#[tokio::main]
async fn main() {
    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    let filter_level = std::env::args()
        .nth(1)
        .map_or(Level::INFO, |s| match s.as_str() {
            "-v" | "--verbose" => Level::TRACE,
            "-d" | "--debug" => Level::DEBUG,
            _ => Level::INFO,
        });

    tracing_subscriber::fmt()
        .with_max_level(filter_level)
        .pretty()
        .init();

    let mut settings = Settings::with_flags(content_store);
    settings.window.size = (1280, 720);
    settings.antialiasing = true;
    settings.default_font = Some(include_bytes!("NotoSans-Regular.ttf"));

    ScreenManager::run(settings).unwrap();
}
