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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTab {
    Cpu,
    Ram,
    Pin,
    Drive,
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

    // Storage, Network & System Tuning (33 Parameters)
    ram_trim_threshold_pct: u32,
    sysmain_disabled: bool,
    disable_ntfs_last_access: bool,
    mmcss_optimized: bool,
    tcp_autotuning: bool,

    // Nhóm 1: CPU & Điều độ
    cpu_min_state_pct: u32,
    win32_priority_boost: bool,
    fast_shutdown_recovery: bool,

    // Nhóm 2: RAM & Cache
    large_system_cache: bool,
    memory_compression: bool,

    // Nhóm 3: Ổ đĩa SSD/HDD
    disable_8dot3_name: bool,
    trim_enabled: bool,
    wait_to_kill_service: bool,

    // Nhóm 4: Đồ họa & Độ trễ
    menu_show_delay_zero: bool,
    startup_delay_zero: bool,

    // Nhóm 5: Mạng TCP/IP
    system_responsiveness_zero: bool,
    tcp_ecn_enabled: bool,

    // Nhóm 7: Telemetry & DiagTrack
    disable_telemetry: bool,
    disable_dmwappush: bool,

    criteria_apply_msg: String,

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

    // 4 Partitions Selection on Home screen
    selected_partition: Option<PartitionTab>,
}

fn render_switch_custom(ui: &mut egui::Ui, value: &mut bool, on_text: &str, off_text: &str) -> bool {
    let (text, fill_color) = if *value {
        (on_text, Color32::from_rgb(16, 149, 60))
    } else {
        (off_text, Color32::from_rgb(100, 116, 139))
    };
    let btn = egui::Button::new(
        RichText::new(text)
            .size(11.0)
            .strong()
            .color(Color32::WHITE),
    )
    .min_size(Vec2::new(66.0, 22.0))
    .fill(fill_color)
    .corner_radius(CornerRadius::same(11));

    if ui.add(btn).clicked() {
        *value = !*value;
        true
    } else {
        false
    }
}

fn render_switch(ui: &mut egui::Ui, value: &mut bool) -> bool {
    render_switch_custom(ui, value, "● BẬT", "○ TẮT")
}

