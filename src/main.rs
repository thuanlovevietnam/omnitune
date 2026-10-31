#![windows_subsystem = "windows"]

mod app;
mod battery_info;
mod cleaner;
mod eco_qos;
mod memory;
mod power;
mod protection;
mod scenarios;
mod startup;
mod theme;
mod types;

use app::OptimizerApp;
use eframe::NativeOptions;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SM_CXSCREEN, SM_CYSCREEN,
};

fn main() -> eframe::Result {
    let win_w = 720.0;
    let win_h = 530.0;

    let (pos_x, pos_y) = unsafe {
        let screen_w = GetSystemMetrics(SM_CXSCREEN) as f32;
        let screen_h = GetSystemMetrics(SM_CYSCREEN) as f32;
        let x = ((screen_w - win_w) / 2.0).max(0.0);
        let y = ((screen_h - win_h) / 2.0).max(0.0);
        (x, y)
    };

    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([win_w, win_h])
            .with_min_inner_size([620.0, 460.0])
            .with_position([pos_x, pos_y])
            .with_title("OmniTune - Universal System & Hardware Optimizer"),
        ..Default::default()
    };

    eframe::run_native(
        "OmniTune Optimizer",
        options,
        Box::new(|cc| Ok(Box::new(OptimizerApp::new(cc)))),
    )
}
