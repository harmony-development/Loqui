use iced::{Application, Settings};
use simplelog::*;
use ui::screen::{ScreenManager, StartupFlag};

pub mod client;
pub mod ui;

const LOG_FILE_PATH: &str = concat!(data_dir!(), "log");

pub fn main() {
    let mut config = ConfigBuilder::new();

    CombinedLogger::init(vec![
        TermLogger::new(LevelFilter::Error, config.build(), TerminalMode::Mixed),
        WriteLogger::new(
            LevelFilter::Error,
            config
                .set_target_level(LevelFilter::Error)
                .set_location_level(LevelFilter::Error)
                .build(),
            std::fs::File::create(LOG_FILE_PATH).unwrap(),
        ),
    ])
    .unwrap();

    let mut settings = if let Ok(Ok(session)) =
        std::fs::read_to_string(client::SESSION_ID_PATH).map(|s| toml::from_str(&s))
    {
        Settings::with_flags(Some(StartupFlag::UseSession(session)))
    } else {
        Settings::default()
    };
    settings.window.size = (1280, 720);

    ScreenManager::run(settings).unwrap();
}
