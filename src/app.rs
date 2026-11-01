use crate::autonomous_engine::{AutonomousEngine, OperatingMode};
use crate::battery_info::get_dual_battery_info;
use crate::cleaner::{
    clean_target, format_bytes, get_default_targets, scan_target,
};
use crate::config::{load_config, save_config, set_autostart_registry, AppConfig};
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process};
use crate::licensing::{deactivate_license, load_license, save_license, LicenseData};
use crate::memory::{
    get_memory_stats, get_top_processes, terminate_process, trim_single_process, trim_working_sets,
    MemStats,
};
use crate::power::{
    get_cooling_policy, get_gamedvr,
    get_power_status, set_brightness, set_cooling_policy, set_cpu_max_state_ac,
    set_cpu_max_state_dc, set_gamedvr, set_keep_awake, set_timer_resolution_1ms,
    PowerStatus,
};
use crate::protection::{
    enable_hibernate_as_admin, get_protection_overview, set_battery_threshold, ProtectionOverview,
};
use crate::scenarios::Scenario;
use crate::startup::{get_startup_apps, is_telemetry_active, toggle_startup_item, toggle_telemetry};
use crate::theme::{
    apply_theme, configure_fonts, ThemeMode, ThemePalette,
};
use crate::types::{BatteryDetails, CleanTier, JunkCategory, ProcessMemInfo, StartupAppItem, Tab};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use sysinfo::System;

pub enum BackgroundMessage {
    InitialScan(InitialScanResult),
    BatteryUpdate(Vec<BatteryDetails>),
}

pub struct InitialScanResult {
    pub mem_stats: MemStats,
    pub top_processes: Vec<ProcessMemInfo>,
    pub startup_apps: Vec<StartupAppItem>,
    pub protection: ProtectionOverview,
    pub is_telemetry_running: bool,
    pub gamedvr_enabled: bool,
    pub cpu_cores_info: String,
    pub dual_batteries: Option<Vec<BatteryDetails>>,
    pub cpu_limit_dc: Option<u32>,
    pub cpu_limit_ac: Option<u32>,
    pub cooling_active: Option<bool>,
    pub foreground_info: Option<(u32, String)>,
    pub scan_duration_ms: f64,
}

pub struct OptimizerApp {
    current_tab: Tab,
    autonomous: AutonomousEngine,
    foreground_info: Option<(u32, String)>,

    theme_mode: ThemeMode,
    palette: ThemePalette,
    sys: System,
    mem_stats: MemStats,
    power_status: PowerStatus,
    dual_batteries: Vec<BatteryDetails>,
    top_processes: Vec<ProcessMemInfo>,
    protection: ProtectionOverview,

    // Cleaner state
    clean_categories: Vec<JunkCategory>,
    clean_status_msg: String,

    // Process Inspector state
    process_search_query: String,
    process_action_msg: String,

    // Startup & Telemetry state
    startup_apps: Vec<StartupAppItem>,
    is_telemetry_running: bool,
    startup_action_msg: String,

    // Criteria Tuning parameters
    cpu_limit_dc: u32,
    cpu_limit_ac: u32,
    timer_1ms_enabled: bool,
    keep_awake_mode: u8,
    cooling_active: bool,
    gamedvr_enabled: bool,

    // Power settings
    brightness_level: u32,

    // Licensing state
    license: LicenseData,
    show_license_modal: bool,
    input_license_email: String,
    input_license_key: String,
    license_feedback_msg: String,

    // Dynamic hardware info
    cpu_cores_info: String,

    // General status
    quick_opt_msg: String,
    last_refresh: Instant,
    last_battery_poll: Instant,

    // Background hardware worker
    bg_tx: Sender<BackgroundMessage>,
    bg_rx: Receiver<BackgroundMessage>,
    bg_fetch_in_progress: bool,
    initial_scan_done: bool,

    // Persistence & Tray
    config: AppConfig,
    run_on_startup: bool,
}

