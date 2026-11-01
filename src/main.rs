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
mod hardware_profile;
mod licensing;
mod reg;
mod config;
mod tray;

use app::OptimizerApp;
use eframe::NativeOptions;
use windows_sys::Win32::Foundation::RECT;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetSystemMetrics, SetForegroundWindow, ShowWindow, SystemParametersInfoW,
    SM_CXSCREEN, SM_CYSCREEN, SPI_GETWORKAREA, SW_RESTORE, SW_SHOW,
};

fn main() -> eframe::Result {
    // Single instance: if already running, restore existing window and exit!
    unsafe {
        let win_title: Vec<u16> = "OmniTune - Trợ Lý Tối Ưu Hệ Thống"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        let existing_hwnd = FindWindowW(std::ptr::null(), win_title.as_ptr());
        if !existing_hwnd.is_null() {
            ShowWindow(existing_hwnd, SW_SHOW);
            ShowWindow(existing_hwnd, SW_RESTORE);
            SetForegroundWindow(existing_hwnd);
            return Ok(());
        }
    }

    // Start system tray background service
    tray::start_tray_service();

    let args: Vec<String> = std::env::args().collect();
    let start_minimized = args.iter().any(|a| a == "--minimized" || a == "--tray");
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

        // Smart Autonomous Widget size: 750x670 px (Comfortable dashboard layout)
        let w = 750.0f32.min(screen_w - 20.0);
        let h = 670.0f32.min(screen_h - 40.0).max(540.0);
        let x = ((screen_w - w) / 2.0).max(0.0);
        let y = ((screen_h - h) / 2.0).max(0.0);
        (x, y, w, h)
    };

    let options = NativeOptions {
        renderer: eframe::Renderer::Glow,
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([win_w, win_h])
            .with_min_inner_size([640.0, 540.0])
            .with_position([pos_x, pos_y])
            .with_visible(!start_minimized)
            .with_title("OmniTune - Trợ Lý Tối Ưu Hệ Thống"),
        ..Default::default()
    };

    eframe::run_native(
        "OmniTune Optimizer",
        options,
        Box::new(|cc| Ok(Box::new(OptimizerApp::new(cc)))),
    )
}
