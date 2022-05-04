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
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::from("info"))
                .add_directive("h2::codec::framed_read=error".parse().unwrap())
                .add_directive("h2::codec::framed_write=error".parse().unwrap()),
        )
        .with(term_logger)
        .with(file_logger)
        .init();

    let icon_data = {
        let icon_raw = include_bytes!("../data/loqui.ico");
        let image = image::load_from_memory(icon_raw).expect("icon must be valid");
        let image = image.to_rgba8();
        eframe::IconData {
            width: image.width(),
            height: image.height(),
            rgba: image.into_vec(),
        }
    };

    let native_options = eframe::NativeOptions {
        initial_window_size: Some([1280.0, 720.0].into()),
        drag_and_drop_support: true,
        icon_data: Some(icon_data),
        ..Default::default()
    };
    eframe::run_native(
        "loqui",
        native_options,
        Box::new(|cc| {
            let mut app = loqui::App::new();
            app.setup(cc);
            Box::new(app)
        }),
    );
}

#[cfg(target_arch = "wasm32")]
fn main() -> Result<(), eframe::wasm_bindgen::JsValue> {
    console_error_panic_hook::set_once();
    tracing_wasm::set_as_global_default();

    eframe::start_web(
        "egui_canvas",
        Box::new(|cc| {
            let mut app = loqui::App::new();
            app.setup(cc);
            Box::new(app)
        }),
    )
}
