//! L desktop application.

mod app;
mod backend;
mod dialogs;
mod discord_web;
mod monocord;
mod keybinds;
mod pages;
mod palette;
mod plugins;
mod rpc;
mod shell;

fn main() -> eframe::Result<()> {
    // `l-app --discord-web` runs the standalone Discord client window
    // (spawned as a child process by the dashboard).
    if std::env::args().any(|a| a == discord_web::WEB_FLAG) {
        discord_web::run_client();
        return Ok(());
    }
    init_logging();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("L")
            .with_inner_size([1280.0, 800.0])
            .with_min_inner_size([900.0, 600.0]),
        ..Default::default()
    };
    eframe::run_native(
        "L",
        options,
        Box::new(|cc| Ok(Box::new(app::LApp::new(cc)))),
    )
}

fn init_logging() {
    use tracing_subscriber::prelude::*;
    let dir = directories::ProjectDirs::from("dev", "l", "l").map_or_else(
        || std::path::PathBuf::from("."),
        |d| d.data_dir().to_path_buf(),
    );
    let _ = std::fs::create_dir_all(&dir);
    let file_appender = tracing_appender::rolling::daily(&dir, "l.log");
    let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);
    // Keep the guard alive for the process lifetime.
    std::mem::forget(guard);
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        .with(tracing_subscriber::fmt::layer().with_writer(non_blocking))
        .init();
}
