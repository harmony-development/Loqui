use iced::{Application, Settings};
use simplelog::*;
use ui::screen::{ScreenManager, StartupFlag};

pub mod client;
pub mod ui;

const LOG_FILE_PATH: &str = concat!(data_dir!(), "log");

pub fn main() {
    // Make sure data dir exists
    std::fs::create_dir_all(data_dir!()).unwrap();

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
        use std::os::unix::fs::PermissionsExt;

        if let Err(err) = std::fs::set_permissions(
            client::SESSION_ID_PATH,
            std::fs::Permissions::from_mode(0o600),
        ) {
            log::error!("Could not set permissions of session file: {}", err);
        }
        Settings::with_flags(StartupFlag::UseSession(session))
    } else {
        Settings::default()
    };
    settings.window.size = (1280, 720);

    ScreenManager::run(settings).unwrap();
}
