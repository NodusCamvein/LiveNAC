mod chat;
mod eventsub;
mod ui;

use eframe::NativeOptions;
use tokio::runtime::Runtime;
use tracing_subscriber::EnvFilter;

fn main() -> eframe::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
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