impl OptimizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let t0 = Instant::now();
        configure_fonts(&cc.egui_ctx);
        let theme_mode = ThemeMode::Light;
        let palette = apply_theme(&cc.egui_ctx, theme_mode);

        let mut sys = System::new().expect("failed to initialize sysinfo");
        let mem_stats = get_memory_stats();
        let power_status = get_power_status();
        let license = load_license();

        let mut config = load_config();
        let run_on_startup = crate::config::is_autostart_enabled();
        config.run_on_startup = run_on_startup;

        // Apply saved configurations immediately on launch
        if config.timer_1ms_enabled {
            set_timer_resolution_1ms(true);
        }
        if config.keep_awake_mode > 0 {
            set_keep_awake(config.keep_awake_mode);
        }
        if config.cooling_active {
            set_cooling_policy(true);
        }
        if config.gamedvr_enabled {
            let _ = set_gamedvr(true);
        }
        if config.battery_threshold_80 {
            let _ = set_battery_threshold(true, 80);
        }

        // Apply permanent OS responsiveness tweaks
        crate::power::apply_permanent_windows_system_tweaks();

        let cfg_dc = config.cpu_limit_dc;
        let cfg_ac = config.cpu_limit_ac;

        let targets = get_default_targets();
        let clean_categories = targets
            .into_iter()
            .map(|t| JunkCategory {
                name: t.name.to_string(),
                path_desc: t.path_desc.to_string(),
                size_bytes: 0,
                file_count: 0,
                selected: true,
                tier: t.tier,
            })
            .collect();

        let (bg_tx, bg_rx) = channel();
        let has_battery = power_status.has_battery;
        let bg_tx_scan = bg_tx.clone();

        // Background worker: App opens FIRST, heavy scans run in background!
        std::thread::spawn(move || {
            let scan_start = Instant::now();
            let mut bg_sys = System::new().expect("failed to initialize sysinfo");
            bg_sys.refresh_cpu_usage();
            bg_sys.refresh_memory();
            bg_sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

            // Apply CPU limits if configured, or read from powercfg
            let dc = if cfg_dc > 0 {
                let _ = set_cpu_max_state_dc(cfg_dc);
                Some(cfg_dc)
            } else {
                Some(crate::power::get_cpu_max_state_dc())
            };
            let ac = if cfg_ac > 0 {
                let _ = set_cpu_max_state_ac(cfg_ac);
                Some(cfg_ac)
            } else {
                Some(crate::power::get_cpu_max_state_ac())
            };

            let mem = get_memory_stats();
            let procs = get_top_processes(&mut bg_sys, 25);
            let startups = get_startup_apps();
            let prot = get_protection_overview(bg_sys.global_cpu_usage(), startups.len());
            let telem = is_telemetry_active();
            let gdvr = get_gamedvr();
            let fg = get_foreground_process(&bg_sys);
            let cores = format!(
                "{} nhân • {} luồng",
                sysinfo::System::physical_core_count().unwrap_or(2),
                bg_sys.cpus().len()
            );

            let bats = if has_battery {
                Some(get_dual_battery_info())
            } else {
                None
            };
            let cool = Some(get_cooling_policy());

            let dur_ms = scan_start.elapsed().as_secs_f64() * 1000.0;

            let _ = bg_tx_scan.send(BackgroundMessage::InitialScan(InitialScanResult {
                mem_stats: mem,
                top_processes: procs,
                startup_apps: startups,
                protection: prot,
                is_telemetry_running: telem,
                gamedvr_enabled: gdvr,
                cpu_cores_info: cores,
                dual_batteries: bats,
                cpu_limit_dc: dc,
                cpu_limit_ac: ac,
                cooling_active: cool,
                foreground_info: fg,
                scan_duration_ms: dur_ms,
            }));
        });

        let hw = crate::hardware_profile::HardwareProfile::detect(&mut sys);
        let mut autonomous = AutonomousEngine::new(hw, config.operating_mode);
        autonomous.auto_pilot_enabled = config.auto_pilot_enabled;
        autonomous.log(
            "[TỐC ĐỘ]",
            format!("Mở app tức thì trong {:.1} ms. Đang quét hệ thống ngầm...", t0.elapsed().as_secs_f64() * 1000.0),
        );

        Self {
            current_tab: Tab::Widget,
            autonomous,
            foreground_info: None,
            theme_mode,
            palette,
            sys,
            mem_stats,
            power_status,
            dual_batteries: Vec::new(),
            top_processes: Vec::new(),
            protection: ProtectionOverview::default(),
            clean_categories,
            clean_status_msg: "Sẵn sàng quét rác.".to_string(),
            process_search_query: String::new(),
            process_action_msg: String::new(),
            startup_apps: Vec::new(),
            is_telemetry_running: false,
            startup_action_msg: String::new(),
            cpu_limit_dc: config.cpu_limit_dc,
            cpu_limit_ac: config.cpu_limit_ac,
            timer_1ms_enabled: config.timer_1ms_enabled,
            keep_awake_mode: config.keep_awake_mode,
            cooling_active: config.cooling_active,
            gamedvr_enabled: config.gamedvr_enabled,
            brightness_level: 80,
            license,
            show_license_modal: false,
            input_license_email: String::new(),
            input_license_key: String::new(),
            license_feedback_msg: String::new(),
            cpu_cores_info: "Đang nhận diện...".to_string(),
            quick_opt_msg: String::new(),
            last_refresh: Instant::now(),
            last_battery_poll: Instant::now(),
            bg_tx,
            bg_rx,
            bg_fetch_in_progress: true,
            initial_scan_done: false,
            config,
            run_on_startup,
        }
    }

    pub fn sync_config(&mut self) {
        self.config.timer_1ms_enabled = self.timer_1ms_enabled;
        self.config.keep_awake_mode = self.keep_awake_mode;
        self.config.cpu_limit_dc = self.cpu_limit_dc;
        self.config.cpu_limit_ac = self.cpu_limit_ac;
        self.config.cooling_active = self.cooling_active;
        self.config.gamedvr_enabled = self.gamedvr_enabled;
        self.config.auto_pilot_enabled = self.autonomous.auto_pilot_enabled;
        self.config.operating_mode = self.autonomous.operating_mode;
        self.config.battery_threshold_80 = self.protection.battery_threshold_enabled;
        self.config.run_on_startup = self.run_on_startup;
        save_config(&self.config);
    }

    fn calculate_health_score(&self) -> (u32, &'static str, Color32, Vec<String>) {
        let mut score = 100i32;
        let mut tips = Vec::new();

        if self.mem_stats.load_percent >= 85 {
            score -= 20;
            tips.push(format!("RAM cao ({}%), bấm 'Tối ưu' để giải phóng bộ nhớ.", self.mem_stats.load_percent));
        } else if self.mem_stats.load_percent >= 70 {
            score -= 10;
        }

        let total_junk: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();
        if total_junk > 2 * 1024 * 1024 * 1024 {
            score -= 20;
            tips.push(format!("Phát hiện {} rác/cache trên ổ {}, khuyến nghị dọn dẹp.", format_bytes(total_junk), self.autonomous.hardware_profile.disk_name));
        } else if total_junk > 500 * 1024 * 1024 {
            score -= 10;
            tips.push(format!("Tìm thấy {} file rác an toàn có thể giải phóng.", format_bytes(total_junk)));
        }

        if self.is_telemetry_running {
            score -= 10;
            tips.push("Dịch vụ DiagTrack đang chạy nền, khuyến nghị tắt để giảm tải ổ đĩa.".to_string());
        }

        if !self.protection.is_hibernate_enabled {
            score -= 10;
            tips.push("Chưa bật Hibernate, laptop có thể hao pin khi ngủ Sleep S3.".to_string());
        }

        if self.power_status.is_ac_online && !self.protection.battery_threshold_enabled {
            tips.push("Đang cắm sạc: Bật ngưỡng 80% trong phần cài đặt để chống phù pin.".to_string());
        }

        if tips.is_empty() {
            tips.push("Hệ thống và phần cứng đang vận hành ở trạng thái tối ưu lý tưởng.".to_string());
        }

        let final_score = score.clamp(0, 100) as u32;

        let (label, color) = if final_score >= 90 {
            ("HỆ THỐNG ĐANG TỐI ƯU", self.palette.success_green)
        } else if final_score >= 75 {
            ("VẬN HÀNH ỔN ĐỊNH", self.palette.accent_blue)
        } else if final_score >= 60 {
            ("CẦN BẢO DƯỠNG NHẸ", self.palette.warn_amber)
        } else {
            ("CẦN TỐI ƯU HÓA NGAY", self.palette.danger_red)
        };

        (final_score, label, color, tips)
    }

    fn update_battery_metrics(&mut self, bats: Vec<BatteryDetails>) {
        let total_discharge: u32 = bats.iter().map(|b| b.discharge_rate_mw).sum();
        let total_charge: u32 = bats.iter().map(|b| b.charge_rate_mw).sum();
        let avg_voltage: u32 = if !bats.is_empty() {
            bats.iter().map(|b| b.voltage_mv).sum::<u32>() / bats.len() as u32
        } else {
            0
        };
        self.power_status.discharge_rate_mw = total_discharge;
        self.power_status.charge_rate_mw = total_charge;
        self.power_status.power_watts = if total_discharge > 0 {
            total_discharge as f32 / 1000.0
        } else if total_charge > 0 {
            total_charge as f32 / 1000.0
        } else {
            0.0
        };
        self.power_status.voltage_volts = avg_voltage as f32 / 1000.0;
        self.dual_batteries = bats;
    }

    fn refresh_periodic(&mut self) {
        // Drain any asynchronous background hardware query results
        while let Ok(msg) = self.bg_rx.try_recv() {
            match msg {
                BackgroundMessage::InitialScan(res) => {
                    self.initial_scan_done = true;
                    self.mem_stats = res.mem_stats;
                    self.top_processes = res.top_processes;
                    self.startup_apps = res.startup_apps;
                    self.protection = res.protection;
                    if self.config.battery_threshold_80 {
                        self.protection.battery_threshold_enabled = true;
                    }
                    self.is_telemetry_running = res.is_telemetry_running;
                    self.cpu_cores_info = res.cpu_cores_info;
                    self.foreground_info = res.foreground_info;

                    if let Some(bats) = res.dual_batteries {
                        self.update_battery_metrics(bats);
                    }
                    // Keep user's saved configurations, or fall back to detected OS values
                    if let Some(dc) = res.cpu_limit_dc {
                        if self.config.cpu_limit_dc == 0 {
                            self.cpu_limit_dc = dc;
                        }
                    }
                    if let Some(ac) = res.cpu_limit_ac {
                        if self.config.cpu_limit_ac == 0 {
                            self.cpu_limit_ac = ac;
                        }
                    }
                    if let Some(cool) = res.cooling_active {
                        if !self.config.cooling_active {
                            self.cooling_active = cool;
                        }
                    }
                    if !self.config.gamedvr_enabled {
                        self.gamedvr_enabled = res.gamedvr_enabled;
                    }

                    self.autonomous.log(
                        "[QUÉT XONG]",
                        format!("Đã nhận diện toàn bộ phần cứng & tiến trình ({:.0} ms).", res.scan_duration_ms),
                    );
                    self.bg_fetch_in_progress = false;
                }
                BackgroundMessage::BatteryUpdate(bats) => {
                    self.update_battery_metrics(bats);
                    self.bg_fetch_in_progress = false;
                }
            }
        }

        if self.last_refresh.elapsed() >= Duration::from_secs(1) {
            if self.initial_scan_done {
                self.sys.refresh_cpu_usage();
                self.protection.cpu_usage = self.sys.global_cpu_usage();
                self.foreground_info = get_foreground_process(&self.sys);
            }
            self.mem_stats = get_memory_stats();
            self.power_status = get_power_status();

            // Run autonomous engine tick
            self.autonomous.tick(
                &mut self.sys,
                self.power_status.is_ac_online,
                self.power_status.battery_percent,
                self.mem_stats.load_percent,
            );

            // Periodic ACPI battery poll (every 10s) running in background without UI freezing
            if self.power_status.has_battery
                && !self.bg_fetch_in_progress
                && self.initial_scan_done
                && self.last_battery_poll.elapsed() >= Duration::from_secs(10)
            {
                self.bg_fetch_in_progress = true;
                self.last_battery_poll = Instant::now();
                let tx = self.bg_tx.clone();
                std::thread::spawn(move || {
                    let bats = get_dual_battery_info();
                    let _ = tx.send(BackgroundMessage::BatteryUpdate(bats));
                });
            }

            self.last_refresh = Instant::now();
        }
    }

    fn run_quick_optimize(&mut self) {
        let targets = get_default_targets();
        let mut freed_junk = 0u64;
        let mut deleted_files = 0usize;
        for target in &targets {
            if target.tier == CleanTier::Safe {
                let (f_bytes, f_count) = clean_target(target);
                freed_junk += f_bytes;
                deleted_files += f_count;
            }
        }

        for (i, target) in targets.iter().enumerate() {
            let (size, count) = scan_target(target);
            if let Some(cat) = self.clean_categories.get_mut(i) {
                cat.size_bytes = size;
                cat.file_count = count;
            }
        }

        let (freed_ram, count) = trim_working_sets(&mut self.sys);

        // Áp dụng tối ưu phần cứng động (không hardcode)
        let _ = self.autonomous.hardware_profile.apply_hardware_optimal(
            self.power_status.is_ac_online,
            self.power_status.battery_percent,
        );

        let (_, eco_count) = apply_foreground_boost_and_eco(
            &self.sys,
            self.foreground_info.as_ref().map(|(pid, _)| *pid),
        );

        self.mem_stats = get_memory_stats();
        self.top_processes = get_top_processes(&mut self.sys, 25);

        let msg = format!(
            "Dọn {} rác ({} files), thu hồi {} RAM từ {} app, tối ưu phần cứng ({} nhân, SSD: {}), ép EcoQoS {} app nền.",
            format_bytes(freed_junk),
            deleted_files,
            format_bytes(freed_ram),
            count,
            self.autonomous.hardware_profile.cpu_logical_cores,
            self.autonomous.hardware_profile.is_ssd,
            eco_count
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[TỐI ƯU]", format!("Tối ưu tức thì: {}", msg));
    }
}

