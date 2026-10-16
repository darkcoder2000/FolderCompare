#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod actions;
mod app;
mod cli;
mod dialogs;
mod main_view;
mod platform;
mod result_list;
mod settings;
mod state;
mod theme;

use std::any::Any;

use tracing::{error, info};

use crate::cli::CommandLineOptions;
use crate::settings::AppSettings;

/// Text of a panic payload.
pub fn panic_message(panic: &Box<dyn Any + Send>) -> String {
    if let Some(s) = panic.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = panic.downcast_ref::<String>() {
        s.clone()
    } else {
        "Unknown error".to_string()
    }
}

/// Rolling daily log files, 14 kept. The guard flushes the log when dropped.
fn init_logging() -> Option<tracing_appender::non_blocking::WorkerGuard> {
    let dir = platform::log_dir();
    let appender = tracing_appender::rolling::Builder::new()
        .rotation(tracing_appender::rolling::Rotation::DAILY)
        .filename_prefix("foldercompare")
        .filename_suffix("log")
        .max_log_files(14)
        .build(&dir)
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .init();
    Some(guard)
}

fn install_panic_hook() {
    let default = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let backtrace = std::backtrace::Backtrace::force_capture();
        error!("Unhandled error: {info}\n{backtrace}");
        default(info);
    }));
}

/// Window placement from the settings: restored if it is big enough and plausibly on screen.
fn viewport(settings: &AppSettings) -> egui::ViewportBuilder {
    let mut vp = egui::ViewportBuilder::default()
        .with_title("FolderCompare")
        .with_inner_size([1280.0, 780.0])
        .with_min_inner_size([700.0, 400.0]);
    if let Some(w) = &settings.window
        && w.width >= 200.0
        && w.height >= 150.0
        && w.left > -w.width + 50.0
        && w.top > -50.0
    {
        vp = vp
            .with_inner_size([w.width as f32, w.height as f32])
            .with_position([w.left as f32, w.top as f32])
            .with_maximized(w.maximized);
    }
    vp
}

fn main() {
    let _guard = init_logging();
    install_panic_hook();
    let args: Vec<String> = std::env::args_os()
        .skip(1)
        .map(|a| a.to_string_lossy().into_owned())
        .collect();
    info!("FolderCompare started with arguments {args:?}");
    let options = CommandLineOptions::parse(&args);
    let settings = settings::load(&settings::settings_path());

    let native = eframe::NativeOptions {
        viewport: viewport(&settings),
        persist_window: false,
        ..Default::default()
    };
    let result = eframe::run_native(
        "FolderCompare",
        native,
        Box::new(move |cc| Ok(Box::new(app::FolderCompareApp::new(cc, options, settings)))),
    );
    if let Err(e) = result {
        error!("Startup failed: {e}");
        rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Error)
            .set_title("FolderCompare")
            .set_description(format!("FolderCompare could not start:\n\n{e}"))
            .show();
    }
    info!("FolderCompare exited");
}
