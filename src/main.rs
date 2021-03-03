use client::content::ContentStore;
use iced::{Application, Settings};
use ui::screen::ScreenManager;

pub mod client;
pub mod ui;

#[tokio::main]
async fn main() {
    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    tracing_subscriber::fmt()
        .with_env_filter("info,wgpu_core=off,iced_wgpu=off")
        .pretty()
        .init();

    let mut settings = Settings::with_flags(content_store);
    settings.window.size = (1280, 720);
    settings.antialiasing = true;
    settings.default_font = Some(include_bytes!("NotoSans-Regular.ttf"));

    ScreenManager::run(settings).unwrap();
}
