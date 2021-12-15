#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

use client::content::ContentStore;

use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub use client;

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let rt = tokio::runtime::Runtime::new().expect("failed to create tokio runtime");
    let _guard = rt.enter();

    // Create the content store
    let content_store = ContentStore::default();
    content_store.create_req_dirs().unwrap();

    let log_file = content_store.log_file();

    let term_logger = fmt::layer();
    let file_appender = tracing_appender::rolling::never(log_file.parent().unwrap(), log_file.file_name().unwrap());
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    let file_logger = fmt::layer().with_ansi(false).with_writer(non_blocking);

    tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::from("info")))
        .with(term_logger)
        .with(file_logger)
        .init();

    let app = loqui::App::new(content_store);
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app), native_options);
}
