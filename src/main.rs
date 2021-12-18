#![windows_subsystem = "windows"]
#![forbid(unsafe_code)]

#[cfg(not(target_arch = "wasm32"))]
use client::content::ContentStore;
#[cfg(not(target_arch = "wasm32"))]
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

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

    let app = loqui::App::new();
    let native_options = eframe::NativeOptions::default();
    eframe::run_native(Box::new(app), native_options);
}

#[cfg(target_arch = "wasm32")]
use eframe::wasm_bindgen::{self, prelude::*};

/// This is the entry-point for all the web-assembly.
/// This is called once from the HTML.
/// It loads the app, installs some callbacks, then returns.
/// You can add more callbacks like this if you want to call in to your code.
#[cfg(target_arch = "wasm32")]
fn main() -> Result<(), eframe::wasm_bindgen::JsValue> {
    let app = loqui::App::new();
    eframe::start_web("egui_canvas", Box::new(app))
}
