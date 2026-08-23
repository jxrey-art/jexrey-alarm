// Suppress the console window that would otherwise pop up behind the GUI
// on Windows. Has no effect on other platforms.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod alarm;
mod app;
mod audio;
mod config;
mod event_log;
mod radar;
mod scheduler;
mod system_info;
mod theme;
mod widgets;

use app::JexreyApp;

fn main() -> eframe::Result<()> {
    let viewport = egui::ViewportBuilder::default()
        .with_title("JEXREY ALARM CONTROL SYSTEM")
        .with_inner_size([1280.0, 720.0])
        .with_min_inner_size([960.0, 600.0])
        .with_resizable(true);

    let native_options = eframe::NativeOptions {
        viewport,
        vsync: true,
        ..Default::default()
    };

    eframe::run_native(
        "JEXREY ALARM CONTROL SYSTEM",
        native_options,
        Box::new(|cc| Box::new(JexreyApp::new(cc))),
    )
}
