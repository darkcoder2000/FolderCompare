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
mod text_compare;
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

/// The window icon (set in Step 7).
pub fn app_icon() -> Option<std::sync::Arc<egui::IconData>> {
    None
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

    let mut main_viewport = viewport(&settings);
    if let Some(icon) = app_icon() {
        main_viewport = main_viewport.with_icon(icon);
    }
    let native = eframe::NativeOptions {
        viewport: main_viewport,
        persist_window: false,
        ..Default::default()
    };
    // Two existing files: only the text compare window (e.g. Explorer "Compare Files"); exits when it closes.
    let two_files = match (&options.left, &options.right) {
        (Some(l), Some(r)) => {
            let (l, r) = (std::path::Path::new(l), std::path::Path::new(r));
            (fc_core::shell_layout::classify(l, r) == fc_core::shell_layout::PairKind::BothFiles)
                .then(|| {
                    (
                        std::path::absolute(l).unwrap_or(l.into()),
                        std::path::absolute(r).unwrap_or(r.into()),
                    )
                })
        }
        _ => None,
    };
    let result = if let Some((left, right)) = two_files {
        info!(
            "Comparing files {} and {} from the command line",
            left.display(),
            right.display()
        );
        let name = |p: &std::path::Path| {
            p.file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default()
        };
        let title = if fc_core::compare::PathComparison::IgnoreCase.eq(&name(&left), &name(&right))
        {
            name(&right)
        } else {
            format!("{} / {}", name(&left), name(&right))
        };
        let request = text_compare::TextCompareRequest {
            left,
            left_exists: true,
            right,
            right_exists: true,
            title,
            relative_path: None,
        };
        let mut viewport = egui::ViewportBuilder::default()
            .with_title(format!("{} - Text compare", request.title))
            .with_inner_size([1200.0, 800.0])
            .with_min_inner_size([600.0, 300.0]);
        if let Some(w) = &settings.text_compare.window
            && w.width >= 300.0
            && w.height >= 200.0
        {
            viewport = viewport
                .with_inner_size([w.width as f32, w.height as f32])
                .with_position([w.left as f32, w.top as f32])
                .with_maximized(w.maximized);
        }
        if let Some(icon) = app_icon() {
            viewport = viewport.with_icon(icon);
        }
        let native = eframe::NativeOptions {
            viewport,
            persist_window: false,
            ..Default::default()
        };
        eframe::run_native(
            "FolderCompare",
            native,
            Box::new(
                move |cc| match text_compare::TextOnlyApp::new(cc, request, settings) {
                    Ok(app) => Ok(Box::new(app) as Box<dyn eframe::App>),
                    Err(e) => Err(e.into()),
                },
            ),
        )
    } else {
        eframe::run_native(
            "FolderCompare",
            native,
            Box::new(move |cc| Ok(Box::new(app::FolderCompareApp::new(cc, options, settings)))),
        )
    };
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