impl eframe::App for OptimizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh_periodic();

        // Check if window restore was requested from Tray
        if crate::tray::SHOW_WINDOW_REQUESTED.swap(false, std::sync::atomic::Ordering::Relaxed) {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(true));
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
        }

        // Intercept close button (X) to keep background optimizations active
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            if self.config.minimize_to_tray {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.autonomous.log(
                    "[KHAY HỆ THỐNG]",
                    "Đã thu nhỏ xuống khay hệ thống. Mọi tối ưu (Auto-Pilot, RAM, Timer) vẫn tiếp tục duy trì.".to_string(),
                );
            }
        }

        // Dynamic 60 FPS repaint during user interaction (scrolling, clicking, hovering); low power 1 FPS when idle
        let is_active = ui.input(|i| {
            i.smooth_scroll_delta != Vec2::ZERO
                || i.pointer.is_moving()
                || i.pointer.any_down()
                || !i.raw.events.is_empty()
        });

        if is_active {
            ui.ctx().request_repaint();
        } else {
            ui.ctx().request_repaint_after(Duration::from_millis(1000));
        }

        let p = self.palette;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg_main).inner_margin(egui::Margin::symmetric(14, 12)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .wheel_scroll_multiplier(Vec2::new(1.0, 2.5))
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .show(ui, |ui| {
                        match self.current_tab {
                            Tab::Widget => self.render_widget_view(ui),
                            Tab::AdvancedSettings => self.render_advanced_view(ui),
                        }
                        ui.add_space(20.0);
                    });
            });
    }
}

impl OptimizerApp {
    // =========================================================================
    // 1. MÀN HÌNH WIDGET TRỢ LÝ TỰ HÀNH CHÍNH (STREAMLINED AUTONOMOUS WIDGET)
    // =========================================================================
    fn render_widget_view(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // --- TOP BAR: Logo + Auto-Pilot Status + Theme + Settings ---
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("⚡ OmniTune")
                    .size(20.0)
                    .strong()
                    .color(p.accent_blue),
            );

            // Auto-Pilot live badge (High contrast)
            let (ap_text, ap_color, ap_bg) = if !self.initial_scan_done {
                ("⟳ Đang khởi động...", p.accent_blue, if p.is_dark { Color32::from_rgb(26, 36, 60) } else { Color32::from_rgb(226, 234, 246) })
            } else if self.autonomous.auto_pilot_enabled {
                ("● Auto-Pilot Tự Hành", p.success_green, if p.is_dark { Color32::from_rgb(18, 48, 28) } else { Color32::from_rgb(220, 252, 231) })
            } else {
                ("○ Chế độ Thủ công", p.warn_amber, if p.is_dark { Color32::from_rgb(52, 38, 18) } else { Color32::from_rgb(254, 243, 199) })
            };

            let ap_badge = egui::Button::new(RichText::new(ap_text).size(11.5).strong().color(ap_color))
                .fill(ap_bg)
                .corner_radius(CornerRadius::same(5));
            if ui.add(ap_badge).on_hover_text("Bấm để Bật/Tắt chế độ Auto-Pilot tự hành (lưu tự động)").clicked() {
                if self.initial_scan_done {
                    self.autonomous.auto_pilot_enabled = !self.autonomous.auto_pilot_enabled;
                    self.sync_config();
                }
            }

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Settings button
                let gear_btn = egui::Button::new(RichText::new("⚙ Cài đặt").size(12.0).strong().color(p.text_primary))
                    .fill(p.card_bg)
                    .stroke(Stroke::new(1.0, p.card_border))
                    .corner_radius(CornerRadius::same(5));

                if ui.add(gear_btn).on_hover_text("Mở Bảng điều khiển Chuyên sâu & Ma trận Setup").clicked() {
                    self.current_tab = Tab::AdvancedSettings;
                }

                ui.add_space(4.0);

                // Pro / License status button
                if self.license.is_valid {
                    let pro_badge = egui::Button::new(RichText::new("★ PRO").size(11.5).strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(217, 119, 6))
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(pro_badge).on_hover_text(format!("OmniTune Pro: {}", self.license.licensee)).clicked() {
                        self.show_license_modal = !self.show_license_modal;
                    }
                } else {
                    let buy_btn = egui::Button::new(RichText::new("★ Lên PRO").size(11.5).strong().color(Color32::WHITE))
                        .fill(p.warn_amber)
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(buy_btn).on_hover_text("Nâng cấp lên bản OmniTune Pro").clicked() {
                        self.show_license_modal = !self.show_license_modal;
                    }
                }

