use eframe::NativeOptions;
use livenac::ui::app_layout::App;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> eframe::Result<()> {
    let file_appender = tracing_appender::rolling::never(".", "livenac.log");
    let (non_blocking, _guard) = tracing_appender::non_blocking(file_appender);
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .with_writer(non_blocking)
        .with_ansi(false)
        .with_timer(tracing_subscriber::fmt::time::LocalTime::rfc_3339())
        .init();

    let native_options = NativeOptions::default();
    eframe::run_native(
        "livenac",
        native_options,
        Box::new(|cc| {
            let app = App::new(cc);
            Ok(Box::new(app))
        }),
    )
}