fn render_mini_bar(ui: &mut egui::Ui, fraction: f32, color: Color32, bg_color: Color32) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 4.0), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), bg_color);
    let mut fill_rect = rect;
    fill_rect.set_width((rect.width() * fraction.clamp(0.0, 1.0)).max(2.0));
    ui.painter().rect_filled(fill_rect, CornerRadius::same(2), color);
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
        if config.brightness_level > 0 {
            let _ = set_brightness(config.brightness_level);
        }
        if config.sysmain_disabled {
            crate::power::set_sysmain_service(false);
        }
        if config.disable_ntfs_last_access {
            crate::power::set_ntfs_last_access(true);
        }
        if config.mmcss_optimized {
            crate::power::set_mmcss_gaming_responsiveness(true);
        }
        if config.tcp_autotuning {
            crate::power::set_tcp_optimization(true);
        }
        if config.cpu_min_state_pct > 0 {
            crate::power::set_cpu_min_state(config.cpu_min_state_pct);
        }
        if config.win32_priority_boost {
            crate::power::set_win32_priority_separation(true);
        }
        if config.fast_shutdown_recovery {
            crate::power::set_fast_shutdown_recovery(true);
        }
        if config.large_system_cache {
            crate::power::set_large_system_cache(true);
        }
        if config.memory_compression {
            crate::power::set_memory_compression(true);
        }
        if config.disable_8dot3_name {
            crate::power::set_ntfs_8dot3_name(true);
        }
        if config.trim_enabled {
            crate::power::set_ssd_trim(true);
        }
        if config.wait_to_kill_service {
            crate::power::set_wait_to_kill_service(true);
        }
        if config.menu_show_delay_zero {
            crate::power::set_menu_show_delay(true);
        }
        if config.startup_delay_zero {
            crate::power::set_startup_delay(true);
        }
        if config.tcp_ecn_enabled {
            crate::power::set_tcp_ecn(true);
        }
        if config.disable_telemetry {
            crate::startup::toggle_telemetry(true);
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

        let mut hw = crate::hardware_profile::HardwareProfile::detect(&mut sys);
        hw.dynamic_ram_threshold_pct = config.ram_trim_threshold_pct;
        let mut autonomous = AutonomousEngine::new(
            hw.clone(),
            config.operating_mode,
            config.saved_scenario,
            config.auto_pilot_enabled,
            config.eco_qos_enabled,
            config.auto_ram_enabled,
        );

        // Khôi phục & thực thi ngay cấu hình chế độ đã lưu
        match config.operating_mode {
            OperatingMode::HardwareOptimal => {
                let msgs = hw.apply_hardware_optimal(power_status.is_ac_online, power_status.battery_percent);
                autonomous.log(
                    "[KHÔI PHỤC]",
                    format!(
                        "Đã khôi phục Chế độ Tối ưu Phần Cứng & OS ({} nhân, {:.1}GB RAM, SSD: {}).",
                        hw.cpu_logical_cores, hw.ram_total_gb, hw.is_ssd
                    ),
                );
                for m in msgs {
                    autonomous.log("[ÁP DỤNG]", m);
                }
            }
            OperatingMode::WorkloadAdaptive => {
                let msg = crate::scenarios::apply_scenario(
                    config.saved_scenario,
                    &mut sys,
                    &hw,
                    power_status.is_ac_online,
                    power_status.battery_percent,
                );
                autonomous.log(
                    "[KHÔI PHỤC]",
                    format!(
                        "Đã khôi phục Kịch bản Công việc [{}]: {}",
                        config.saved_scenario.name(),
                        msg
                    ),
                );
            }
        }

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
            brightness_level: config.brightness_level,
            ram_trim_threshold_pct: config.ram_trim_threshold_pct,
            sysmain_disabled: config.sysmain_disabled,
            disable_ntfs_last_access: config.disable_ntfs_last_access,
            mmcss_optimized: config.mmcss_optimized,
            tcp_autotuning: config.tcp_autotuning,

            cpu_min_state_pct: config.cpu_min_state_pct,
            win32_priority_boost: config.win32_priority_boost,
            fast_shutdown_recovery: config.fast_shutdown_recovery,
            large_system_cache: config.large_system_cache,
            memory_compression: config.memory_compression,
            disable_8dot3_name: config.disable_8dot3_name,
            trim_enabled: config.trim_enabled,
            wait_to_kill_service: config.wait_to_kill_service,
            menu_show_delay_zero: config.menu_show_delay_zero,
            startup_delay_zero: config.startup_delay_zero,
            system_responsiveness_zero: config.system_responsiveness_zero,
            tcp_ecn_enabled: config.tcp_ecn_enabled,
            disable_telemetry: config.disable_telemetry,
            disable_dmwappush: true,

            criteria_apply_msg: String::new(),
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
            selected_partition: None,
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
        self.config.saved_scenario = self.autonomous.current_scenario;
        self.config.eco_qos_enabled = self.autonomous.eco_qos_enabled;
        self.config.auto_ram_enabled = self.autonomous.auto_ram_enabled;
        self.config.brightness_level = self.brightness_level;
        self.config.ram_trim_threshold_pct = self.ram_trim_threshold_pct;
        self.config.sysmain_disabled = self.sysmain_disabled;
        self.config.disable_ntfs_last_access = self.disable_ntfs_last_access;
        self.config.mmcss_optimized = self.mmcss_optimized;
        self.config.tcp_autotuning = self.tcp_autotuning;
        self.config.battery_threshold_80 = self.protection.battery_threshold_enabled;
        self.config.run_on_startup = self.run_on_startup;

        // 33 parameters sync
        self.config.cpu_min_state_pct = self.cpu_min_state_pct;
        self.config.win32_priority_boost = self.win32_priority_boost;
        self.config.fast_shutdown_recovery = self.fast_shutdown_recovery;
        self.config.large_system_cache = self.large_system_cache;
        self.config.memory_compression = self.memory_compression;
        self.config.disable_8dot3_name = self.disable_8dot3_name;
        self.config.trim_enabled = self.trim_enabled;
        self.config.wait_to_kill_service = self.wait_to_kill_service;
        self.config.menu_show_delay_zero = self.menu_show_delay_zero;
        self.config.startup_delay_zero = self.startup_delay_zero;
        self.config.system_responsiveness_zero = self.system_responsiveness_zero;
        self.config.tcp_ecn_enabled = self.tcp_ecn_enabled;
        self.config.disable_telemetry = self.disable_telemetry;

        save_config(&self.config);
    }

    pub fn apply_all_criteria(&mut self) {
        // Nhóm 1: CPU & Điều độ Tiến trình (7)
        let _ = set_cpu_max_state_ac(self.cpu_limit_ac);
        let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
        let _ = crate::power::set_cpu_min_state(self.cpu_min_state_pct);
        let _ = crate::power::set_win32_priority_separation(self.win32_priority_boost);
        let _ = crate::power::set_fast_shutdown_recovery(self.fast_shutdown_recovery);

        // Nhóm 2: Bộ nhớ RAM & Bộ nhớ ảo (4)
        self.autonomous.hardware_profile.dynamic_ram_threshold_pct = self.ram_trim_threshold_pct;
        let _ = crate::power::set_large_system_cache(self.large_system_cache);
        let _ = crate::power::set_memory_compression(self.memory_compression);
        if self.autonomous.auto_ram_enabled {
            let (freed, _) = trim_working_sets(&mut self.sys);
            if freed > 0 {
                self.autonomous.log("[RAM]", format!("Thu hồi tức thì {} RAM.", format_bytes(freed)));
            }
        }

        // Nhóm 3: Lưu trữ & Ổ đĩa SSD/HDD (6)
        let _ = crate::power::set_sysmain_service(!self.sysmain_disabled);
        let _ = crate::power::set_ntfs_last_access(self.disable_ntfs_last_access);
        let _ = crate::power::set_ntfs_8dot3_name(self.disable_8dot3_name);
        let _ = crate::power::set_ssd_trim(self.trim_enabled);
        let _ = crate::power::set_wait_to_kill_service(self.wait_to_kill_service);

        // Nhóm 4: Đồ họa, Màn hình & Độ trễ (5)
        set_timer_resolution_1ms(self.timer_1ms_enabled);
        let _ = set_gamedvr(self.gamedvr_enabled);
        let _ = crate::power::set_menu_show_delay(self.menu_show_delay_zero);
        let _ = crate::power::set_startup_delay(self.startup_delay_zero);
        if self.brightness_level > 0 {
            let _ = set_brightness(self.brightness_level);
        }

        // Nhóm 5: Mạng & Kết nối TCP/IP (4)
        let _ = crate::power::set_tcp_optimization(self.tcp_autotuning);
        let _ = crate::power::set_mmcss_gaming_responsiveness(self.mmcss_optimized);
        let _ = crate::power::set_tcp_ecn(self.tcp_ecn_enabled);

        // Nhóm 6: Nguồn điện, Pin & Tản nhiệt (4)
        let _ = set_cooling_policy(self.cooling_active);
        let _ = set_battery_threshold(self.protection.battery_threshold_enabled, 80);
        set_keep_awake(self.keep_awake_mode);
        if self.protection.is_hibernate_enabled {
            let _ = enable_hibernate_as_admin();
        }

        // Nhóm 7: Dịch vụ ngầm & Telemetry (3)
        let _ = crate::startup::toggle_telemetry(self.disable_telemetry);

        crate::power::apply_permanent_windows_system_tweaks();

        self.sync_config();
        let msg = "✓ Đã áp dụng toàn bộ 33 thông số tối ưu vào hệ điều hành!".to_string();
        self.criteria_apply_msg = msg.clone();
        self.quick_opt_msg = msg;
        self.autonomous.log("[CÀI ĐẶT]", "Đã áp dụng và ghi thành công 33 thông số tối ưu vào Windows.".to_string());
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

            // Run autonomous engine tick and persist if state changed
            let state_changed = self.autonomous.tick(
                &mut self.sys,
                self.power_status.is_ac_online,
                self.power_status.battery_percent,
                self.mem_stats.load_percent,
            );
            if state_changed {
                self.sync_config();
            }

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

        // --- TOP BAR: Logo + License + Theme ---
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("⚡ OmniTune")
                    .size(20.0)
                    .strong()
                    .color(p.accent_blue),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
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

        // --- HEALTH & HERO ACTION CARD ---
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

        // --- 4 PHÂN VÙNG TỐI ƯU HỆ THỐNG (CPU • RAM • PIN • DRIVE) ---
        ui.horizontal(|ui| {
            ui.label(RichText::new("4 PHÂN VÙNG TỐI ƯU HỆ THỐNG").size(12.5).strong().color(p.accent_blue));
            ui.label(RichText::new("(Bấm phân vùng để xem và tinh chỉnh thông số)").size(10.5).color(p.text_secondary));
        });
        ui.add_space(2.0);

        let total_junk: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();

        // Row 1: CPU (Col 0) & RAM (Col 1)
        ui.columns(2, |cols| {
            // Card 1: CPU
            let is_cpu = self.selected_partition == Some(PartitionTab::Cpu);
            let cpu_card_bg = if is_cpu {
                if p.is_dark { Color32::from_rgb(26, 42, 72) } else { Color32::from_rgb(225, 239, 255) }
            } else {
                p.card_bg
            };
            let cpu_stroke = if is_cpu { Stroke::new(2.0, p.accent_blue) } else { Stroke::new(1.0, p.card_border) };

            egui::Frame::new()
                .inner_margin(egui::Margin::same(10))
                .corner_radius(CornerRadius::same(8))
                .fill(cpu_card_bg)
                .stroke(cpu_stroke)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("⚡ CPU").size(12.5).strong().color(p.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("17 thông số").size(10.0).strong().color(p.accent_blue));
                        });
                    });
                    ui.add_space(2.0);

                    let cpu = self.protection.cpu_usage;
                    let cpu_color = if cpu > 80.0 { p.danger_red } else { p.accent_blue };
                    let cpu_text = if self.initial_scan_done {
                        format!("{:.1}%", cpu)
                    } else {
                        "⟳ Đang đo".to_string()
                    };
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(cpu_text).size(22.0).strong().color(cpu_color));
                        let display_cpu = self.autonomous.hardware_profile.cpu_brand
                            .replace("(R)", "")
                            .replace("(TM)", "")
                            .replace("CPU", "")
                            .trim()
                            .chars()
                            .take(16)
                            .collect::<String>();
                        ui.label(RichText::new(display_cpu).size(11.0).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, (cpu / 100.0) as f32, cpu_color, p.tab_inactive_bg);
                    ui.add_space(4.0);

                    let cur_cpu_mhz = self.sys.cpus().first().map(|c| c.frequency()).unwrap_or(self.autonomous.hardware_profile.cpu_base_mhz as u64) as u32;
                    let full_cpu = self.autonomous.hardware_profile.cpu_brand
                        .replace("(R)", "")
                        .replace("(TM)", "")
                        .replace("CPU", "")
                        .trim()
                        .to_string();

                    ui.label(RichText::new(format!("• Vi xử lý: {}", full_cpu)).size(10.0).strong().color(p.text_primary));
                    ui.label(RichText::new(format!("• Cấu trúc: {} Nhân thực / {} Luồng", self.autonomous.hardware_profile.cpu_physical_cores, self.autonomous.hardware_profile.cpu_logical_cores)).size(9.8).color(p.text_secondary));
                    ui.label(RichText::new(format!("• Xung nhịp: Cơ sở {} MHz • Hiện tại {} MHz", self.autonomous.hardware_profile.cpu_base_mhz, cur_cpu_mhz)).size(9.8).color(p.text_secondary));
                    ui.label(RichText::new(format!("• Giới hạn Turbo: AC {}% | DC {}% (Sàn {}%)", self.cpu_limit_ac, self.cpu_limit_dc, self.cpu_min_state_pct)).size(9.8).color(p.text_secondary));

                    ui.add_space(6.0);
                    let btn_txt = if is_cpu { "▲ Thu gọn" } else { "⚙ Cài đặt (17 thông số) ▾" };
                    let btn_fill = if is_cpu { p.primary_btn } else { p.tab_inactive_bg };
                    let btn = egui::Button::new(RichText::new(btn_txt).size(10.5).strong().color(if is_cpu { Color32::WHITE } else { p.text_primary }))
                        .fill(btn_fill)
                        .min_size(Vec2::new(ui.available_width(), 26.0))
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(btn).clicked() {
                        self.selected_partition = if is_cpu { None } else { Some(PartitionTab::Cpu) };
                    }
                });

            // Card 2: RAM
            let is_ram = self.selected_partition == Some(PartitionTab::Ram);
            let ram_card_bg = if is_ram {
                if p.is_dark { Color32::from_rgb(26, 42, 72) } else { Color32::from_rgb(225, 239, 255) }
            } else {
                p.card_bg
            };
            let ram_stroke = if is_ram { Stroke::new(2.0, p.accent_blue) } else { Stroke::new(1.0, p.card_border) };

            egui::Frame::new()
                .inner_margin(egui::Margin::same(10))
                .corner_radius(CornerRadius::same(8))
                .fill(ram_card_bg)
                .stroke(ram_stroke)
                .show(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🧠 RAM").size(12.5).strong().color(p.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("4 thông số").size(10.0).strong().color(p.accent_blue));
                        });
                    });
                    ui.add_space(2.0);

                    let load = self.mem_stats.load_percent;
                    let ram_color = if load > 85 { p.danger_red } else { p.success_green };
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(format!("{}%", load)).size(22.0).strong().color(ram_color));
                        ui.label(RichText::new(format!("Dùng {} / {:.1} GB", format_bytes(self.mem_stats.used_bytes), self.autonomous.hardware_profile.ram_total_gb)).size(11.0).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, (load as f32) / 100.0, ram_color, p.tab_inactive_bg);
                    ui.add_space(4.0);

                    let pf_used = self.mem_stats.pagefile_used_bytes;
                    let pf_total = self.mem_stats.pagefile_total_bytes;
                    ui.label(RichText::new(format!("• RAM vật lý: Dùng {} / {:.1} GB", format_bytes(self.mem_stats.used_bytes), self.autonomous.hardware_profile.ram_total_gb)).size(10.0).strong().color(p.text_primary));
                    ui.label(RichText::new(format!("• Khả dụng: {} trống ({:.0}%)", format_bytes(self.mem_stats.free_bytes), (100.0 - load as f32).max(0.0))).size(9.8).color(p.text_secondary));
                    if pf_total > 0 {
                        ui.label(RichText::new(format!("• Bộ nhớ ảo (Pagefile): {} / {}", format_bytes(pf_used), format_bytes(pf_total))).size(9.8).color(p.text_secondary));
                    } else {
                        ui.label(RichText::new(format!("• Nén RAM: {}", if self.memory_compression { "Đang bật (Giảm swap)" } else { "Tắt" })).size(9.8).color(p.text_secondary));
                    }
                    ui.label(RichText::new(format!("• Auto Trim: Ngưỡng {}% • Nén RAM: {}", self.ram_trim_threshold_pct, if self.memory_compression { "Bật" } else { "Tắt" })).size(9.8).color(p.text_secondary));

                    ui.add_space(6.0);
                    let btn_txt = if is_ram { "▲ Thu gọn" } else { "⚙ Cài đặt (4 thông số) ▾" };
                    let btn_fill = if is_ram { p.primary_btn } else { p.tab_inactive_bg };
                    let btn = egui::Button::new(RichText::new(btn_txt).size(10.5).strong().color(if is_ram { Color32::WHITE } else { p.text_primary }))
                        .fill(btn_fill)
                        .min_size(Vec2::new(ui.available_width(), 26.0))
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(btn).clicked() {
                        self.selected_partition = if is_ram { None } else { Some(PartitionTab::Ram) };
                    }
                });
        });

        ui.add_space(6.0);

        // Row 2: PIN (Col 0) & DRIVE (Col 1)
        ui.columns(2, |cols| {
            // Card 3: PIN
            let is_pin = self.selected_partition == Some(PartitionTab::Pin);
            let pin_card_bg = if is_pin {
                if p.is_dark { Color32::from_rgb(26, 42, 72) } else { Color32::from_rgb(225, 239, 255) }
            } else {
                p.card_bg
            };
            let pin_stroke = if is_pin { Stroke::new(2.0, p.accent_blue) } else { Stroke::new(1.0, p.card_border) };

            egui::Frame::new()
                .inner_margin(egui::Margin::same(10))
                .corner_radius(CornerRadius::same(8))
                .fill(pin_card_bg)
                .stroke(pin_stroke)
                .show(&mut cols[0], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("🔋 PIN / NGUỒN").size(12.5).strong().color(p.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("6 thông số").size(10.0).strong().color(p.accent_blue));
                        });
                    });
                    ui.add_space(2.0);

                    if self.power_status.has_battery {
                        let bat = self.power_status.battery_percent;
                        let bat_color = if bat < 20 { p.danger_red } else if bat < 50 { p.warn_amber } else { p.success_green };
                        let pwr_str = if self.power_status.discharge_rate_mw > 0 {
                            format!("{:.1}W xả", self.power_status.power_watts)
                        } else if self.power_status.charge_rate_mw > 0 {
                            format!("{:.1}W nạp", self.power_status.power_watts)
                        } else if self.power_status.is_ac_online {
                            "Cắm sạc AC".to_string()
                        } else {
                            "Dùng pin".to_string()
                        };
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new(format!("{}%", bat)).size(22.0).strong().color(bat_color));
                            let volt_str = if self.power_status.voltage_volts > 0.0 {
                                format!("{} • {:.1}V", pwr_str, self.power_status.voltage_volts)
                            } else {
                                pwr_str
                            };
                            ui.label(RichText::new(volt_str).size(11.0).strong().color(p.text_primary));
                        });

                        render_mini_bar(ui, (bat as f32) / 100.0, bat_color, p.tab_inactive_bg);
                        ui.add_space(4.0);

                        let num_bats = self.dual_batteries.len();
                        if num_bats >= 2 {
                            let b1 = &self.dual_batteries[0];
                            let b2 = &self.dual_batteries[1];
                            let tot_design = (b1.design_capacity_mwh + b2.design_capacity_mwh) as f32 / 1000.0;
                            let tot_full = (b1.full_charge_capacity_mwh + b2.full_charge_capacity_mwh) as f32 / 1000.0;
                            let avg_wear = if tot_design > 0.0 {
                                ((1.0 - (tot_full / tot_design)) * 100.0).clamp(0.0, 100.0)
                            } else {
                                0.0
                            };
                            ui.label(RichText::new(format!("• Số lượng: 2 pin (Pin kép) • Chai TB: {:.1}%", avg_wear)).size(10.0).strong().color(p.text_primary));
                            ui.label(RichText::new(format!("• Pin 1 ({}): Chai {:.1}% • {} chu kỳ ({}%)", b1.id, b1.wear_percent, b1.cycle_count, b1.current_charge_percent)).size(9.8).color(p.text_secondary));
                            ui.label(RichText::new(format!("• Pin 2 ({}): Chai {:.1}% • {} chu kỳ ({}%)", b2.id, b2.wear_percent, b2.cycle_count, b2.current_charge_percent)).size(9.8).color(p.text_secondary));
                            ui.label(RichText::new(format!("• Dung lượng: {:.1} Wh sạc đầy / {:.1} Wh gốc", tot_full, tot_design)).size(9.8).color(p.text_secondary));
                        } else if num_bats == 1 {
                            let b = &self.dual_batteries[0];
                            let health = (100.0 - b.wear_percent).clamp(0.0, 100.0);
                            let full_wh = b.full_charge_capacity_mwh as f32 / 1000.0;
                            let design_wh = b.design_capacity_mwh as f32 / 1000.0;
                            ui.label(RichText::new(format!("• Số lượng: 1 pin ({})", b.id)).size(10.0).strong().color(p.text_primary));
                            ui.label(RichText::new(format!("• Mức độ chai: {:.1}% (Độ khỏe pin: {:.1}%)", b.wear_percent, health)).size(9.8).color(p.text_secondary));
                            ui.label(RichText::new(format!("• Chu kỳ sạc: {} lần • {:.1} / {:.1} Wh", b.cycle_count, full_wh, design_wh)).size(9.8).color(p.text_secondary));
                            let bat_care_str = if self.protection.battery_threshold_enabled { "Khóa 80% (Bảo vệ pin)" } else { "100% Sạc đầy" };
                            ui.label(RichText::new(format!("• Ngưỡng sạc: {}", bat_care_str)).size(9.8).color(p.text_secondary));
                        } else {
                            ui.label(RichText::new("• Số lượng: 1 pin (Đang nhận diện mức chai...)").size(10.0).strong().color(p.text_primary));
                            ui.label(RichText::new(format!("• Trạng thái: {}", if self.power_status.is_ac_online { "Đang cắm nguồn AC" } else { "Đang dùng pin" })).size(9.8).color(p.text_secondary));
                            let bat_care_str = if self.protection.battery_threshold_enabled { "Khóa 80% (Bảo vệ pin)" } else { "100% Sạc đầy" };
                            ui.label(RichText::new(format!("• Chế độ sạc: {}", bat_care_str)).size(9.8).color(p.text_secondary));
                            ui.label(RichText::new(format!("• Quạt làm mát: {}", if self.cooling_active { "Active" } else { "Passive (0dB)" })).size(9.8).color(p.text_secondary));
                        }
                    } else {
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new("Nguồn AC").size(22.0).strong().color(p.accent_blue));
                            ui.label(RichText::new("PC để bàn • 220V").size(11.0).strong().color(p.text_primary));
                        });
                        render_mini_bar(ui, 1.0, p.accent_blue, p.tab_inactive_bg);
                        ui.add_space(4.0);
                        ui.label(RichText::new("• Số lượng pin: 0 (Nguồn AC trực tiếp)").size(10.0).strong().color(p.text_primary));
                        ui.label(RichText::new("• Mức chai: Không áp dụng (PC không pin)").size(9.8).color(p.text_secondary));
                        ui.label(RichText::new("• Cung cấp điện: Nguồn AC trực tiếp 220V").size(9.8).color(p.text_secondary));
                        ui.label(RichText::new(format!("• Quạt tản nhiệt: {}", if self.cooling_active { "Active (Làm mát tối đa)" } else { "Passive (0dB)" })).size(9.8).color(p.text_secondary));
                    }

                    ui.add_space(6.0);
                    let btn_txt = if is_pin { "▲ Thu gọn" } else { "⚙ Cài đặt (6 thông số) ▾" };
                    let btn_fill = if is_pin { p.primary_btn } else { p.tab_inactive_bg };
                    let btn = egui::Button::new(RichText::new(btn_txt).size(10.5).strong().color(if is_pin { Color32::WHITE } else { p.text_primary }))
                        .fill(btn_fill)
                        .min_size(Vec2::new(ui.available_width(), 26.0))
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(btn).clicked() {
                        self.selected_partition = if is_pin { None } else { Some(PartitionTab::Pin) };
                    }
                });

            // Card 4: DRIVE
            let is_drv = self.selected_partition == Some(PartitionTab::Drive);
            let drv_card_bg = if is_drv {
                if p.is_dark { Color32::from_rgb(26, 42, 72) } else { Color32::from_rgb(225, 239, 255) }
            } else {
                p.card_bg
            };
            let drv_stroke = if is_drv { Stroke::new(2.0, p.accent_blue) } else { Stroke::new(1.0, p.card_border) };

            egui::Frame::new()
                .inner_margin(egui::Margin::same(10))
                .corner_radius(CornerRadius::same(8))
                .fill(drv_card_bg)
                .stroke(drv_stroke)
                .show(&mut cols[1], |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("💾 Ổ ĐĨA / DRIVE").size(12.5).strong().color(p.text_primary));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.label(RichText::new("6 thông số").size(10.0).strong().color(p.accent_blue));
                        });
                    });
                    ui.add_space(2.0);

                    let drv_type = if self.autonomous.hardware_profile.is_ssd { "SSD Siêu tốc" } else { "HDD Cơ học" };
                    let d_total = self.autonomous.hardware_profile.disk_total_bytes;
                    let d_avail = self.autonomous.hardware_profile.disk_available_bytes;
                    let d_used = d_total.saturating_sub(d_avail);
                    let frac = if d_total > 0 { (d_used as f32 / d_total as f32).clamp(0.0, 1.0) } else { 0.5 };
                    let drv_pct = (frac * 100.0) as u32;

                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(format!("{}%", drv_pct)).size(22.0).strong().color(p.accent_blue));
                        ui.label(RichText::new(format!("{} • {}", drv_type, format_bytes(d_total))).size(11.0).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, frac, p.accent_blue, p.tab_inactive_bg);
                    ui.add_space(4.0);

                    let clean_disk = self.autonomous.hardware_profile.disk_name.chars().take(24).collect::<String>();
                    ui.label(RichText::new(format!("• Thiết bị: {}", clean_disk)).size(10.0).strong().color(p.text_primary));
                    if d_total > 0 {
                        ui.label(RichText::new(format!("• Dung lượng: Dùng {} / Trống {}", format_bytes(d_used), format_bytes(d_avail))).size(9.8).color(p.text_secondary));
                    }
                    let trim_str = if self.trim_enabled { "Bật" } else { "Tắt" };
                    ui.label(RichText::new(format!("• Định dạng: {} • SSD TRIM: {}", self.autonomous.hardware_profile.disk_fs, trim_str)).size(9.8).color(p.text_secondary));
                    ui.label(RichText::new(format!("• Rác hệ thống: {} (Quét 3 tầng)", format_bytes(total_junk))).size(9.8).color(p.text_secondary));

                    ui.add_space(6.0);
                    let btn_txt = if is_drv { "▲ Thu gọn" } else { "⚙ Cài đặt (6 thông số) ▾" };
                    let btn_fill = if is_drv { p.primary_btn } else { p.tab_inactive_bg };
                    let btn = egui::Button::new(RichText::new(btn_txt).size(10.5).strong().color(if is_drv { Color32::WHITE } else { p.text_primary }))
                        .fill(btn_fill)
                        .min_size(Vec2::new(ui.available_width(), 26.0))
                        .corner_radius(CornerRadius::same(5));
                    if ui.add(btn).clicked() {
                        self.selected_partition = if is_drv { None } else { Some(PartitionTab::Drive) };
                    }
                });
        });

        // --- EXPANDABLE SETTINGS CARD FOR SELECTED PARTITION ---
        if let Some(active) = self.selected_partition {
            ui.add_space(8.0);
            p.modern_card(ui, |ui| {
                ui.horizontal(|ui| {
                    let (part_icon, part_title, count) = match active {
                        PartitionTab::Cpu => ("⚡", "PHÂN VÙNG CPU & ĐIỀU ĐỘ TIẾN TRÌNH", 17),
                        PartitionTab::Ram => ("🧠", "PHÂN VÙNG BỘ NHỚ RAM & CACHE", 4),
                        PartitionTab::Pin => ("🔋", "PHÂN VÙNG PIN, NGUỒN ĐIỆN & TẢN NHIỆT", 6),
                        PartitionTab::Drive => ("💾", "PHÂN VÙNG LƯU TRỮ & Ổ ĐĨA SSD/HDD", 6),
                    };
                    ui.label(RichText::new(format!("{} CÀI ĐẶT {} ({} THÔNG SỐ)", part_icon, part_title, count)).size(12.5).strong().color(p.accent_blue));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button(RichText::new("✕ Đóng cài đặt").size(11.0).strong()).clicked() {
                            self.selected_partition = None;
                        }
                    });
                });
                ui.separator();
                ui.add_space(4.0);

                match active {
                    PartitionTab::Cpu => self.render_cpu_partition_settings(ui),
                    PartitionTab::Ram => self.render_ram_partition_settings(ui),
                    PartitionTab::Pin => self.render_pin_partition_settings(ui),
                    PartitionTab::Drive => self.render_drive_partition_settings(ui),
                }

                ui.add_space(8.0);
                ui.separator();
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let apply_btn = egui::Button::new(
                        RichText::new("⚡ ÁP DỤNG CÀI ĐẶT PHÂN VÙNG NÀY VÀO WINDOWS")
                            .size(11.5)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(p.primary_btn)
                    .corner_radius(CornerRadius::same(5));
                    if ui.add(apply_btn).clicked() {
                        self.apply_all_criteria();
                    }
                    if !self.criteria_apply_msg.is_empty() {
                        ui.label(RichText::new(&self.criteria_apply_msg).size(11.0).strong().color(p.success_green));
                    }
                });
            });
        }

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
                    if ui.checkbox(
                        &mut self.autonomous.eco_qos_enabled,
                        RichText::new("EcoQoS Bật").size(11.5).strong().color(p.accent_blue),
                    ).changed() {
                        self.sync_config();
                    }
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
                    if ui.checkbox(
                        &mut self.autonomous.auto_pilot_enabled,
                        RichText::new("Auto-Pilot").size(11.0).strong().color(p.success_green),
                    ).changed() {
                        self.sync_config();
                    }
                    ui.add_space(6.0);
                    if ui.checkbox(
                        &mut self.autonomous.auto_ram_enabled,
                        RichText::new("Auto-RAM").size(11.0).strong().color(p.accent_blue),
                    ).changed() {
                        self.sync_config();
                    }
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

        // --- CHỌN CHẾ ĐỘ TỐI ƯU HỆ THỐNG ---
        ui.label(RichText::new("CHẾ ĐỘ TỐI ƯU HỆ THỐNG HIỆN TẠI").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        p.modern_card(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                let is_hw = self.autonomous.operating_mode == OperatingMode::HardwareOptimal;
                let hw_btn = egui::Button::new(
                    RichText::new(if is_hw { "● 1. Tối ưu Phần Cứng & OS (Theo máy thực tế)" } else { "○ 1. Tối ưu Phần Cứng & OS" })
                        .size(11.5)
                        .strong()
                        .color(if is_hw { Color32::WHITE } else { p.text_primary }),
                )
                .fill(if is_hw { p.primary_btn } else { p.tab_inactive_bg })
                .corner_radius(CornerRadius::same(5));

                if ui.add(hw_btn).clicked() {
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
                    RichText::new(if is_wl { "● 2. Tự Động Theo Công Việc (Auto-Pilot)" } else { "○ 2. Tự Động Theo Công Việc" })
                        .size(11.5)
                        .strong()
                        .color(if is_wl { Color32::WHITE } else { p.text_primary }),
                )
                .fill(if is_wl { p.accent_blue } else { p.tab_inactive_bg })
                .corner_radius(CornerRadius::same(5));

                if ui.add(wl_btn).clicked() {
                    self.autonomous.set_operating_mode(
                        OperatingMode::WorkloadAdaptive,
                        &mut self.sys,
                        self.power_status.is_ac_online,
                        self.power_status.battery_percent,
                    );
                    self.sync_config();
                }
            });
        });

        ui.add_space(8.0);

        // --- PHẦN 1: BẢNG MA TRẬN 33 THÔNG SỐ TỐI ƯU HỆ THỐNG ---
        ui.label(RichText::new("1. MA TRẬN 33 THÔNG SỐ TỐI ƯU HỆ THỐNG & PHẦN CỨNG").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_all_parameters_grid(ui);

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

    fn render_all_parameters_grid(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Header Master Action
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                let apply_all_btn = egui::Button::new(
                    RichText::new("⚡ ÁP DỤNG TOÀN BỘ 33 THÔNG SỐ VÀO CÀI ĐẶT & WINDOWS")
                        .size(12.0)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(6));

                if ui.add(apply_all_btn).clicked() {
                    self.apply_all_criteria();
                }

                if !self.criteria_apply_msg.is_empty() {
                    ui.label(RichText::new(&self.criteria_apply_msg).size(11.5).strong().color(p.success_green));
                }
            });
        });

        ui.add_space(8.0);

        // PHÂN VÙNG 1: CPU (17 THÔNG SỐ)
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("⚡ PHÂN VÙNG 1: CPU & ĐIỀU ĐỘ TIẾN TRÌNH (17 THÔNG SỐ)").size(12.0).strong().color(p.accent_blue));
            });
            ui.separator();
            self.render_cpu_partition_settings(ui);
        });

        ui.add_space(8.0);

        // PHÂN VÙNG 2: RAM (4 THÔNG SỐ)
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🧠 PHÂN VÙNG 2: BỘ NHỚ RAM & CACHE (4 THÔNG SỐ)").size(12.0).strong().color(p.accent_blue));
            });
            ui.separator();
            self.render_ram_partition_settings(ui);
        });

        ui.add_space(8.0);

        // PHÂN VÙNG 3: PIN / NGUỒN (6 THÔNG SỐ)
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("🔋 PHÂN VÙNG 3: PIN, NGUỒN ĐIỆN & TẢN NHIỆT (6 THÔNG SỐ)").size(12.0).strong().color(p.accent_blue));
            });
            ui.separator();
            self.render_pin_partition_settings(ui);
        });

        ui.add_space(8.0);

        // PHÂN VÙNG 4: DRIVE / Ổ ĐĨA (6 THÔNG SỐ)
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("💾 PHÂN VÙNG 4: LƯU TRỮ, Ổ ĐĨA SSD/HDD & RÁC (6 THÔNG SỐ)").size(12.0).strong().color(p.accent_blue));
            });
            ui.separator();
            self.render_drive_partition_settings(ui);
        });
    }

    fn render_cpu_partition_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Grid::new("grid_part_cpu")
            .striped(true)
            .min_col_width(135.0)
            .max_col_width(380.0)
            .spacing(Vec2::new(14.0, 8.0))
            .show(ui, |ui| {
                // 1. CPU Max AC
                ui.vertical(|ui| {
                    ui.label(RichText::new("1. Xung trần CPU cắm sạc (AC)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Mức xung tối đa khi dùng nguồn AC").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
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
                    ui.label(RichText::new(format!("💡 Kiến nghị: 100% để mở khóa xung nhịp Turbo tối đa cho {} nhân.", self.autonomous.hardware_profile.cpu_logical_cores)).size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 2. CPU Min State
                ui.vertical(|ui| {
                    ui.label(RichText::new("2. Xung sàn CPU nhàn rỗi (Min State)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Hạ xung sâu khi không có tải").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut self.cpu_min_state_pct, 1..=50).suffix("%")).changed() {
                            let _ = crate::power::set_cpu_min_state(self.cpu_min_state_pct);
                            self.sync_config();
                        }
                        if ui.button(RichText::new("5% Chuẩn").size(10.0)).clicked() {
                            self.cpu_min_state_pct = 5;
                            let _ = crate::power::set_cpu_min_state(5);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: 5% cho phép CPU giảm nhiệt và tiêu thụ điện tối thiểu khi nghỉ.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 3. Win32PrioritySeparation
                ui.vertical(|ui| {
                    ui.label(RichText::new("3. Win32PrioritySeparation (Quantum)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Cấp 3x thời lượng CPU cho app active").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.win32_priority_boost) {
                            crate::power::set_win32_priority_separation(self.win32_priority_boost);
                            self.sync_config();
                        }
                        ui.label(RichText::new(if self.win32_priority_boost { "38 (3x Quantum)" } else { "2 (Mặc định)" }).size(10.5).color(p.accent_blue));
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để ứng dụng cửa sổ đang làm việc chạy mượt mà nhất.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 4. EcoQoS / Efficiency Mode
                ui.vertical(|ui| {
                    ui.label(RichText::new("4. EcoQoS / Efficiency Mode").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Ép tiến trình nền vào lõi E-Core").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.autonomous.eco_qos_enabled) {
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để tiết kiệm điện và giảm nghẽn luồng cho ứng dụng chính.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 5. Foreground Priority Boost
                ui.vertical(|ui| {
                    ui.label(RichText::new("5. Foreground Priority Boost").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Nâng ưu tiên tiến trình cửa sổ đang mở").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.autonomous.auto_pilot_enabled) {
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để tự động tăng quyền ưu tiên CPU (AboveNormal) cho app active.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 6. Fast Shutdown & HungApp Recovery
                ui.vertical(|ui| {
                    ui.label(RichText::new("6. Fast Shutdown & HungApp Recovery").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Tự động giải phóng ứng dụng treo đơ").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.fast_shutdown_recovery) {
                            crate::power::set_fast_shutdown_recovery(self.fast_shutdown_recovery);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (HungApp: 1000ms, AutoEndTasks: 1) chống đơ app và tắt máy nhanh.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 7. Timer Resolution
                ui.vertical(|ui| {
                    ui.label(RichText::new("7. High Precision Timer (1.0ms vs 15.6ms)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Triệt tiêu độ trễ chuột & giật FPS").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.timer_1ms_enabled, "● 1.0 ms (Tối ưu)", "○ 15.6 ms (Mặc định)") {
                            set_timer_resolution_1ms(self.timer_1ms_enabled);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: 1.0ms triệt tiêu giật khung hình FPS và trễ chuột; 15.6ms tiết kiệm pin.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 8. MMCSS Gaming
                ui.vertical(|ui| {
                    ui.label(RichText::new("8. MMCSS Gaming Responsiveness").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("NetworkThrottlingIndex: 0xFFFFFFFF").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.mmcss_optimized) {
                            crate::power::set_mmcss_gaming_responsiveness(self.mmcss_optimized);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (NetworkThrottling: 0xFFFFFFFF) triệt tiêu bóp mạng khi nghe nhạc/game.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 9. SystemResponsiveness 0
                ui.vertical(|ui| {
                    ui.label(RichText::new("9. SystemResponsiveness 0 (Không dự trữ)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Dành 100% tài nguyên CPU cho app active").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.system_responsiveness_zero) {
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (0%) dành 100% tài nguyên xử lý cho ứng dụng trực tiếp, xóa dự trữ 20%.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 10. GameDVR
                ui.vertical(|ui| {
                    ui.label(RichText::new("10. Vô hiệu hóa GameDVR & AppCapture").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Giải phóng encoder GPU nền").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.gamedvr_enabled, "● BẬT (Đã Tắt DVR)", "○ TẮT (Chưa tắt DVR)") {
                            let _ = set_gamedvr(self.gamedvr_enabled);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: TẮT GameDVR để giải phóng bộ mã hóa GPU encoder, tăng độ mượt khi dùng máy.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 11. MenuShowDelay
                ui.vertical(|ui| {
                    ui.label(RichText::new("11. Triệt tiêu trễ Menu (MenuShowDelay: 0ms)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Mở menu chuột phải và Start tức thì").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.menu_show_delay_zero) {
                            crate::power::set_menu_show_delay(self.menu_show_delay_zero);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (0ms) loại bỏ thời gian trễ 400ms của menu chuột phải và thanh tác vụ.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 12. StartupDelayInMSec
                ui.vertical(|ui| {
                    ui.label(RichText::new("12. Bỏ delay khởi động Explorer (0ms)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Không trì hoãn nạp app khi vào desktop").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.startup_delay_zero) {
                            crate::power::set_startup_delay(self.startup_delay_zero);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (0ms) loại bỏ 10 giây chờ tải app khi khởi động vào màn hình chính.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 13. TCP Auto-Tuning
                ui.vertical(|ui| {
                    ui.label(RichText::new("13. TCP Window Auto-Tuning").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Tự động mở rộng buffer tải file").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.tcp_autotuning) {
                            crate::power::set_tcp_optimization(self.tcp_autotuning);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (mức normal) để card mạng tự động mở rộng buffer, đạt băng thông tối đa.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 14. TCP ECN
                ui.vertical(|ui| {
                    ui.label(RichText::new("14. TCP ECN Capability (Báo nghẽn mạng)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Giảm mất gói tin và giật lag ping").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.tcp_ecn_enabled) {
                            crate::power::set_tcp_ecn(self.tcp_ecn_enabled);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để router điều tiết lưu lượng không gây rớt gói tin, giảm ping họp/game.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 15. DiagTrack Telemetry
                ui.vertical(|ui| {
                    ui.label(RichText::new("15. Chặn Telemetry DiagTrack của Windows").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Dừng thu thập dữ liệu chẩn đoán").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.disable_telemetry, "● ĐÃ CHẶN", "○ ĐANG BẬT") {
                            crate::startup::toggle_telemetry(self.disable_telemetry);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (Chặn) để ngắt việc âm thầm gửi dữ liệu chẩn đoán về máy chủ Microsoft.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 16. dmwappushservice
                ui.vertical(|ui| {
                    ui.label(RichText::new("16. Tắt dịch vụ WAP Push Service").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Vô hiệu hóa cổng nhận lệnh thu thập từ xa").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.disable_dmwappush, "● ĐÃ TẮT", "○ ĐANG BẬT") {
                            crate::startup::toggle_telemetry(self.disable_dmwappush);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (Tắt) để vô hiệu hóa cổng nhận lệnh thu thập dữ liệu ngầm từ xa.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 17. Startup Apps
                ui.vertical(|ui| {
                    ui.label(RichText::new("17. Tinh gọn Ứng dụng Khởi động").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Kiểm soát ứng dụng chạy cùng Windows").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Đã nạp: {} ứng dụng startup", self.startup_apps.len())).size(11.0).color(p.accent_blue));
                    });
                    ui.label(RichText::new("💡 Kiến nghị: Tắt các app chạy ngầm không cần thiết để khởi động Windows dưới 10 giây.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();
            });
    }

    fn render_ram_partition_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Grid::new("grid_part_ram")
            .striped(true)
            .min_col_width(135.0)
            .max_col_width(380.0)
            .spacing(Vec2::new(14.0, 8.0))
            .show(ui, |ui| {
                // 1. Auto RAM Trim
                ui.vertical(|ui| {
                    ui.label(RichText::new("1. Tự động thu hồi RAM (Auto Trim)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Dọn dẹp Working Set tiến trình nền").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.autonomous.auto_ram_enabled) {
                            self.sync_config();
                        }
                        if ui.button(RichText::new("⚡ Thu hồi ngay").size(10.5)).clicked() {
                            let (freed, count) = trim_working_sets(&mut self.sys);
                            self.criteria_apply_msg = format!("✓ Thu hồi {} từ {} tiến trình.", format_bytes(freed), count);
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để thu hồi bộ nhớ đệm rò rỉ khi RAM đạt ngưỡng.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 2. Ngưỡng RAM tự động thu hồi
                ui.vertical(|ui| {
                    ui.label(RichText::new("2. Ngưỡng RAM tự động thu hồi").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new(format!("Dung lượng máy: {:.1} GB", self.autonomous.hardware_profile.ram_total_gb)).size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut self.ram_trim_threshold_pct, 60..=95).suffix("%")).changed() {
                            self.autonomous.hardware_profile.dynamic_ram_threshold_pct = self.ram_trim_threshold_pct;
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new(format!("💡 Kiến nghị: 80% (máy {:.1}GB) cân bằng dung lượng và hiệu năng.", self.autonomous.hardware_profile.ram_total_gb)).size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 3. LargeSystemCache
                ui.vertical(|ui| {
                    ui.label(RichText::new("3. LargeSystemCache (Cache tệp)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Mở rộng cache đọc ghi tệp hệ thống").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.large_system_cache) {
                            crate::power::set_large_system_cache(self.large_system_cache);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT (máy >= 8GB RAM) để tăng tốc độ truy xuất tệp hệ điều hành.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 4. Memory Compression
                ui.vertical(|ui| {
                    ui.label(RichText::new("4. Memory Compression (Nén RAM)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Nén trang RAM giảm swap ghi ổ đĩa").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.memory_compression) {
                            crate::power::set_memory_compression(self.memory_compression);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT nén RAM trong bộ nhớ trước khi ghi swap đĩa, bảo vệ SSD.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();
            });
    }

    fn render_pin_partition_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Grid::new("grid_part_pin")
            .striped(true)
            .min_col_width(135.0)
            .max_col_width(380.0)
            .spacing(Vec2::new(14.0, 8.0))
            .show(ui, |ui| {
                // 1. CPU Max DC
                ui.vertical(|ui| {
                    ui.label(RichText::new("1. Xung trần CPU dùng pin (DC)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Mức xung tối đa khi chạy bằng pin").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
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
                    ui.label(RichText::new("💡 Kiến nghị: 70-80% để cân bằng thời lượng pin và độ mượt mà.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 2. Battery Threshold 80%
                ui.vertical(|ui| {
                    ui.label(RichText::new("2. Khóa sạc ngưỡng pin 80% (Battery Care)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Chống chai pin và phồng cell lithium").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.protection.battery_threshold_enabled) {
                            let st = self.protection.battery_threshold_enabled;
                            let _ = set_battery_threshold(st, 80);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT khi cắm sạc bàn làm việc để chống chai pin và phồng cell pin lithium.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 3. Brightness
                ui.vertical(|ui| {
                    ui.label(RichText::new("3. Độ sáng Panel màn hình LED").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Tiết kiệm 30-50% điện năng tiêu thụ").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut self.brightness_level, 10..=100).suffix("%")).changed() {
                            let _ = set_brightness(self.brightness_level);
                            self.sync_config();
                        }
                        if ui.button(RichText::new("45% Pin").size(10.0)).clicked() {
                            self.brightness_level = 45;
                            let _ = set_brightness(45);
                            self.sync_config();
                        }
                        if ui.button(RichText::new("70% Phim").size(10.0)).clicked() {
                            self.brightness_level = 70;
                            let _ = set_brightness(70);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: Pin: 40-45% (tiết kiệm điện tối đa); Văn phòng/sạc: 60-70%.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 4. Cooling Policy
                ui.vertical(|ui| {
                    ui.label(RichText::new("4. Chính sách quạt làm mát").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Chủ động (Active) vs Thụ động (Passive)").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.cooling_active, "● Active (Làm mát)", "○ Passive (0dB êm)") {
                            let _ = set_cooling_policy(self.cooling_active);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: Passive (0dB tĩnh lặng) cho cafe/văn phòng; Active khi render/biên dịch/game.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 5. Keep Awake
                ui.vertical(|ui| {
                    ui.label(RichText::new("5. Chế độ Chống Sleep (Execution State)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Ngăn tắt màn hình và ngủ máy").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        let mut is_keep_awake = self.keep_awake_mode == 2;
                        if render_switch_custom(ui, &mut is_keep_awake, "● Chống Sleep", "○ Sleep OS") {
                            self.keep_awake_mode = if is_keep_awake { 2 } else { 0 };
                            set_keep_awake(self.keep_awake_mode);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT khi tải tệp nặng, render video hoặc remote desktop để máy không tự ngủ.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 6. Hibernate
                ui.vertical(|ui| {
                    ui.label(RichText::new("6. Chế độ Ngủ đông Hibernate").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Ngủ an toàn thay thế Sleep S3 hao pin").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.protection.is_hibernate_enabled) {
                            if self.protection.is_hibernate_enabled {
                                let _ = enable_hibernate_as_admin();
                            }
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để lưu phiên làm việc vào ổ đĩa khi gập máy, triệt tiêu hao pin Sleep S3.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();
            });
    }

    fn render_drive_partition_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        egui::Grid::new("grid_part_drive")
            .striped(true)
            .min_col_width(135.0)
            .max_col_width(380.0)
            .spacing(Vec2::new(14.0, 8.0))
            .show(ui, |ui| {
                // 1. SysMain Service
                ui.vertical(|ui| {
                    ui.label(RichText::new("1. Dịch vụ SysMain (Superfetch)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new(format!("Ổ đĩa: {}", if self.autonomous.hardware_profile.is_ssd { "SSD Tốc độ cao" } else { "HDD Cơ học" })).size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch_custom(ui, &mut self.sysmain_disabled, "● TẮT (Chuẩn SSD)", "○ BẬT (Chuẩn HDD)") {
                            crate::power::set_sysmain_service(!self.sysmain_disabled);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: TẮT với SSD để chống mài mòn cell nhớ flash; BẬT với HDD cơ học.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 2. NTFS DisableLastAccess
                ui.vertical(|ui| {
                    ui.label(RichText::new("2. NTFS DisableLastAccess").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Tắt ghi dấu thời gian truy cập file").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.disable_ntfs_last_access) {
                            crate::power::set_ntfs_last_access(self.disable_ntfs_last_access);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để loại bỏ ghi nhật ký truy cập file liên tục, giảm 30% I/O đĩa.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 3. NTFS Disable8dot3
                ui.vertical(|ui| {
                    ui.label(RichText::new("3. Tắt tên tệp DOS 8.3 (NtfsDisable8dot3)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Bỏ tạo tên ngắn tương thích cũ").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.disable_8dot3_name) {
                            crate::power::set_ntfs_8dot3_name(self.disable_8dot3_name);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT loại bỏ định dạng DOS 8.3, tăng tốc duyệt và tạo file NTFS.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 4. SSD TRIM
                ui.vertical(|ui| {
                    ui.label(RichText::new("4. Kích hoạt SSD TRIM (DeleteNotify: 0)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Chủ động thu hồi khối nhớ SSD đã xóa").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.trim_enabled) {
                            crate::power::set_ssd_trim(self.trim_enabled);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT đảm bảo SSD được thông báo dọn dẹp các khối nhớ rác sau xóa.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 5. WaitToKillServiceTimeout
                ui.vertical(|ui| {
                    ui.label(RichText::new("5. Giảm chờ tắt Service (WaitToKill: 2s)").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Rút ngắn thời gian tắt máy từ 12s còn 2s").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if render_switch(ui, &mut self.wait_to_kill_service) {
                            crate::power::set_wait_to_kill_service(self.wait_to_kill_service);
                            self.sync_config();
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: BẬT để giảm thời gian chờ tắt service từ 5-12s xuống 2s khi shutdown.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();

                // 6. Dọn rác 3 tầng
                ui.vertical(|ui| {
                    ui.label(RichText::new("6. Dọn rác 3 tầng & Cache hệ thống").size(11.5).strong().color(p.text_primary));
                    ui.label(RichText::new("Temp, Prefetch, Log và Thumbnail").size(10.0).color(p.text_secondary));
                });
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("⚡ Dọn Rác Tầng 1").size(11.0).color(Color32::WHITE)).clicked() {
                            let targets = get_default_targets();
                            let mut freed = 0u64;
                            for target in &targets {
                                if target.tier == CleanTier::Safe {
                                    let (f, _) = clean_target(target);
                                    freed += f;
                                }
                            }
                            self.criteria_apply_msg = format!("✓ Đã dọn sạch {} rác an toàn.", format_bytes(freed));
                        }
                    });
                    ui.label(RichText::new("💡 Kiến nghị: Dọn dẹp định kỳ Tầng 1 an toàn để thu hồi dung lượng trống cho ổ C:.").size(10.0).color(p.text_secondary));
                });
                ui.end_row();
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
                self.apply_all_criteria();
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
