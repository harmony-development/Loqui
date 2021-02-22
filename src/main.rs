use client::content::ContentStore;
use iced::{Application, Settings};
use simplelog::*;
use ui::screen::ScreenManager;

pub mod client;
pub mod ui;

#[tokio::main]
async fn main() {
    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    let config = ConfigBuilder::new()
        .set_target_level(LevelFilter::Error)
        .set_location_level(LevelFilter::Error)
        .add_filter_ignore_str("wgpu_core")
        .add_filter_ignore_str("wgpu")
        .add_filter_ignore_str("iced_wgpu")
        .add_filter_ignore_str("tracing")
        .build();

    let filter_level = std::env::args()
        .nth(1)
        .map_or(LevelFilter::Info, |s| match s.as_str() {
            "-v" | "--verbose" => LevelFilter::Trace,
            "-d" | "--debug" => LevelFilter::Debug,
            _ => LevelFilter::Info,
        });

    CombinedLogger::init(vec![
        TermLogger::new(filter_level, config.clone(), TerminalMode::Mixed),
        WriteLogger::new(
            filter_level,
            config,
            std::fs::File::create(content_store.log_file()).unwrap(),
        ),
    ])
    .unwrap();

    let mut settings = Settings::with_flags(content_store);
    settings.window.size = (1280, 720);
    settings.antialiasing = true;
    settings.default_font = Some(include_bytes!("NotoSans-Regular.ttf"));

    ScreenManager::run(settings).unwrap();
}