                ui.add_space(4.0);

                // Theme toggle
                let (theme_text, theme_hover) = if self.theme_mode == ThemeMode::Dark {
                    ("Sáng", "Chuyển giao diện Sáng")
                } else {
                    ("Tối", "Chuyển giao diện Tối")
                };

                let theme_btn = egui::Button::new(RichText::new(theme_text).size(12.0).strong().color(p.text_primary))
                    .fill(p.card_bg)
                    .stroke(Stroke::new(1.0, p.card_border))
                    .corner_radius(CornerRadius::same(5));

                if ui.add(theme_btn).on_hover_text(theme_hover).clicked() {
                    self.theme_mode = match self.theme_mode {
                        ThemeMode::Dark => ThemeMode::Light,
                        ThemeMode::Light => ThemeMode::Dark,
                    };
                    self.palette = apply_theme(ui.ctx(), self.theme_mode);
                }
            });
        });

        ui.add_space(8.0);

        // --- LICENSE ACTIVATION CARD ---
        if self.show_license_modal {
            p.modern_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("★ BẢN QUYỀN OMNITUNE PRO").size(13.0).strong().color(p.warn_amber));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("✕ Đóng").size(11.0)).clicked() {
                            self.show_license_modal = false;
                        }
                    });
                });
                ui.separator();
                ui.label(RichText::new("Mở khóa vĩnh viễn: Dọn sạch NPM/Cargo cache hàng chục GB, Timer 1.0ms tối ưu game, khóa ngưỡng sạc pin 80%.").size(11.0).color(p.text_secondary));
                ui.add_space(4.0);

                ui.horizontal(|ui| {
                    ui.label(RichText::new("Email:").size(11.5).strong().color(p.text_primary));
                    ui.add(egui::TextEdit::singleline(&mut self.input_license_email).hint_text("Nhập email mua hàng...").desired_width(180.0));
                });
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Key:").size(11.5).strong().color(p.text_primary));
                    ui.add(egui::TextEdit::singleline(&mut self.input_license_key).hint_text("OTPRO-XXXX-XXXX-XXXX").desired_width(180.0));
                });

                ui.horizontal(|ui| {
                    let activate_btn = egui::Button::new(RichText::new("✓ Kích hoạt").size(11.5).strong().color(Color32::WHITE))
                        .fill(p.primary_btn);
                    if ui.add(activate_btn).clicked() {
                        if save_license(&self.input_license_email, &self.input_license_key) {
                            self.license = load_license();
                            self.license_feedback_msg = "✓ Kích hoạt OmniTune Pro thành công! Cảm ơn bạn.".to_string();
                            self.autonomous.log("[VIP]", "Đã kích hoạt bản quyền OmniTune Pro vĩnh viễn.".to_string());
                        } else {
                            self.license_feedback_msg = "✗ Mã bản quyền không hợp lệ. Vui lòng kiểm tra lại.".to_string();
                        }
                    }

                    if self.license.is_valid {
                        if ui.button(RichText::new("Hủy kích hoạt").size(11.0)).clicked() {
                            deactivate_license();
                            self.license = load_license();
                            self.license_feedback_msg = "Đã hủy kích hoạt bản quyền.".to_string();
                        }
                    }
                });

                ui.add_space(2.0);
                ui.label(RichText::new("💡 Master Key dùng thử offline: OTPRO-LIFETIME-VIP-2026").size(10.5).color(p.accent_blue));

                if !self.license_feedback_msg.is_empty() {
                    ui.add_space(2.0);
                    ui.label(RichText::new(&self.license_feedback_msg).size(11.0).strong().color(p.warn_amber));
                }
            });
            ui.add_space(6.0);
        }

        // --- 2 CHẾ ĐỘ TỐI ƯU HÓA HỆ THỐNG (HARDWARE OPTIMAL VS WORKLOAD ADAPTIVE) ---
        ui.horizontal(|ui| {
            let is_hw = self.autonomous.operating_mode == OperatingMode::HardwareOptimal;
            let hw_btn = egui::Button::new(
                RichText::new(if is_hw { "● 1. TỐI ƯU PHẦN CỨNG & OS" } else { "○ 1. Tối ưu Phần Cứng & OS" })
                    .size(11.5)
                    .strong()
                    .color(if is_hw { Color32::WHITE } else { p.text_secondary }),
            )
            .fill(if is_hw { p.primary_btn } else { p.card_bg })
            .stroke(Stroke::new(1.0, if is_hw { p.primary_btn } else { p.card_border }))
            .corner_radius(CornerRadius::same(6));

            if ui.add(hw_btn).on_hover_text("Chế độ 1: Tối ưu chuẩn xác dựa trên OS, CPU, RAM, SSD/HDD, GPU thực tế của máy (Không hardcode)").clicked() {
                self.autonomous.set_operating_mode(
                    OperatingMode::HardwareOptimal,
                    &mut self.sys,
                    self.power_status.is_ac_online,
                    self.power_status.battery_percent,
                );
                self.sync_config();
            }

            let is_wl = self.autonomous.operating_mode == OperatingMode::WorkloadAdaptive;
            let wl_btn = egui::Button::new(
                RichText::new(if is_wl { "● 2. TỰ ĐỘNG THEO CÔNG VIỆC" } else { "○ 2. Tự Động Theo Công Việc" })
                    .size(11.5)
                    .strong()
                    .color(if is_wl { Color32::WHITE } else { p.text_secondary }),
            )
            .fill(if is_wl { p.accent_blue } else { p.card_bg })
            .stroke(Stroke::new(1.0, if is_wl { p.accent_blue } else { p.card_border }))
            .corner_radius(CornerRadius::same(6));

            if ui.add(wl_btn).on_hover_text("Chế độ 2: Tự động điều tiết theo tác vụ đang làm (Dev, Game, Họp, Xem phim, Di động, Nghỉ ngơi)").clicked() {
                self.autonomous.set_operating_mode(
                    OperatingMode::WorkloadAdaptive,
                    &mut self.sys,
                    self.power_status.is_ac_online,
                    self.power_status.battery_percent,
                );
                self.sync_config();
            }
        });
        ui.add_space(6.0);

        // --- HEALTH & CONTEXT BANNER CARD ---
        let (score, score_label, score_color, tips) = self.calculate_health_score();
        p.modern_card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("CHỈ SỐ SỨC KHỎE").size(11.5).strong().color(p.text_secondary));
                ui.add_space(2.0);
                ui.heading(
                    RichText::new(format!("{}/100", score))
                        .size(36.0)
                        .strong()
                        .color(score_color),
                );
                ui.label(RichText::new(score_label).size(13.0).strong().color(score_color));

                ui.add_space(6.0);

                if self.autonomous.operating_mode == OperatingMode::HardwareOptimal {
                    // Chế độ 1: Card chi tiết phần cứng & cấu hình tối ưu động
                    egui::Frame::new()
                        .fill(if p.is_dark { Color32::from_rgb(26, 36, 60) } else { Color32::from_rgb(226, 234, 246) })
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            ui.vertical(|ui| {
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new("[CHẾ ĐỘ 1: PHẦN CỨNG & OS]").size(11.5).strong().color(p.accent_blue));
                                    ui.label(RichText::new(&self.autonomous.hardware_profile.os_name).size(11.5).strong().color(p.text_primary));
                                });
                                ui.add_space(3.0);
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(format!(
                                        "⚡ CPU: {} ({} nhân) | RAM: {:.1}GB (Ngưỡng tự động: {}%)",
                                        self.autonomous.hardware_profile.cpu_brand,
                                        self.autonomous.hardware_profile.cpu_logical_cores,
                                        self.autonomous.hardware_profile.ram_total_gb,
                                        self.autonomous.hardware_profile.dynamic_ram_threshold_pct
                                    )).size(10.5).color(p.text_secondary));
                                });
                                ui.horizontal_wrapped(|ui| {
                                    ui.label(RichText::new(format!(
                                        "💾 Ổ đĩa: {} ({}) | GPU: {}",
                                        self.autonomous.hardware_profile.disk_name,
                                        if self.autonomous.hardware_profile.is_ssd { "SSD - Đã tắt SysMain & NTFS ghi rác" } else { "HDD - Đã bật SysMain Cache" },
                                        self.autonomous.hardware_profile.gpu_name
                                    )).size(10.5).color(p.text_secondary));
                                });
                            });
                        });
                } else {
                    // Chế độ 2: Card ngữ cảnh công việc đang nhận diện
                    let scen = self.autonomous.current_scenario;
                    let (context_tag, context_name) = match scen {
                        Scenario::Developer => ("[DEV]", format!("Lập trình & Biên dịch (Mở 100% xung {} nhân, làm mát chủ động)", self.autonomous.hardware_profile.cpu_logical_cores)),
                        Scenario::Gaming => ("[GAMING]", format!("Đang Chơi Game (Timer 1.0ms • Tối đa GPU {})", self.autonomous.hardware_profile.gpu_name)),
                        Scenario::OfficeMeeting => ("[VĂN PHÒNG]", "Họp & Làm việc (Giảm trễ TCP, quạt êm chống ồn, chống tắt màn)".to_string()),
                        Scenario::RemoteDesktop => ("[REMOTE]", "Đang Remote Desktop (Tối ưu mạng TCP & trễ chuột, quạt 0dB)".to_string()),
                        Scenario::MediaChill => ("[MEDIA]", format!("Xem phim & Thư giãn (Màn hình luôn sáng • GPU {})", self.autonomous.hardware_profile.gpu_name)),
                        Scenario::CafeMobility => ("[PIN]", format!("Di động / Tiết kiệm pin (Khóa trần CPU {} nhân, quạt tắt)", self.autonomous.hardware_profile.cpu_logical_cores)),
                        Scenario::IdlePowerSave => ("[NGHỈ]", format!("Nhàn rỗi tự động (Không thao tác > 5p, hạ tải CPU {} nhân)", self.autonomous.hardware_profile.cpu_logical_cores)),
                        Scenario::Default => ("[CÂN BẰNG]", format!("Chế độ Cân bằng thích ứng ({} nhân, {:.1}GB RAM)", self.autonomous.hardware_profile.cpu_logical_cores, self.autonomous.hardware_profile.ram_total_gb)),
                    };

                    egui::Frame::new()
                        .fill(if p.is_dark { Color32::from_rgb(26, 36, 60) } else { Color32::from_rgb(226, 234, 246) })
                        .corner_radius(CornerRadius::same(6))
                        .inner_margin(egui::Margin::symmetric(10, 6))
                        .show(ui, |ui| {
                            ui.horizontal_wrapped(|ui| {
                                ui.label(RichText::new(context_tag).size(11.5).strong().color(p.accent_blue));
                                ui.label(RichText::new(&context_name).size(11.5).strong().color(p.text_primary));
                            });
                        });
                }

                ui.add_space(6.0);

                // Smart Diagnostic Tip (Wrapped)
                if let Some(tip) = tips.first() {
                    ui.label(RichText::new(format!("💡 {}", tip)).size(11.5).color(p.text_secondary));
                }

                ui.add_space(8.0);

                // Hero Action Button
                let opt_btn = egui::Button::new(
                    RichText::new("► TỐI ƯU HÓA TOÀN DIỆN (1-CLICK)")
                        .size(13.5)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(p.primary_btn)
                .min_size(Vec2::new(ui.available_width() - 20.0, 38.0))
                .corner_radius(CornerRadius::same(6));

                if ui.add(opt_btn).clicked() {
                    self.run_quick_optimize();
                }

                if !self.quick_opt_msg.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.quick_opt_msg).size(11.5).color(p.success_green).strong());
                }
            });
        });

        ui.add_space(8.0);

        // --- 3 VITALS CHIPS (CPU, RAM, PIN - HIGH CONTRAST & NO OVERFLOW) ---
        ui.columns(3, |cols| {
            // CPU Chip
            p.modern_card(&mut cols[0], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("CPU").size(11.5).strong().color(p.text_secondary));
                    let cpu = self.protection.cpu_usage;
                    let cpu_color = if cpu > 80.0 { p.danger_red } else { p.accent_blue };
                    let cpu_text = if self.initial_scan_done {
                        format!("{:.1}%", cpu)
                    } else {
                        "⟳ Đang đo".to_string()
                    };
                    ui.heading(RichText::new(cpu_text).size(22.0).strong().color(cpu_color));
                    let display_cpu = self.autonomous.hardware_profile.cpu_brand
                        .replace("(R)", "")
                        .replace("(TM)", "")
                        .replace("CPU", "")
                        .trim()
                        .to_string();
                    ui.label(RichText::new(display_cpu).size(11.0).strong().color(p.text_primary));
                    ui.label(RichText::new(&self.cpu_cores_info).size(10.5).color(p.text_secondary));
                });
            });

            // RAM Chip
            p.modern_card(&mut cols[1], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("RAM").size(11.5).strong().color(p.text_secondary));
                    let load = self.mem_stats.load_percent;
                    let ram_color = if load > 85 { p.danger_red } else { p.success_green };
                    ui.heading(RichText::new(format!("{}%", load)).size(22.0).strong().color(ram_color));
                    ui.label(
                        RichText::new(format_bytes(self.mem_stats.used_bytes))
                            .size(11.5)
                            .strong()
                            .color(p.text_primary),
                    );
                    ui.label(
                        RichText::new(format!("Trống {}", format_bytes(self.mem_stats.free_bytes)))
                            .size(10.5)
                            .color(p.text_secondary),
                    );
                });
            });

            // Battery Chip
            p.modern_card(&mut cols[2], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("PIN").size(11.5).strong().color(p.text_secondary));
                    if self.power_status.has_battery {
                        let bat = self.power_status.battery_percent;
                        let bat_color = if bat < 20 { p.danger_red } else { p.warn_amber };
                        ui.heading(RichText::new(format!("{}%", bat)).size(22.0).strong().color(bat_color));
                        let pwr_str = if self.power_status.discharge_rate_mw > 0 {
                            format!("{:.1}W xả", self.power_status.power_watts)
                        } else if self.power_status.charge_rate_mw > 0 {
                            format!("{:.1}W nạp", self.power_status.power_watts)
                        } else if self.power_status.is_ac_online {
                            "Cắm sạc AC".to_string()
                        } else {
                            "Dùng pin".to_string()
                        };
                        ui.label(RichText::new(pwr_str).size(11.5).strong().color(p.text_primary));
                        let volt_str = if self.power_status.voltage_volts > 0.0 {
                            format!("{:.1} V", self.power_status.voltage_volts)
                        } else {
                            "Pin kép".to_string()
                        };
                        ui.label(RichText::new(volt_str).size(10.5).color(p.text_secondary));
                    } else {
                        ui.heading(RichText::new("Nguồn AC").size(20.0).strong().color(p.accent_blue));
                        ui.label(RichText::new("PC để bàn").size(11.5).strong().color(p.text_primary));
                        ui.label(RichText::new("Trực tiếp 220V").size(10.5).color(p.text_secondary));
                    }
                });
            });
        });

        ui.add_space(8.0);

        // --- FOREGROUND GOVERNOR CARD ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Cửa sổ Focus:").size(11.5).strong().color(p.text_secondary));
                        if let Some((_, ref name)) = self.foreground_info {
                            ui.label(RichText::new(name).size(12.0).strong().color(p.accent_blue));
                        } else {
                            ui.label(RichText::new("Chưa xác định").size(11.5).color(p.text_secondary));
                        }
                    });
                    ui.label(
                        RichText::new("Tự động ưu tiên luồng cho cửa sổ đang dùng, hạ EcoQoS app ngầm.")
                            .size(11.0)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(
                        &mut self.autonomous.eco_qos_enabled,
                        RichText::new("EcoQoS Bật").size(11.5).strong().color(p.accent_blue),
                    );
                });
            });
        });

        ui.add_space(8.0);

        // --- AUTONOMOUS ACTIVITY STREAM (NHẬT KÝ TỰ HÀNH - HIGH CONTRAST & WRAPPED) ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("[LIVE] NHẬT KÝ TỰ ĐỘNG CHẠY NGẦM")
                        .size(11.5)
                        .strong()
                        .color(p.accent_blue),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(
                        &mut self.autonomous.auto_pilot_enabled,
                        RichText::new("Auto-Pilot").size(11.0).strong().color(p.success_green),
                    );
                });
            });
            ui.separator();
            ui.add_space(2.0);

            if self.autonomous.activity_log.is_empty() {
                ui.label(RichText::new("Đang theo dõi ngữ cảnh...").size(11.0).color(p.text_secondary));
            } else {
                for ev in self.autonomous.activity_log.iter().take(5) {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new(&ev.time).size(10.5).color(p.text_secondary));
                        ui.label(RichText::new(ev.icon).size(11.0).strong().color(p.accent_blue));
                        ui.label(RichText::new(&ev.message).size(11.0).color(p.text_primary));
                    });
                    ui.add_space(2.0);
                }
            }
        });
    }

    // =========================================================================
    // 2. MÀN HÌNH CHUYÊN SÂU: MA TRẬN SETUP & BẢO TRÌ (DRAWER / ADVANCED VIEW)
    // =========================================================================
    fn render_advanced_view(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            let back_btn = egui::Button::new(
                RichText::new("◄ Quay lại Widget")
                    .size(12.0)
                    .strong()
                    .color(Color32::WHITE),
            )
            .fill(p.primary_btn)
            .corner_radius(CornerRadius::same(5));

            if ui.add(back_btn).clicked() {
                self.current_tab = Tab::Widget;
            }

            ui.heading(
                RichText::new("Cài Đặt & Ma Trận Tinh Chỉnh Chuyên Sâu")
                    .size(15.5)
                    .strong()
                    .color(p.text_primary),
            );
        });

        ui.add_space(6.0);
        ui.label(
            RichText::new("Can thiệp sâu vào thông số phần cứng, dọn rác 3 tầng, tiến trình và bảo vệ máy.")
                .size(11.5)
                .color(p.text_secondary),
        );
        ui.add_space(6.0);

        // --- PHẦN 1: BẢNG MA TRẬN SETUP (EGUI GRID CHUẨN XÁC) ---
        ui.label(RichText::new("1. MA TRẬN 10 TIÊU CHÍ SETUP PHẦN CỨNG").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_criteria_grid(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // --- PHẦN 2: DỌN RÁC 3 TẦNG & RAM ---
        ui.label(RichText::new("2. DỌN DẸP RÁC 3 TẦNG & BỘ NHỚ RAM").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_cleaner_section(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // --- PHẦN 3: KHỞI ĐỘNG & BẢO VỆ ---
        ui.label(RichText::new("3. KHỞI ĐỘNG & BẢO VỆ PHẦN CỨNG").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_hardware_section(ui);

        ui.add_space(10.0);
        ui.separator();
        ui.add_space(10.0);

        // --- PHẦN 4: KHAY HỆ THỐNG & TỰ HÀNH CHẠY NGẦM ---
        ui.label(RichText::new("4. KHAY HỆ THỐNG & DUY TRÌ TỐI ƯU LIÊN TỤC").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_tray_and_persistence_section(ui);
    }

    // Grid chuẩn xác, căn đều 2 cột, không bao giờ bị lệch hay tràn viền
    fn render_criteria_grid(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        p.modern_card(ui, |ui| {
            egui::Grid::new("criteria_table_grid")
                .striped(true)
                .min_col_width(130.0)
                .max_col_width(380.0)
                .spacing(Vec2::new(14.0, 10.0))
                .show(ui, |ui| {
                    // Header Row
                    ui.label(RichText::new("TIÊU CHÍ").size(12.0).strong().color(p.accent_blue));
                    ui.label(RichText::new("TINH CHỈNH THÔNG SỐ TRỰC TIẾP & KIẾN NGHỊ").size(12.0).strong().color(p.accent_blue));
                    ui.end_row();

                    // Row 1: CPU Xung & TDP
                    ui.vertical(|ui| {
                        ui.label(RichText::new("1. Xung CPU & TDP").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new(format!("{} ({} nhân)", self.autonomous.hardware_profile.cpu_brand, self.autonomous.hardware_profile.cpu_logical_cores)).size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Pin (DC):").size(11.0).strong().color(p.text_primary));
                            if ui.add(egui::Slider::new(&mut self.cpu_limit_dc, 50..=100).suffix("%")).changed() {
                                let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
                                self.sync_config();
                            }
                            if ui.button(RichText::new("60%").size(10.0)).clicked() {
                                self.cpu_limit_dc = 60;
                                let _ = set_cpu_max_state_dc(60);
                                self.sync_config();
                            }
                            if ui.button(RichText::new("80%").size(10.0)).clicked() {
                                self.cpu_limit_dc = 80;
                                let _ = set_cpu_max_state_dc(80);
                                self.sync_config();
                            }
                        });
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Sạc (AC):").size(11.0).strong().color(p.text_primary));
                            if ui.add(egui::Slider::new(&mut self.cpu_limit_ac, 50..=100).suffix("%")).changed() {
                                let _ = set_cpu_max_state_ac(self.cpu_limit_ac);
                                self.sync_config();
                            }
                            if ui.button(RichText::new("100% Turbo").size(10.0)).clicked() {
                                self.cpu_limit_ac = 100;
                                let _ = set_cpu_max_state_ac(100);
                                self.sync_config();
                            }
                        });
                        ui.label(RichText::new(format!("💡 Pin: Giới hạn trần để tiết kiệm điện; Sạc: 100% mở tối đa xung nhịp {} nhân.", self.autonomous.hardware_profile.cpu_logical_cores)).size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();

                    // Row 2: Timer Resolution
                    ui.vertical(|ui| {
                        ui.label(RichText::new("2. Timer & FPS").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new("15.6ms vs 1.0ms tối ưu").size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let btn_1ms = egui::Button::new(RichText::new("1.0 ms (Game & Remote)").size(11.0).color(Color32::WHITE))
                                .fill(if self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg });
                            if ui.add(btn_1ms).clicked() {
                                self.timer_1ms_enabled = true;
                                set_timer_resolution_1ms(true);
                                self.sync_config();
                            }

                            let btn_15ms = egui::Button::new(RichText::new("15.6 ms (Mặc định)").size(11.0).color(if !self.timer_1ms_enabled { Color32::WHITE } else { p.text_primary }))
                                .fill(if !self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg });
                            if ui.add(btn_15ms).clicked() {
                                self.timer_1ms_enabled = false;
                                set_timer_resolution_1ms(false);
                                self.sync_config();
                            }
                        });
                        ui.label(RichText::new("💡 1.0ms triệt tiêu giật khung hình & trễ chuột; 15.6ms tiết kiệm pin.").size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();

                    // Row 3: Brightness
                    ui.vertical(|ui| {
                        ui.label(RichText::new("3. Độ sáng Màn hình").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new("Panel LED (30-50% pin)").size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            if ui.add(egui::Slider::new(&mut self.brightness_level, 10..=100).suffix("%")).changed() {
                                let _ = set_brightness(self.brightness_level);
                            }
                            if ui.button(RichText::new("45% Pin").size(10.0)).clicked() {
                                self.brightness_level = 45;
                                let _ = set_brightness(45);
                            }
                            if ui.button(RichText::new("70% Phim").size(10.0)).clicked() {
                                self.brightness_level = 70;
                                let _ = set_brightness(70);
                            }
                        });
                        ui.label(RichText::new("💡 Cà phê/Pin: 40-45% tối ưu nhất; Văn phòng: 50%; Phim: 70%.").size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();

                    // Row 4: Fan Cooling
                    ui.vertical(|ui| {
                        ui.label(RichText::new("4. Quạt làm mát").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new("Active vs Passive 0dB").size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let btn_p = egui::Button::new(RichText::new("Thụ động (0dB)").size(11.0).color(if !self.cooling_active { Color32::WHITE } else { p.text_primary }))
                                .fill(if !self.cooling_active { Color32::from_rgb(16, 120, 60) } else { p.tab_inactive_bg });
                            if ui.add(btn_p).clicked() {
                                self.cooling_active = false;
                                let _ = set_cooling_policy(false);
                                self.sync_config();
                            }

                            let btn_a = egui::Button::new(RichText::new("Chủ động (Active)").size(11.0).color(if self.cooling_active { Color32::WHITE } else { p.text_primary }))
                                .fill(if self.cooling_active { p.primary_btn } else { p.tab_inactive_bg });
                            if ui.add(btn_a).clicked() {
                                self.cooling_active = true;
                                let _ = set_cooling_policy(true);
                                self.sync_config();
                            }
                        });
                        ui.label(RichText::new("💡 Passive cho cafe/remote êm ái; Active cho game & compile.").size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();

                    // Row 5: GameDVR & Sleep Lock
                    ui.vertical(|ui| {
                        ui.label(RichText::new("5. GameDVR & Sleep").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new("Quay màn hình & Giữ máy").size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let gdvr_text = if self.gamedvr_enabled { "GameDVR: Bật" } else { "GameDVR: Tắt" };
                            if ui.button(RichText::new(gdvr_text).size(10.5)).clicked() {
                                self.gamedvr_enabled = !self.gamedvr_enabled;
                                let _ = set_gamedvr(self.gamedvr_enabled);
                                self.sync_config();
                            }

                            if ui.selectable_value(&mut self.keep_awake_mode, 0, "Sleep OS").clicked() {
                                set_keep_awake(0);
                                self.sync_config();
                            }
                            if ui.selectable_value(&mut self.keep_awake_mode, 2, "Chống Sleep").clicked() {
                                set_keep_awake(2);
                                self.sync_config();
                            }
                        });
                        ui.label(RichText::new("💡 Tắt GameDVR giải phóng GPU encoder; Bật Chống Sleep khi remote/xem phim.").size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();

                    // Row 6: Battery Threshold & Hibernate
                    ui.vertical(|ui| {
                        ui.label(RichText::new("6. Ngưỡng pin & Ngủ").size(12.0).strong().color(p.text_primary));
                        ui.label(RichText::new("Khóa 80% & Hibernate").size(10.5).color(p.text_secondary));
                    });
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            let thresh_text = if self.protection.battery_threshold_enabled { "Ngưỡng 80%: Bật" } else { "Ngưỡng 80%: Tắt" };
                            let thresh_col = if self.protection.battery_threshold_enabled { p.danger_red } else { Color32::from_rgb(16, 120, 60) };
                            if ui.add(egui::Button::new(RichText::new(thresh_text).size(10.5).color(Color32::WHITE)).fill(thresh_col)).clicked() {
                                let st = !self.protection.battery_threshold_enabled;
                                let _ = set_battery_threshold(st, 80);
                                self.protection.battery_threshold_enabled = st;
                                self.sync_config();
                            }

                            if !self.protection.is_hibernate_enabled {
                                if ui.button(RichText::new("Bật Hibernate").size(10.5)).clicked() {
                                    let _ = enable_hibernate_as_admin();
                                    self.protection.is_hibernate_enabled = true;
                                }
                            } else {
                                ui.label(RichText::new("✓ Hibernate Đã bật").size(11.0).color(p.success_green));
                            }
                        });
                        ui.label(RichText::new("💡 Bật ngưỡng 80% khi cắm sạc bàn làm việc; Dùng Hibernate thay vì Sleep S3.").size(10.5).color(p.text_secondary));
                    });
                    ui.end_row();
                });
        });
    }

    fn render_cleaner_section(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            let scan_btn = egui::Button::new(RichText::new("Quét toàn bộ").size(12.0).color(Color32::WHITE))
                .fill(p.primary_btn);
            if ui.add(scan_btn).clicked() {
                let targets = get_default_targets();
                for (i, target) in targets.iter().enumerate() {
                    let (size, count) = scan_target(target);
                    if let Some(cat) = self.clean_categories.get_mut(i) {
                        cat.size_bytes = size;
                        cat.file_count = count;
                    }
                }
                let total_size: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();
                self.clean_status_msg = format!("✓ Quét xong: Tìm thấy {}.", format_bytes(total_size));
            }

            let clean_safe_btn = egui::Button::new(RichText::new("Dọn Rác Tầng 1").size(12.0).color(Color32::WHITE))
                .fill(Color32::from_rgb(16, 120, 60));
            if ui.add(clean_safe_btn).clicked() {
                let targets = get_default_targets();
                let mut freed = 0u64;
                for target in &targets {
                    if target.tier == CleanTier::Safe {
                        let (f, _) = clean_target(target);
                        freed += f;
                    }
                }
                self.clean_status_msg = format!("✓ Đã dọn an toàn {}.", format_bytes(freed));
            }

            let dev_btn_text = if self.license.is_valid {
                "Dọn Dev Cache (NPM/Go)"
            } else {
                "★ [PRO] Dọn Dev Cache"
            };

            let clean_dev_btn = egui::Button::new(RichText::new(dev_btn_text).size(12.0).color(Color32::WHITE))
                .fill(if self.license.is_valid { Color32::from_rgb(147, 51, 234) } else { p.warn_amber });

            if ui.add(clean_dev_btn).clicked() {
                if !self.license.is_valid {
                    self.show_license_modal = true;
                    self.current_tab = Tab::Widget;
                    self.license_feedback_msg = "Tính năng dọn cache lập trình viên (NPM/Go hàng GB) chỉ có trên OmniTune Pro. Nhập key hoặc kích hoạt để mở khóa!".to_string();
                } else {
                    let targets = get_default_targets();
                    let mut freed = 0u64;
                    for target in &targets {
                        if target.tier == CleanTier::Developer {
                            let (f, _) = clean_target(target);
                            freed += f;
                        }
                    }
                    self.clean_status_msg = format!("✓ Đã dọn Dev Cache {}.", format_bytes(freed));
                }
            }
        });

        if !self.clean_status_msg.is_empty() {
            ui.add_space(2.0);
            ui.label(RichText::new(&self.clean_status_msg).size(11.5).color(p.accent_blue));
        }

        ui.add_space(6.0);

        // Process search and actions
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("DANH SÁCH TIẾN TRÌNH").size(11.5).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(egui::TextEdit::singleline(&mut self.process_search_query).hint_text("Lọc app...").desired_width(120.0));
                });
            });

            if !self.process_action_msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&self.process_action_msg).size(11.0).color(p.success_green));
            }

            ui.separator();
            ui.add_space(2.0);

            let query = self.process_search_query.trim().to_lowercase();
            let filtered_procs: Vec<_> = self
                .top_processes
                .iter()
                .filter(|proc| {
                    if query.is_empty() { true } else { proc.name.to_lowercase().contains(&query) || proc.pid.to_string().contains(&query) }
                })
                .take(8)
                .collect();

            egui::Grid::new("adv_proc_grid").striped(true).spacing(Vec2::new(8.0, 4.0)).show(ui, |ui| {
                ui.label(RichText::new("Ứng dụng").size(11.0).strong().color(p.text_secondary));
                ui.label(RichText::new("RAM").size(11.0).strong().color(p.text_secondary));
                ui.label(RichText::new("Thao tác").size(11.0).strong().color(p.text_secondary));
                ui.end_row();

                for proc in filtered_procs {
                    ui.label(RichText::new(&proc.name).size(11.0).strong().color(p.text_primary));
                    ui.label(RichText::new(format_bytes(proc.memory_bytes)).size(11.0).color(p.accent_blue));
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Dọn").size(10.0)).clicked() {
                            let f = trim_single_process(proc.pid);
                            self.process_action_msg = format!("✓ Thu hồi {} từ '{}'", format_bytes(f), proc.name);
                        }
                        if ui.add(egui::Button::new(RichText::new("Tắt").size(10.0).color(p.danger_red))).clicked() {
                            let _ = terminate_process(proc.pid);
                            self.process_action_msg = format!("✓ Kết thúc '{}'", proc.name);
                        }
                    });
                    ui.end_row();
                }
            });
        });
    }

    fn render_hardware_section(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Startup apps toggle
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("ỨNG DỤNG KHỞI ĐỘNG CÙNG WINDOWS").size(11.5).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("Làm mới").size(11.0)).clicked() {
                        self.startup_apps = get_startup_apps();
                    }
                });
            });

            if !self.startup_action_msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&self.startup_action_msg).size(11.0).color(p.success_green));
            }

            ui.separator();

            for app in &mut self.startup_apps {
                ui.horizontal(|ui| {
                    ui.label(RichText::new(&app.name).size(11.0).strong().color(p.text_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let btn_text = if app.is_enabled { "Tắt" } else { "Bật" };
                        let btn_col = if app.is_enabled { p.danger_red } else { p.primary_btn };
                        if ui.add(egui::Button::new(RichText::new(btn_text).size(10.0).color(Color32::WHITE)).fill(btn_col)).clicked() {
                            let new_st = !app.is_enabled;
                            if toggle_startup_item(&app.name, &app.location, new_st) {
                                app.is_enabled = new_st;
                                self.startup_action_msg = format!("✓ Đã {} '{}'", if new_st { "bật" } else { "tắt" }, app.name);
                            }
                        }
                    });
                });
            }
        });

        ui.add_space(6.0);

        // Telemetry Debloat
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("WINDOWS DIAGTRACK TELEMETRY").size(11.5).strong().color(p.accent_blue));
                    ui.label(RichText::new("Chặn thu thập dữ liệu ngầm của Microsoft.").size(10.5).color(p.text_secondary));
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_t = if self.is_telemetry_running { "Tắt DiagTrack" } else { "Bật DiagTrack" };
                    let btn_c = if self.is_telemetry_running { p.danger_red } else { Color32::from_rgb(16, 120, 60) };
                    if ui.add(egui::Button::new(RichText::new(btn_t).size(10.5).color(Color32::WHITE)).fill(btn_c)).clicked() {
                        let _ = toggle_telemetry(self.is_telemetry_running);
                        self.is_telemetry_running = !self.is_telemetry_running;
                    }
                });
            });
        });
    }

    fn render_tray_and_persistence_section(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        p.modern_card(ui, |ui| {
            ui.label(RichText::new("CƠ CHẾ LƯU VĨNH VIỄN & ĐÓNG ỨNG DỤNG").size(11.5).strong().color(p.accent_blue));
            ui.add_space(4.0);

            // Ghi vĩnh viễn
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔒 Trạng thái lưu:").size(11.5).strong().color(p.text_primary));
                ui.label(RichText::new("Đã ghi trực tiếp vào Windows (Registry & Powercfg)").size(11.5).color(p.success_green));
            });
            ui.label(RichText::new("Mọi thông số (Xung CPU, Chống Sleep, Timer 1.0ms, GameDVR, Telemetry) đều được ghi vĩnh viễn vào hệ điều hành. Khi bạn TẮT HẲN OmniTune, Windows vẫn giữ nguyên 100% các thiết lập này.").size(10.5).color(p.text_secondary));

            ui.add_space(6.0);
            let apply_btn = egui::Button::new(RichText::new("⚡ Áp dụng & Ép ghi vĩnh viễn vào Windows").size(11.0).strong().color(Color32::WHITE))
                .fill(p.primary_btn);
            if ui.add(apply_btn).clicked() {
                set_cpu_max_state_dc(self.cpu_limit_dc);
                set_cpu_max_state_ac(self.cpu_limit_ac);
                set_timer_resolution_1ms(self.timer_1ms_enabled);
                set_keep_awake(self.keep_awake_mode);
                set_cooling_policy(self.cooling_active);
                set_gamedvr(self.gamedvr_enabled);
                crate::power::apply_permanent_windows_system_tweaks();
                self.sync_config();
                self.quick_opt_msg = "✓ Đã ghi toàn bộ thông số vĩnh viễn vào Windows thành công!".to_string();
            }

            ui.add_space(8.0);
            ui.separator();
            ui.add_space(6.0);

            // Toggle Minimize to tray
            if ui.checkbox(&mut self.config.minimize_to_tray, RichText::new("Thu nhỏ xuống khay hệ thống thay vì tắt hẳn khi bấm [X]").size(11.5).color(p.text_primary)).changed() {
                self.sync_config();
            }
            ui.label(RichText::new("💡 Mặc định TẮT: Bấm [X] sẽ tắt ứng dụng hoàn toàn mà không cần chạy ngầm (các thông số Windows vẫn có hiệu lực). BẬT nếu bạn muốn app chạy ngầm ở góc đồng hồ.").size(10.5).color(p.text_secondary));

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            // Toggle Run on Startup
            if ui.checkbox(&mut self.run_on_startup, RichText::new("Khởi động ngầm cùng Windows").size(11.5).color(p.text_primary)).changed() {
                let ok = set_autostart_registry(self.run_on_startup);
                if ok {
                    self.config.run_on_startup = self.run_on_startup;
                    self.sync_config();
                    self.startup_action_msg = if self.run_on_startup {
                        "✓ Đã đăng ký OmniTune tự chạy cùng Windows".to_string()
                    } else {
                        "✓ Đã hủy đăng ký tự chạy cùng Windows".to_string()
                    };
                }
            }

            ui.add_space(6.0);
            ui.separator();
            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Tắt ứng dụng:").size(11.0).color(p.text_secondary));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let exit_btn = egui::Button::new(RichText::new("Đóng OmniTune").size(11.0).color(Color32::WHITE))
                        .fill(p.danger_red);
                    if ui.add(exit_btn).clicked() {
                        std::process::exit(0);
                    }
                });
            });
        });
    }
}
