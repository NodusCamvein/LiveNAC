mod auth;
mod chat;
mod eventsub;
mod ui;

use eframe::NativeOptions;
use tokio::runtime::Runtime;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    // Setup file-based logging
    let file_appender = tracing_appender::rolling::never(".", "livenac.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(non_blocking)
        .init();

    let runtime = Runtime::new().expect("Failed to create Tokio runtime");

    let native_options = NativeOptions::default();
    eframe::run_native(
        "LiveNAC",
        native_options,
        Box::new(move |cc| {
            let app = ui::LiveNAC::new(cc, runtime.handle().clone());
            Ok(Box::new(app))
        }),
    )
}
