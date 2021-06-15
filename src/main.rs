#![windows_subsystem = "windows"]

use client::content::ContentStore;
use screen::ScreenManager;
use style::DEF_SIZE;

use iced::{Application, Settings};
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub use client;
pub mod component;
pub mod screen;
pub mod style;

fn main() {
    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    let term_logger = fmt::layer();
    let log_file = content_store.log_file();
    let file_appender = tracing_appender::rolling::never(log_file.parent().unwrap(), log_file.file_name().unwrap());
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let file_logger = fmt::layer().with_ansi(false).with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::from("info"))
                .add_directive("wgpu_core=error".parse().unwrap())
                .add_directive("iced_wgpu=error".parse().unwrap())
                .add_directive("gfx_memory=error".parse().unwrap())
                .add_directive("gfx_descriptor".parse().unwrap())
                .add_directive("gfx_backend_vulkan=error".parse().unwrap()),
        )
        .with(term_logger)
        .with(file_logger)
        .init();

    let mut settings = Settings::with_flags(content_store);
    settings.window.size = (1280, 720);
    settings.antialiasing = false;
    settings.default_font = Some(include_bytes!("NotoSans-Regular.ttf"));
    settings.default_text_size = DEF_SIZE;

    ScreenManager::run(settings).unwrap();
}
