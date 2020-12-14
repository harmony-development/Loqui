use client::content::ContentStore;
use iced::{Application, Settings};
use simplelog::*;
use ui::screen::ScreenManager;

pub mod client;
pub mod ui;

pub fn main() {
    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    let mut config = ConfigBuilder::new();

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Error, config.build(), TerminalMode::Mixed),
        WriteLogger::new(
            LevelFilter::Error,
            config
                .set_target_level(LevelFilter::Error)
                .set_location_level(LevelFilter::Error)
                .build(),
            std::fs::File::create(content_store.log_file()).unwrap(),
        ),
    ])
    .unwrap();

    let mut settings = Settings::with_flags(content_store);
    settings.window.size = (1280, 720);

    ScreenManager::run(settings).unwrap();
}
