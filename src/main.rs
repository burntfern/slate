#![cfg_attr(windows, windows_subsystem = "windows")]

#[cfg(windows)]
mod config;
mod core;
#[cfg(windows)]
mod logging;
#[cfg(windows)]
mod win;

#[cfg(windows)]
mod controller;
#[cfg(windows)]
mod ui;

#[cfg(windows)]
use ely_gpui_component::Assets;
#[cfg(windows)]
use gpui::{px, size, AppContext, Bounds, TitlebarOptions, WindowBounds, WindowOptions};

#[cfg(windows)]
fn main() {
    crate::config::paths::ensure_dirs();
    logging::init();
    let args: Vec<String> = std::env::args().collect();
    if let Some(i) = args.iter().position(|a| a == "--janitor") {
        let path = args.get(i + 1).cloned().unwrap_or_default();
        let pid = args.get(i + 2).and_then(|s| s.parse().ok()).unwrap_or(0);
        win::janitor::run(std::path::PathBuf::from(path), pid);
        return;
    }
    gpui_platform::application()
        .with_assets(Assets)
        .run(|cx: &mut gpui::App| {
            if let Err(err) = ely_gpui_component::init(cx) {
                eprintln!("Slate: Ely failed to start: {err:?}");
                std::process::exit(1);
            }
            let controller = cx.new(controller::Controller::new);

            let bounds = Bounds::centered(None, size(px(1100.0), px(720.0)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Slate".into()),
                    appears_transparent: true,
                    traffic_light_position: None,
                }),
                window_min_size: Some(size(px(640.0), px(420.0))),
                ..Default::default()
            };
            if let Err(err) = cx.open_window(options, move |_window, cx| {
                cx.new(|cx| ui::HostWindow::new(controller, cx))
            }) {
                eprintln!("Slate: host window failed to open: {err:?}");
                std::process::exit(1);
            }

            cx.activate(true);
        });
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Slate is Windows-only.");
}
