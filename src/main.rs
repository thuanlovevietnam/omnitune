#![windows_subsystem = "windows"]

mod app;
mod autonomous_engine;
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
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetSystemMetrics, SystemParametersInfoW, SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA,
};

fn main() -> eframe::Result {
    let (pos_x, pos_y, win_w, win_h) = unsafe {
        let mut work_area: RECT = std::mem::zeroed();
        let ok = SystemParametersInfoW(
            SPI_GETWORKAREA,
            0,
            &mut work_area as *mut _ as *mut _,
            0,
        );

        let (screen_w, screen_h) = if ok != 0 {
            (
                (work_area.right - work_area.left) as f32,
                (work_area.bottom - work_area.top) as f32,
            )
        } else {
            (
                GetSystemMetrics(SM_CXSCREEN) as f32,
                GetSystemMetrics(SM_CYSCREEN) as f32,
            )
        };

        // Compact autonomous assistant widget size: 440x560 px
        let w = 440.0f32.min(screen_w - 20.0);
        let h = 560.0f32.min(screen_h - 20.0).max(480.0);
        let x = ((screen_w - w) / 2.0).max(0.0);
        let y = ((screen_h - h) / 2.0).max(0.0);
        (x, y, w, h)
    };

    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([win_w, win_h])
            .with_min_inner_size([400.0, 480.0])
            .with_position([pos_x, pos_y])
            .with_title("OmniTune - Autonomous System Assistant"),
        ..Default::default()
    };

    eframe::run_native(
        "OmniTune Optimizer",
        options,
        Box::new(|cc| Ok(Box::new(OptimizerApp::new(cc)))),
    )
}
