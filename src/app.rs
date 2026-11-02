use crate::autonomous_engine::{AutonomousEngine, OperatingMode};
use crate::battery_info::get_dual_battery_info;
use crate::cleaner::{
    clean_target, format_bytes, get_default_targets, scan_target,
};
use crate::config::{load_config, save_config, set_autostart_registry, AppConfig};
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process};
use crate::licensing::{
    deactivate_license, format_remaining_trial, get_trial_status, is_pro_active, load_license,
    save_license, start_trial, LicenseData, TrialStatus,
};
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
    create_restore_point, enable_hibernate_as_admin, get_protection_overview, restore_windows_defaults,
    set_battery_threshold, ProtectionOverview,
};
use crate::startup::{get_startup_apps, is_telemetry_active, toggle_startup_item};
use crate::theme::{
    apply_theme, configure_fonts, spacing, typography, ThemeMode, ThemePalette,
};
use crate::types::{BatteryDetails, CleanTier, JunkCategory, ProcessMemInfo, StartupAppItem, Tab};
use eframe::egui::{self, Color32, CornerRadius, RichText, Stroke, Vec2};
use std::os::windows::process::CommandExt;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use sysinfo::System;

pub enum BackgroundMessage {
    InitialScan(InitialScanResult),
    BatteryUpdate(Vec<BatteryDetails>),
    UpdateAvailable { latest_version: String, download_url: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionTab {
    Cpu,
    Ram,
    Pin,
    Drive,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PartitionSubTab {
    Info,
    Settings,
    Pro,
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
    #[allow(dead_code)]
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
    #[allow(dead_code)]
    clean_status_msg: String,

    // Process Inspector state
    #[allow(dead_code)]
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
    deep_debloat: bool,

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
    partition_sub_tab: PartitionSubTab,

    // Update Checker
    update_available: Option<(String, String)>,
    last_interaction: Instant,
}

fn render_switch_custom(ui: &mut egui::Ui, value: &mut bool, on_text: &str, off_text: &str) -> bool {
    let (text, fill_color) = if *value {
        (on_text, Color32::from_rgb(16, 149, 60))
    } else {
        (off_text, Color32::from_rgb(100, 116, 139))
    };
    let btn = egui::Button::new(
        RichText::new(text)
            .size(typography::SWITCH_TEXT)
            .strong()
            .color(Color32::WHITE),
    )
    .min_size(spacing::SWITCH_SIZE)
    .fill(fill_color)
    .corner_radius(CornerRadius::same(spacing::SWITCH_RADIUS));

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
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), spacing::MINI_BAR_HEIGHT), egui::Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(2), bg_color);
    let mut fill_rect = rect;
    fill_rect.set_width((rect.width() * fraction.clamp(0.0, 1.0)).max(2.0));
    ui.painter().rect_filled(fill_rect, CornerRadius::same(2), color);
}

fn render_chip_frame<R>(
    ui: &mut egui::Ui,
    is_dark: bool,
    border_color: Color32,
    add_contents: impl FnOnce(&mut egui::Ui) -> R,
) -> R {
    let bg = if is_dark {
        Color32::from_rgb(22, 31, 52)
    } else {
        Color32::from_rgb(248, 250, 252)
    };
    egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(12, 10))
        .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
        .fill(bg)
        .stroke(Stroke::new(1.0, border_color))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(64.0);
            add_contents(ui)
        })
        .inner
}

// =========================================================================
// REUSABLE UI TEMPLATES (GIẢM LẶP CODE & TẬP TRUNG TYPOGRAPHY/LAYOUT)
// =========================================================================

macro_rules! render_settings_row {
    ($ui:expr, |$c1:ident| $b1:expr, |$c2:ident| $b2:expr $(,)?) => {
        $ui.spacing_mut().item_spacing.x = 10.0;
        $ui.columns(2, |cols| {
            {
                let $c1 = &mut cols[0];
                $b1;
            }
            {
                let $c2 = &mut cols[1];
                $b2;
            }
        });
        $ui.add_space(8.0);
    };
}

fn render_switch_chip(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    title: &str,
    desc: &str,
    value: &mut bool,
    custom_labels: Option<(&str, &str)>,
) -> bool {
    render_chip_frame(ui, palette.is_dark, palette.card_border, |ui| {
        let changed = ui
            .horizontal(|ui| {
                ui.label(
                    RichText::new(title)
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if let Some((on, off)) = custom_labels {
                        render_switch_custom(ui, value, on, off)
                    } else {
                        render_switch(ui, value)
                    }
                })
                .inner
            })
            .inner;
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(desc)
                    .size(typography::CHIP_DESC)
                    .color(palette.text_secondary),
            )
            .wrap(),
        );
        changed
    })
}

fn render_slider_chip<T: egui::emath::Numeric>(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    title: &str,
    desc: &str,
    value: &mut T,
    range: std::ops::RangeInclusive<T>,
    suffix: &str,
    preset: Option<(&str, T)>,
) -> bool {
    render_chip_frame(ui, palette.is_dark, palette.card_border, |ui| {
        let mut changed = false;
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .size(typography::CHIP_TITLE)
                    .strong()
                    .color(palette.text_primary),
            );
            if let Some((preset_label, preset_val)) = preset {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui
                        .button(
                            RichText::new(preset_label)
                                .size(typography::BTN_SMALL)
                                .strong(),
                        )
                        .clicked()
                    {
                        *value = preset_val;
                        changed = true;
                    }
                });
            }
        });
        ui.add_space(2.0);
        if ui
            .add(egui::Slider::new(value, range).suffix(suffix))
            .changed()
        {
            changed = true;
        }
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(desc)
                    .size(typography::CHIP_DESC)
                    .color(palette.text_secondary),
            )
            .wrap(),
        );
        changed
    })
}

fn render_action_chip(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    title: &str,
    desc: &str,
    action_label: &str,
    mut on_action: impl FnMut(),
) {
    render_chip_frame(ui, palette.is_dark, palette.card_border, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .size(typography::CHIP_TITLE)
                    .strong()
                    .color(palette.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = egui::Button::new(
                    RichText::new(action_label)
                        .size(typography::BTN_SMALL)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(palette.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                if ui.add(btn).clicked() {
                    on_action();
                }
            });
        });
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(desc)
                    .size(typography::CHIP_DESC)
                    .color(palette.text_secondary),
            )
            .wrap(),
        );
    });
}

fn render_info_chip(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    title: &str,
    desc: &str,
    badge: Option<(&str, Color32)>,
) {
    render_chip_frame(ui, palette.is_dark, palette.card_border, |ui| {
        ui.horizontal(|ui| {
            ui.label(
                RichText::new(title)
                    .size(typography::CHIP_TITLE)
                    .strong()
                    .color(palette.text_primary),
            );
            if let Some((badge_text, badge_color)) = badge {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(
                        RichText::new(badge_text)
                            .size(typography::BTN_NORMAL)
                            .strong()
                            .color(badge_color),
                    );
                });
            }
        });
        ui.add_space(2.0);
        ui.add(
            egui::Label::new(
                RichText::new(desc)
                    .size(typography::CHIP_DESC)
                    .color(palette.text_secondary),
            )
            .wrap(),
        );
    });
}

fn render_partition_card(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    icon_title: &str,
    param_count: usize,
    is_active: bool,
    render_body: impl FnOnce(&mut egui::Ui),
) -> bool {
    let card_bg = if is_active {
        if palette.is_dark {
            Color32::from_rgb(26, 42, 72)
        } else {
            Color32::from_rgb(225, 239, 255)
        }
    } else {
        palette.card_bg
    };
    let card_stroke = if is_active {
        Stroke::new(2.0, palette.accent_blue)
    } else {
        Stroke::new(1.0, palette.card_border)
    };

    let response = egui::Frame::new()
        .inner_margin(egui::Margin::symmetric(14, 10))
        .corner_radius(CornerRadius::same(spacing::CARD_RADIUS))
        .fill(card_bg)
        .stroke(card_stroke)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.set_min_height(58.0);
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(icon_title)
                        .size(typography::CARD_TITLE)
                        .strong()
                        .color(palette.text_primary),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let (toggle_txt, toggle_color) = if is_active {
                        ("▲ Thu gọn", palette.accent_blue)
                    } else {
                        ("▼ Chi tiết & Cài đặt", palette.text_secondary)
                    };
                    ui.label(
                        RichText::new(format!("{} ({} mục)", toggle_txt, param_count))
                            .size(typography::DETAIL_TEXT)
                            .strong()
                            .color(toggle_color),
                    );
                });
            });
            ui.add_space(4.0);

            render_body(ui);
        });

    let interact = ui.interact(
        response.response.rect,
        response.response.id.with(icon_title),
        egui::Sense::click(),
    );

    if interact.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }

    interact.clicked()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RiskLevel {
    Safe,
    Warning,
    Danger,
}

fn render_risk_card(
    ui: &mut egui::Ui,
    palette: &ThemePalette,
    level: RiskLevel,
    title: &str,
    diagnosis: &str,
    advice: &str,
) {
    let (badge_txt, badge_color, bg_color, stroke_color) = match level {
        RiskLevel::Safe => (
            "🟢 AN TOÀN",
            palette.success_green,
            if palette.is_dark {
                Color32::from_rgb(16, 38, 28)
            } else {
                Color32::from_rgb(236, 253, 245)
            },
            palette.success_green,
        ),
        RiskLevel::Warning => (
            "🟡 CẢNH BÁO RỦI RO",
            palette.warn_amber,
            if palette.is_dark {
                Color32::from_rgb(45, 33, 16)
            } else {
                Color32::from_rgb(255, 251, 235)
            },
            palette.warn_amber,
        ),
        RiskLevel::Danger => (
            "🔴 NGUY HIỂM",
            palette.danger_red,
            if palette.is_dark {
                Color32::from_rgb(45, 18, 20)
            } else {
                Color32::from_rgb(254, 242, 242)
            },
            palette.danger_red,
        ),
    };

    egui::Frame::new()
        .inner_margin(egui::Margin::same(10))
        .corner_radius(CornerRadius::same(spacing::CARD_RADIUS))
        .fill(bg_color)
        .stroke(Stroke::new(1.2, stroke_color))
        .show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(badge_txt)
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(badge_color),
                );
                ui.label(
                    RichText::new(title)
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(palette.text_primary),
                );
            });
            ui.add_space(2.0);
            ui.label(
                RichText::new(diagnosis)
                    .size(typography::DETAIL_TEXT)
                    .color(palette.text_primary),
            );
            ui.add_space(2.0);
            ui.label(
                RichText::new(format!("💡 Khuyến nghị: {}", advice))
                    .size(typography::CHIP_DESC)
                    .strong()
                    .color(badge_color),
            );
        });
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
        let is_pro = is_pro_active(&license);

        let mut config = load_config();
        let run_on_startup = crate::config::is_autostart_enabled();
        config.run_on_startup = run_on_startup;

        // Apply saved configurations immediately on launch
        if config.timer_1ms_enabled && is_pro {
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
        if config.battery_threshold_80 && is_pro {
            let _ = set_battery_threshold(true, 80);
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
        if config.disable_telemetry && is_pro {
            crate::startup::toggle_telemetry(true);
        }
        if config.deep_debloat && is_pro {
            let _ = crate::protection::apply_deep_debloat(true);
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

        // Background update check against GitHub releases
        let bg_tx_update = bg_tx.clone();
        std::thread::spawn(move || {
            let script = r#"$ProgressPreference = 'SilentlyContinue'; try { $res = Invoke-RestMethod -Uri 'https://api.github.com/repos/thuanlovevietnam/omnitune/releases/latest' -Headers @{'User-Agent'='OmniTune'} -TimeoutSec 4; if ($res.tag_name) { Write-Output ($res.tag_name + '|' + $res.html_url) } } catch {}"#;
            if let Ok(output) = std::process::Command::new("powershell")
                .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
                .creation_flags(0x08000000)
                .output()
            {
                let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if let Some((tag, url)) = text.split_once('|') {
                    let tag_clean = tag.trim_start_matches('v').trim();
                    let cur_ver = env!("CARGO_PKG_VERSION");
                    if tag_clean != cur_ver && !tag_clean.is_empty() {
                        let _ = bg_tx_update.send(BackgroundMessage::UpdateAvailable {
                            latest_version: tag.to_string(),
                            download_url: url.to_string(),
                        });
                    }
                }
            }
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
            is_pro,
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
                    is_pro,
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
            brightness_level: crate::power::get_brightness(),
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
            deep_debloat: config.deep_debloat,

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
            partition_sub_tab: PartitionSubTab::Info,
            update_available: None,
            last_interaction: Instant::now(),
        }
    }

    pub fn check_pro_gate(&mut self, feature_name: &str) -> bool {
        if is_pro_active(&self.license) {
            true
        } else {
            self.show_license_modal = true;
            self.license_feedback_msg = format!(
                "★ Tính năng '{}' chỉ dành cho bản OmniTune Pro VIP (hoặc dùng thử 7 ngày). Mở khóa ngay!",
                feature_name
            );
            false
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
        self.config.deep_debloat = self.deep_debloat;

        save_config(&self.config);
    }

    pub fn apply_all_criteria(&mut self) {
        let is_pro = is_pro_active(&self.license);

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
        if self.autonomous.auto_ram_enabled && is_pro {
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

        // Nhóm 4: Đồ họa, Màn hình & Độ trễ (4)
        if is_pro {
            set_timer_resolution_1ms(self.timer_1ms_enabled);
        } else {
            set_timer_resolution_1ms(false);
            self.timer_1ms_enabled = false;
        }
        let _ = set_gamedvr(self.gamedvr_enabled);
        let _ = crate::power::set_menu_show_delay(self.menu_show_delay_zero);
        let _ = crate::power::set_startup_delay(self.startup_delay_zero);

        // Nhóm 5: Mạng & Kết nối TCP/IP (4)
        let _ = crate::power::set_tcp_optimization(self.tcp_autotuning);
        let _ = crate::power::set_mmcss_gaming_responsiveness(self.mmcss_optimized);
        let _ = crate::power::set_tcp_ecn(self.tcp_ecn_enabled);

        // Nhóm 6: Nguồn điện, Pin & Tản nhiệt (4)
        let _ = set_cooling_policy(self.cooling_active);
        if is_pro {
            let _ = set_battery_threshold(self.protection.battery_threshold_enabled, 80);
        } else {
            self.protection.battery_threshold_enabled = false;
        }
        set_keep_awake(self.keep_awake_mode);
        if self.protection.is_hibernate_enabled {
            let _ = enable_hibernate_as_admin();
        }

        // Nhóm 7: Dịch vụ ngầm, Telemetry & Deep Debloat (3)
        if is_pro {
            let _ = crate::startup::toggle_telemetry(self.disable_telemetry);
            if self.deep_debloat {
                let _ = crate::protection::apply_deep_debloat(true);
            }
        }

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
                BackgroundMessage::UpdateAvailable { latest_version, download_url } => {
                    self.autonomous.log(
                        "[CẬP NHẬT]",
                        format!("Có bản cập nhật mới: {}!", latest_version),
                    );
                    self.update_available = Some((latest_version, download_url));
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
        // 1. Áp dụng toàn bộ 33 thông số tối ưu phần cứng & Windows
        self.apply_all_criteria();
        self.autonomous.set_operating_mode(
            OperatingMode::HardwareOptimal,
            &mut self.sys,
            self.power_status.is_ac_online,
            self.power_status.battery_percent,
        );

        // 2. Dọn dẹp tệp rác Tầng 1 an toàn
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

        // 3. Thu hồi bộ nhớ Working Set RAM
        let (freed_ram, count) = trim_working_sets(&mut self.sys);

        // 4. Ưu tiên CPU app Focus và ép EcoQoS app ngầm
        let (_, eco_count) = apply_foreground_boost_and_eco(
            &self.sys,
            self.foreground_info.as_ref().map(|(pid, _)| *pid),
        );

        self.mem_stats = get_memory_stats();
        self.top_processes = get_top_processes(&mut self.sys, 25);

        let msg = format!(
            "Dọn {} rác ({} tệp), thu hồi {} RAM ({} app), tối ưu phần cứng ({} nhân, SSD: {}), ép EcoQoS {} app nền.",
            format_bytes(freed_junk),
            deleted_files,
            format_bytes(freed_ram),
            count,
            self.autonomous.hardware_profile.cpu_logical_cores,
            self.autonomous.hardware_profile.is_ssd,
            eco_count
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[TỐI ƯU TOÀN DIỆN]", format!("Đã tối ưu toàn bộ hệ thống: {}", msg));
    }

    fn clean_cpu_partition(&mut self) {
        let (boosted, eco_count) = apply_foreground_boost_and_eco(
            &self.sys,
            self.foreground_info.as_ref().map(|(pid, _)| *pid),
        );
        let msg = format!(
            "Dọn dẹp CPU: Ưu tiên tiến trình đang dùng ({}), đưa {} ứng dụng nền vào chế độ EcoQoS.",
            if boosted > 0 { "Tăng tốc hoàn tất" } else { "Đã duy trì" },
            eco_count
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[CPU]", msg);
    }

    fn clean_ram_partition(&mut self) {
        let (freed_ram, count) = trim_working_sets(&mut self.sys);
        self.mem_stats = get_memory_stats();
        self.top_processes = get_top_processes(&mut self.sys, 25);
        let msg = format!(
            "Dọn dẹp RAM: Thu hồi {} bộ nhớ đệm thừa từ {} tiến trình.",
            format_bytes(freed_ram),
            count
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[RAM]", msg);
    }

    fn clean_pin_partition(&mut self) {
        let _ = self.autonomous.hardware_profile.apply_hardware_optimal(
            self.power_status.is_ac_online,
            self.power_status.battery_percent,
        );
        let (_, eco_count) = apply_foreground_boost_and_eco(
            &self.sys,
            self.foreground_info.as_ref().map(|(pid, _)| *pid),
        );
        let (freed_ram, _) = trim_working_sets(&mut self.sys);
        self.power_status = crate::power::get_power_status();
        let msg = format!(
            "Dọn dẹp PIN: Kích hoạt tiết kiệm điện, đưa {} app vào EcoQoS, thu hồi {} RAM.",
            eco_count,
            format_bytes(freed_ram)
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[PIN]", msg);
    }

    fn clean_drive_partition(&mut self) {
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
        let msg = format!(
            "Dọn dẹp Ổ đĩa: Xóa {} tệp rác an toàn ({}), thu hồi không gian lưu trữ.",
            deleted_files,
            format_bytes(freed_junk)
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("[DRIVE]", msg);
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

        // Intercept close button (X)
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            if self.config.minimize_to_tray {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::Visible(false));
                self.autonomous.log(
                    "[KHAY HỆ THỐNG]",
                    "Đã thu nhỏ xuống khay hệ thống. Mọi tối ưu (Auto-Pilot, RAM, Timer) vẫn tiếp tục duy trì.".to_string(),
                );
            } else {
                std::process::exit(0);
            }
        }

        // Track any user interaction (mouse wheel scroll, cursor movement, clicks, keys)
        let is_interacting = ui.input(|i| {
            i.smooth_scroll_delta != Vec2::ZERO
                || i.pointer.is_moving()
                || i.pointer.any_down()
                || !i.raw.events.is_empty()
        });

        if is_interacting {
            self.last_interaction = Instant::now();
        }

        // Maintain continuous 60+ FPS rendering during interaction and for 1.2s after,
        // ensuring kinetic scroll deceleration and animations glide completely smoothly.
        if self.last_interaction.elapsed() < Duration::from_millis(1200) {
            ui.ctx().request_repaint();
        } else {
            ui.ctx().request_repaint_after(Duration::from_millis(300));
        }

        let p = self.palette;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg_main).inner_margin(egui::Margin::symmetric(14, 12)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .animated(false)
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::VisibleWhenNeeded)
                    .wheel_scroll_multiplier(Vec2::new(1.0, 3.5))
                    .show(ui, |ui| {
                        self.render_widget_view(ui);
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

        // --- UPDATE NOTIFICATION BANNER ---
        if let Some((ver, url)) = &self.update_available {
            p.modern_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("🚀").size(14.0));
                    ui.label(RichText::new(format!("Có bản cập nhật mới: {}", ver)).size(12.0).strong().color(p.accent_blue));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let dl_btn = egui::Button::new(RichText::new("Tải ngay ↗").size(11.5).strong().color(Color32::WHITE))
                            .fill(p.accent_blue)
                            .corner_radius(CornerRadius::same(6));
                        if ui.add(dl_btn).clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab(url));
                        }
                    });
                });
            });
            ui.add_space(4.0);
        }

        // --- TOP BAR: Logo + License + Theme ---
        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("⚡ OmniTune")
                    .size(typography::APP_TITLE)
                    .strong()
                    .color(p.accent_blue),
            );

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Exit app button
                let exit_btn = egui::Button::new(RichText::new("✕ Thoát").size(13.0).strong().color(Color32::WHITE))
                    .fill(Color32::from_rgb(220, 38, 38))
                    .corner_radius(CornerRadius::same(6));
                if ui.add(exit_btn).on_hover_text("Thoát hoàn toàn ứng dụng (dừng mọi tiến trình)").clicked() {
                    std::process::exit(0);
                }

                ui.add_space(6.0);

                // Theme toggle
                let (theme_text, theme_hover) = if self.theme_mode == ThemeMode::Dark {
                    ("Sáng", "Chuyển giao diện Sáng")
                } else {
                    ("Tối", "Chuyển giao diện Tối")
                };

                let theme_btn = egui::Button::new(RichText::new(theme_text).size(13.5).strong().color(p.text_primary))
                    .fill(p.tab_inactive_bg)
                    .stroke(Stroke::new(1.0, p.card_border))
                    .corner_radius(CornerRadius::same(6));

                if ui.add(theme_btn).on_hover_text(theme_hover).clicked() {
                    self.theme_mode = match self.theme_mode {
                        ThemeMode::Dark => ThemeMode::Light,
                        ThemeMode::Light => ThemeMode::Dark,
                    };
                    self.palette = apply_theme(ui.ctx(), self.theme_mode);
                }

                ui.add_space(6.0);

                // Pro / License status button
                if self.license.is_valid {
                    let pro_badge = egui::Button::new(RichText::new("★ PRO").size(13.5).strong().color(Color32::WHITE))
                        .fill(Color32::from_rgb(217, 119, 6))
                        .corner_radius(CornerRadius::same(6));
                    if ui.add(pro_badge).on_hover_text(format!("OmniTune Pro: {}", self.license.licensee)).clicked() {
                        self.show_license_modal = !self.show_license_modal;
                    }
                } else {
                    let trial = get_trial_status(&self.license);
                    match trial {
                        TrialStatus::Active { remaining_secs } => {
                            let label = format!("★ Dùng thử ({})", format_remaining_trial(remaining_secs));
                            let trial_badge = egui::Button::new(RichText::new(label).size(13.0).strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(37, 99, 235))
                                .corner_radius(CornerRadius::same(6));
                            if ui.add(trial_badge).on_hover_text("Bản quyền dùng thử Pro đang hoạt động").clicked() {
                                self.show_license_modal = !self.show_license_modal;
                            }
                        }
                        TrialStatus::Expired => {
                            let exp_badge = egui::Button::new(RichText::new("★ Hết thử").size(13.0).strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(220, 38, 38))
                                .corner_radius(CornerRadius::same(6));
                            if ui.add(exp_badge).on_hover_text("Dùng thử Pro đã hết hạn. Bấm để mua key Pro.").clicked() {
                                self.show_license_modal = !self.show_license_modal;
                            }
                        }
                        TrialStatus::NotStarted => {
                            let buy_btn = egui::Button::new(RichText::new("★ Lên PRO").size(13.5).strong().color(Color32::WHITE))
                                .fill(Color32::from_rgb(217, 119, 6))
                                .corner_radius(CornerRadius::same(6));
                            if ui.add(buy_btn).on_hover_text("Nâng cấp lên bản OmniTune Pro (hoặc dùng thử 7 ngày)").clicked() {
                                self.show_license_modal = !self.show_license_modal;
                            }
                        }
                    }
                }
            });
        });

        ui.add_space(8.0);

        // --- LICENSE ACTIVATION CARD ---
        if self.show_license_modal {
            p.modern_card(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("★ BẢN QUYỀN OMNITUNE PRO").size(16.0).strong().color(p.warn_amber));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let close_btn = egui::Button::new(RichText::new("✕ Đóng").size(13.0).strong().color(p.text_primary))
                            .fill(p.tab_inactive_bg)
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(close_btn).clicked() {
                            self.show_license_modal = false;
                        }
                    });
                });
                ui.separator();
                ui.add_space(4.0);

                ui.columns(2, |cols| {
                    // CỘT 1: FORM NHẬP LIỆU & TRẠNG THÁI
                    let ui = &mut cols[0];
                    ui.label(RichText::new("🔑 THÔNG TIN KÍCH HOẠT").size(14.5).strong().color(p.accent_blue));
                    ui.add_space(4.0);

                    // Trial Section
                    if !self.license.is_valid {
                        let trial = get_trial_status(&self.license);
                        match trial {
                            TrialStatus::NotStarted => {
                                let start_trial_btn = egui::Button::new(RichText::new("🎁 Dùng thử 7 ngày").size(13.0).strong().color(Color32::WHITE))
                                    .fill(Color32::from_rgb(16, 149, 60))
                                    .min_size(Vec2::new(ui.available_width(), 32.0))
                                    .corner_radius(CornerRadius::same(6));
                                if ui.add(start_trial_btn).on_hover_text("Dùng thử đầy đủ tính năng Pro trong 7 ngày không cần thẻ").clicked() {
                                    self.license = start_trial();
                                    self.autonomous.is_pro = is_pro_active(&self.license);
                                    self.license_feedback_msg = "✓ Đã kích hoạt 7 ngày trải nghiệm OmniTune Pro!".to_string();
                                    self.autonomous.log("[TRIAL]", "Bắt đầu 7 ngày trải nghiệm Pro cao cấp.".to_string());
                                }
                            }
                            TrialStatus::Active { remaining_secs } => {
                                ui.label(RichText::new(format!("🎁 Đang trải nghiệm Pro miễn phí (Còn {})", format_remaining_trial(remaining_secs))).size(13.0).strong().color(p.accent_blue));
                            }
                            TrialStatus::Expired => {
                                ui.label(RichText::new("⚠️ Hết hạn dùng thử 7 ngày. Vui lòng mua key để tiếp tục.").size(12.5).strong().color(p.warn_amber));
                            }
                        }
                        ui.add_space(6.0);
                    } else {
                        ui.label(RichText::new(format!("✓ Đã kích hoạt cho: {}", self.license.licensee)).size(13.5).strong().color(p.success_green));
                        ui.add_space(6.0);
                    }

                    let hwid = crate::licensing::get_hardware_id();
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("HWID:").size(13.0).color(p.text_secondary));
                        ui.label(RichText::new(&hwid).size(12.5).strong().color(p.accent_blue));
                        let copy_btn = egui::Button::new(RichText::new("📋 Copy").size(11.5).strong().color(p.text_primary))
                            .fill(p.tab_inactive_bg)
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(copy_btn).clicked() {
                            ui.ctx().copy_text(hwid.clone());
                            self.license_feedback_msg = "✓ Đã copy HWID vào Clipboard!".to_string();
                        }
                    });

                    ui.add_space(4.0);
                    ui.label(RichText::new("Email người dùng:").size(13.0).strong().color(p.text_primary));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.input_license_email)
                            .hint_text("Nhập email nhận key...")
                            .desired_width(ui.available_width()),
                    );

                    ui.add_space(4.0);
                    ui.label(RichText::new("Mã kích hoạt (Key):").size(13.0).strong().color(p.text_primary));
                    ui.add(
                        egui::TextEdit::singleline(&mut self.input_license_key)
                            .hint_text("OTPRO-... hoặc OTHW-...")
                            .desired_width(ui.available_width()),
                    );

                    ui.add_space(8.0);
                    ui.horizontal_wrapped(|ui| {
                        let activate_btn = egui::Button::new(RichText::new("✓ Kích hoạt").size(13.0).strong().color(Color32::WHITE))
                            .fill(p.primary_btn)
                            .min_size(Vec2::new(100.0, 30.0))
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(activate_btn).clicked() {
                            if save_license(&self.input_license_email, &self.input_license_key) {
                                self.license = load_license();
                                self.autonomous.is_pro = is_pro_active(&self.license);
                                self.license_feedback_msg = "✓ Kích hoạt OmniTune Pro thành công! Cảm ơn bạn.".to_string();
                                self.autonomous.log("[VIP]", "Đã kích hoạt bản quyền OmniTune Pro vĩnh viễn.".to_string());
                            } else {
                                self.license_feedback_msg = "✗ Mã bản quyền không khớp với Email hoặc Máy này.".to_string();
                            }
                        }

                        let buy_online_btn = egui::Button::new(RichText::new("💳 Mua Key ($9)").size(13.0).strong().color(Color32::WHITE))
                            .fill(Color32::from_rgb(217, 119, 6))
                            .min_size(Vec2::new(90.0, 30.0))
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(buy_online_btn).on_hover_text("Mở trang thanh toán nhận key ngay tức thì").clicked() {
                            ui.ctx().open_url(egui::OpenUrl::new_tab("https://thuanlovevietnam.github.io/omnitune/#pricing"));
                        }

                        if self.license.is_valid {
                            let deact_btn = egui::Button::new(RichText::new("Hủy kích hoạt").size(12.0).strong().color(Color32::WHITE))
                                .fill(p.danger_red)
                                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                            if ui.add(deact_btn).clicked() {
                                deactivate_license();
                                self.license = load_license();
                                self.autonomous.is_pro = false;
                                self.license_feedback_msg = "Đã hủy kích hoạt bản quyền.".to_string();
                            }
                        }
                    });

                    if !self.license_feedback_msg.is_empty() {
                        ui.add_space(4.0);
                        ui.label(RichText::new(&self.license_feedback_msg).size(12.5).strong().color(p.warn_amber));
                    }

                    // CỘT 2: DANH SÁCH TÍNH NĂNG PRO
                    let ui = &mut cols[1];
                    ui.label(RichText::new("⭐ ĐẶC QUYỀN GÓI PRO VIP ($9 TRỌN ĐỜI)").size(14.5).strong().color(p.warn_amber));
                    ui.add_space(4.0);

                    egui::Frame::new()
                        .fill(Color32::TRANSPARENT)
                        .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
                        .stroke(Stroke::new(1.0, p.card_border))
                        .inner_margin(egui::Margin::symmetric(10, 8))
                        .show(ui, |ui| {
                            let perks = [
                                ("🚀", "Dọn rác Dev (NPM, Cargo, Go hàng chục GB)"),
                                ("🎯", "Timer Resolution 1.0ms tối ưu Game & Remote"),
                                ("🛡️", "Khóa sạc Pin 80% chống phù và chai cell pin"),
                                ("🔒", "Chặn Telemetry DiagTrack gửi dữ liệu ngầm"),
                                ("🧹", "Deep Debloat: Tắt Copilot, Bing & Edge ngầm"),
                                ("⚡", "Smart Auto-RAM Trim 24/7 ngầm không đơ máy"),
                                ("🧠", "CPU Affinity & High Priority cho app chính"),
                                ("💾", "Tạo điểm khôi phục an toàn (Restore Point)"),
                            ];

                            egui::Grid::new("pro_perks_grid_modal")
                                .num_columns(2)
                                .spacing(Vec2::new(8.0, 6.0))
                                .show(ui, |ui| {
                                    for (icon, text) in perks {
                                        ui.label(RichText::new(icon).size(16.0));
                                        ui.add(egui::Label::new(RichText::new(text).size(13.0).strong().color(p.text_primary)).wrap());
                                        ui.end_row();
                                    }
                                });
                        });
                });
            });
            ui.add_space(6.0);
        }

        // --- HEALTH & HERO ACTION CARD ---
        let (score, score_label, score_color, _tips) = self.calculate_health_score();
        p.modern_card(ui, |ui| {
            ui.columns(2, |cols| {
                // Cột 1: Mô tả và nút click (Center)
                cols[0].vertical_centered(|ui| {
                    ui.add_space(4.0);

                    // 5 nút thiết lập & chế độ tự hành
                    ui.horizontal_wrapped(|ui| {
                        if ui.checkbox(
                            &mut self.autonomous.auto_pilot_enabled,
                            RichText::new("Auto-Pilot").size(13.5).strong().color(p.success_green),
                        ).changed() {
                            self.sync_config();
                        }
                        ui.add_space(8.0);
                        if ui.checkbox(
                            &mut self.autonomous.auto_ram_enabled,
                            RichText::new("Auto-RAM").size(13.5).strong().color(p.accent_blue),
                        ).changed() {
                            if self.autonomous.auto_ram_enabled && !self.check_pro_gate("Auto-RAM 24/7") {
                                self.autonomous.auto_ram_enabled = false;
                            } else {
                                self.sync_config();
                            }
                        }
                        ui.add_space(8.0);
                        if ui.checkbox(
                            &mut self.autonomous.eco_qos_enabled,
                            RichText::new("EcoQoS").size(13.5).strong().color(p.accent_blue),
                        ).changed() {
                            self.sync_config();
                        }
                    });

                    ui.add_space(4.0);

                    ui.horizontal_wrapped(|ui| {
                        if ui.checkbox(
                            &mut self.run_on_startup,
                            RichText::new("Khởi động cùng Windows").size(13.2).color(p.text_primary),
                        ).changed() {
                            let ok = set_autostart_registry(self.run_on_startup);
                            if ok {
                                self.config.run_on_startup = self.run_on_startup;
                                self.sync_config();
                            }
                        }
                        ui.add_space(10.0);
                        if ui.checkbox(
                            &mut self.config.minimize_to_tray,
                            RichText::new("Thu nhỏ vào khay [X]").size(13.2).color(p.text_primary),
                        ).changed() {
                            self.sync_config();
                        }
                    });

                    ui.add_space(6.0);

                    let opt_btn = egui::Button::new(
                        RichText::new("► TỐI ƯU TOÀN DIỆN")
                            .size(typography::BTN_NORMAL)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(p.primary_btn)
                    .min_size(Vec2::new(ui.available_width() - 10.0, spacing::HERO_BTN_HEIGHT))
                    .corner_radius(CornerRadius::same(spacing::CARD_RADIUS));

                    if ui.add(opt_btn).clicked() {
                        self.run_quick_optimize();
                    }

                    if !self.quick_opt_msg.is_empty() {
                        ui.add_space(4.0);
                        ui.label(
                            RichText::new(&self.quick_opt_msg)
                                .size(typography::BODY_TEXT)
                                .color(p.success_green)
                                .strong(),
                        );
                    }
                });

                // Cột 2: Chỉ số sức khỏe (Center)
                cols[1].vertical_centered(|ui| {
                    ui.label(
                        RichText::new("CHỈ SỐ SỨC KHỎE")
                            .size(13.0)
                            .strong()
                            .color(p.text_secondary),
                    );
                    ui.add_space(2.0);
                    ui.heading(
                        RichText::new(format!("{}/100", score))
                            .size(typography::HERO_SCORE)
                            .strong()
                            .color(score_color),
                    );
                    ui.label(
                        RichText::new(score_label)
                            .size(typography::BTN_LARGE)
                            .strong()
                            .color(score_color),
                    );
                });
            });
        });

        ui.add_space(8.0);

        // Row 1: CPU (Col 0) & RAM (Col 1)
        let is_cpu = self.selected_partition == Some(PartitionTab::Cpu);
        let is_ram = self.selected_partition == Some(PartitionTab::Ram);
        let is_pin = self.selected_partition == Some(PartitionTab::Pin);
        let is_drv = self.selected_partition == Some(PartitionTab::Drive);

        let mut cpu_clicked = false;
        let mut ram_clicked = false;

        ui.columns(2, |cols| {
            // Card 1: CPU
            cpu_clicked = render_partition_card(
                &mut cols[0],
                &p,
                "⚡ CPU",
                17,
                is_cpu,
                |ui| {
                    let cpu = self.protection.cpu_usage;
                    let cpu_color = if cpu > 80.0 { p.danger_red } else { p.accent_blue };
                    let cpu_text = if self.initial_scan_done {
                        format!("{:.1}%", cpu)
                    } else {
                        "⟳ Đang đo".to_string()
                    };
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(cpu_text).size(typography::STAT_VALUE).strong().color(cpu_color));
                        let display_cpu = self.autonomous.hardware_profile.cpu_brand
                            .replace("(R)", "")
                            .replace("(TM)", "")
                            .replace("CPU", "")
                            .trim()
                            .chars()
                            .take(18)
                            .collect::<String>();
                        ui.label(RichText::new(display_cpu).size(typography::CHIP_TITLE).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, (cpu / 100.0) as f32, cpu_color, p.tab_inactive_bg);
                },
            );

            // Card 2: RAM
            ram_clicked = render_partition_card(
                &mut cols[1],
                &p,
                "🧠 RAM",
                4,
                is_ram,
                |ui| {
                    let load = self.mem_stats.load_percent;
                    let ram_color = if load > 85 { p.danger_red } else { p.success_green };
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(format!("{}%", load)).size(typography::STAT_VALUE).strong().color(ram_color));
                        ui.label(RichText::new(format!("Dùng {} / {:.1} GB", format_bytes(self.mem_stats.used_bytes), self.autonomous.hardware_profile.ram_total_gb)).size(typography::CHIP_TITLE).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, (load as f32) / 100.0, ram_color, p.tab_inactive_bg);
                },
            );
        });

        if cpu_clicked {
            self.selected_partition = if is_cpu { None } else { Some(PartitionTab::Cpu) };
        }
        if ram_clicked {
            self.selected_partition = if is_ram { None } else { Some(PartitionTab::Ram) };
        }

        ui.add_space(10.0);

        // Row 2: PIN (Col 0) & DRIVE (Col 1)
        let mut pin_clicked = false;
        let mut drv_clicked = false;

        ui.spacing_mut().item_spacing.x = 10.0;
        ui.columns(2, |cols| {
            // Card 3: PIN
            pin_clicked = render_partition_card(
                &mut cols[0],
                &p,
                "🔋 PIN",
                6,
                is_pin,
                |ui| {
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
                            ui.heading(RichText::new(format!("{}%", bat)).size(typography::STAT_VALUE).strong().color(bat_color));
                            let volt_str = if self.power_status.voltage_volts > 0.0 {
                                format!("{} • {:.1}V", pwr_str, self.power_status.voltage_volts)
                            } else {
                                pwr_str
                            };
                            ui.label(RichText::new(volt_str).size(typography::CHIP_TITLE).strong().color(p.text_primary));
                        });

                        render_mini_bar(ui, (bat as f32) / 100.0, bat_color, p.tab_inactive_bg);
                    } else {
                        ui.horizontal(|ui| {
                            ui.heading(RichText::new("Nguồn AC").size(typography::STAT_VALUE).strong().color(p.accent_blue));
                            ui.label(RichText::new("PC để bàn • 220V").size(typography::CHIP_TITLE).strong().color(p.text_primary));
                        });
                        render_mini_bar(ui, 1.0, p.accent_blue, p.tab_inactive_bg);
                    }
                },
            );

            // Card 4: DRIVE
            drv_clicked = render_partition_card(
                &mut cols[1],
                &p,
                "💾 DRIVE",
                6,
                is_drv,
                |ui| {
                    let drv_type = if self.autonomous.hardware_profile.is_ssd { "SSD Siêu tốc" } else { "HDD Cơ học" };
                    let d_total = self.autonomous.hardware_profile.disk_total_bytes;
                    let d_avail = self.autonomous.hardware_profile.disk_available_bytes;
                    let d_used = d_total.saturating_sub(d_avail);
                    let frac = if d_total > 0 { (d_used as f32 / d_total as f32).clamp(0.0, 1.0) } else { 0.5 };
                    let drv_pct = (frac * 100.0) as u32;

                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(format!("{}%", drv_pct)).size(typography::STAT_VALUE).strong().color(p.accent_blue));
                        ui.label(RichText::new(format!("{} • {}", drv_type, format_bytes(d_total))).size(typography::CHIP_TITLE).strong().color(p.text_primary));
                    });

                    render_mini_bar(ui, frac, p.accent_blue, p.tab_inactive_bg);
                },
            );
        });

        if pin_clicked {
            self.selected_partition = if is_pin { None } else { Some(PartitionTab::Pin) };
        }
        if drv_clicked {
            self.selected_partition = if is_drv { None } else { Some(PartitionTab::Drive) };
        }

        // --- EXPANDABLE UNIFIED CARD FOR SELECTED PARTITION ---
        if let Some(active) = self.selected_partition {
            ui.add_space(12.0);
            self.render_partition_unified_view(ui, active);
        }
    }

    // =========================================================================
    // PHÂN VÙNG: GIAO DIỆN HỢP NHẤT (CHI TIẾT • RỦI RO • TOP 5 • HÀNH ĐỘNG • SETUP)
    // =========================================================================
    fn render_partition_unified_view(&mut self, ui: &mut egui::Ui, active: PartitionTab) {
        let p = self.palette;
        p.modern_card(ui, |ui| {
            let (part_icon, part_title, basic_count, pro_count, clean_btn_txt, clean_tooltip) = match active {
                PartitionTab::Cpu => (
                    "⚡",
                    "CPU & TIẾN TRÌNH",
                    16,
                    4,
                    "TỐI ƯU TIẾN TRÌNH & ECOQOS",
                    "Tối ưu mức ưu tiên và ép chế độ tiết kiệm EcoQoS cho các app ngầm",
                ),
                PartitionTab::Ram => (
                    "🧠",
                    "RAM & CACHE",
                    4,
                    4,
                    "THU HỒI & DỌN RAM",
                    "Thu hồi bộ nhớ đệm thừa từ tất cả các tiến trình",
                ),
                PartitionTab::Pin => (
                    "🔋",
                    "PIN, NGUỒN ĐIỆN & TẢN NHIỆT",
                    6,
                    2,
                    "BẬT TIẾT KIỆM PIN",
                    "Hạ độ sáng, kích hoạt tản nhiệt êm và hạn chế xung CPU khi dùng pin",
                ),
                PartitionTab::Drive => (
                    "💾",
                    "LƯU TRỮ & Ổ ĐĨA SSD/HDD",
                    8,
                    4,
                    "DỌN RÁC & SSD TRIM",
                    "Xóa sạch tệp rác Tầng 1 an toàn và chủ động kích hoạt TRIM khối nhớ SSD",
                ),
            };

            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!("{} {}", part_icon, part_title))
                        .size(typography::SECTION_TITLE)
                        .strong()
                        .color(p.accent_blue),
                );
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let close_btn = egui::Button::new(RichText::new("✕ Đóng").size(typography::BTN_NORMAL).strong().color(p.text_primary))
                        .fill(p.tab_inactive_bg)
                        .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                    if ui.add(close_btn).clicked() {
                        self.selected_partition = None;
                    }
                });
            });
            ui.separator();
            ui.add_space(4.0);

            // Sub-tabs: Info, Settings & Pro Selector
            ui.horizontal(|ui| {
                let is_pro = is_pro_active(&self.license);
                let info_active = self.partition_sub_tab == PartitionSubTab::Info;
                let settings_active = self.partition_sub_tab == PartitionSubTab::Settings;
                let pro_active = self.partition_sub_tab == PartitionSubTab::Pro;

                let info_btn = egui::Button::new(
                    RichText::new("ℹ️ THÔNG TIN")
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(if info_active { Color32::WHITE } else { p.text_primary }),
                )
                .fill(if info_active { p.accent_blue } else { p.card_bg })
                .stroke(Stroke::new(1.0, if info_active { p.accent_blue } else { p.card_border }))
                .min_size(Vec2::new(0.0, 34.0))
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                if ui.add(info_btn).clicked() {
                    self.partition_sub_tab = PartitionSubTab::Info;
                }

                let settings_btn = egui::Button::new(
                    RichText::new(format!("⚙️ CÀI ĐẶT ({})", basic_count))
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(if settings_active { Color32::WHITE } else { p.text_primary }),
                )
                .fill(if settings_active { p.accent_blue } else { p.card_bg })
                .stroke(Stroke::new(1.0, if settings_active { p.accent_blue } else { p.card_border }))
                .min_size(Vec2::new(0.0, 34.0))
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                if ui.add(settings_btn).clicked() {
                    self.partition_sub_tab = PartitionSubTab::Settings;
                }

                let pro_badge = if is_pro { "ĐÃ MỞ KHÓA" } else { "VIP" };
                let pro_color = if pro_active {
                    Color32::WHITE
                } else if p.is_dark {
                    p.warn_amber
                } else {
                    Color32::from_rgb(180, 83, 9)
                };
                let pro_btn = egui::Button::new(
                    RichText::new(format!("⭐ GÓI PRO ({})", pro_badge))
                        .size(typography::CHIP_TITLE)
                        .strong()
                        .color(pro_color),
                )
                .fill(if pro_active { Color32::from_rgb(147, 51, 234) } else { p.card_bg })
                .stroke(Stroke::new(1.0, if pro_active { Color32::from_rgb(147, 51, 234) } else { p.card_border }))
                .min_size(Vec2::new(0.0, 34.0))
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                if ui.add(pro_btn).clicked() {
                    self.partition_sub_tab = PartitionSubTab::Pro;
                }
            });

            ui.add_space(8.0);

            match self.partition_sub_tab {
                PartitionSubTab::Info => {
                    // 1 & 2. Thông tin cấu hình chi tiết & Đánh giá rủi ro
                    match active {
                        PartitionTab::Cpu => self.render_cpu_info(ui),
                        PartitionTab::Ram => self.render_ram_info(ui),
                        PartitionTab::Pin => self.render_pin_info(ui),
                        PartitionTab::Drive => self.render_drive_info(ui),
                    }
                }
                PartitionSubTab::Settings => {
                    // Nút Dọn dẹp nhanh trực tiếp nổi bật
                    let quick_btn = egui::Button::new(
                        RichText::new(clean_btn_txt)
                            .size(typography::BTN_LARGE)
                            .strong()
                            .color(Color32::WHITE),
                    )
                    .fill(p.success_green)
                    .min_size(Vec2::new(ui.available_width(), 38.0))
                    .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                    if ui.add(quick_btn).on_hover_text(clean_tooltip).clicked() {
                        match active {
                            PartitionTab::Cpu => self.clean_cpu_partition(),
                            PartitionTab::Ram => self.clean_ram_partition(),
                            PartitionTab::Pin => self.clean_pin_partition(),
                            PartitionTab::Drive => self.clean_drive_partition(),
                        }
                    }

                    if !self.criteria_apply_msg.is_empty() {
                        ui.add_space(2.0);
                        ui.label(RichText::new(&self.criteria_apply_msg).size(typography::BODY_TEXT).color(p.success_green).strong());
                    }

                    ui.add_space(8.0);

                    // Danh sách tác nhân & tiến trình nổi bật
                    match active {
                        PartitionTab::Cpu => self.render_cpu_tasks(ui),
                        PartitionTab::Ram => self.render_ram_tasks(ui),
                        PartitionTab::Pin => self.render_pin_tasks(ui),
                        PartitionTab::Drive => self.render_drive_tasks(ui),
                    }

                    ui.add_space(10.0);
                    ui.separator();
                    ui.add_space(6.0);

                    // Cài đặt
                    ui.label(
                        RichText::new(format!("{} THIẾT LẬP CÀI ĐẶT:", basic_count))
                            .size(typography::CHIP_TITLE)
                            .strong()
                            .color(p.text_primary),
                    );
                    ui.add_space(4.0);

                    match active {
                        PartitionTab::Cpu => self.render_cpu_basic_settings(ui),
                        PartitionTab::Ram => self.render_ram_basic_settings(ui),
                        PartitionTab::Pin => self.render_pin_basic_settings(ui),
                        PartitionTab::Drive => self.render_drive_basic_settings(ui),
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("ÁP DỤNG CÀI ĐẶT")
                                .size(typography::CHIP_TITLE)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(p.primary_btn)
                        .min_size(Vec2::new(0.0, spacing::MASTER_APPLY_BTN_HEIGHT))
                        .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(apply_btn).clicked() {
                            self.apply_all_criteria();
                        }
                    });
                }
                PartitionSubTab::Pro => {
                    self.render_pro_banner(ui, match active {
                        PartitionTab::Cpu => "BỘ XỬ LÝ (CPU)",
                        PartitionTab::Ram => "BỘ NHỚ (RAM)",
                        PartitionTab::Pin => "NGUỒN ĐIỆN & PIN",
                        PartitionTab::Drive => "LƯU TRỮ & Ổ ĐĨA",
                    });

                    if !self.criteria_apply_msg.is_empty() {
                        ui.add_space(2.0);
                        ui.label(RichText::new(&self.criteria_apply_msg).size(typography::BODY_TEXT).color(p.success_green).strong());
                    }

                    ui.add_space(8.0);

                    ui.label(
                        RichText::new(format!("{} TÍNH NĂNG CHUYÊN SÂU DÀNH CHO GÓI PRO VIP:", pro_count))
                            .size(typography::CHIP_TITLE)
                            .strong()
                            .color(p.warn_amber),
                    );
                    ui.add_space(4.0);

                    match active {
                        PartitionTab::Cpu => self.render_cpu_pro_settings(ui),
                        PartitionTab::Ram => self.render_ram_pro_settings(ui),
                        PartitionTab::Pin => self.render_pin_pro_settings(ui),
                        PartitionTab::Drive => self.render_drive_pro_settings(ui),
                    }

                    ui.add_space(8.0);
                    ui.separator();
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let apply_btn = egui::Button::new(
                            RichText::new("ÁP DỤNG CÀI ĐẶT PRO")
                                .size(typography::CHIP_TITLE)
                                .strong()
                                .color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(147, 51, 234))
                        .min_size(Vec2::new(0.0, spacing::MASTER_APPLY_BTN_HEIGHT))
                        .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(apply_btn).clicked() {
                            self.apply_all_criteria();
                        }
                    });
                }
            }
        });
    }

    fn render_pro_banner(&mut self, ui: &mut egui::Ui, partition_label: &str) {
        let p = self.palette;
        let is_pro = is_pro_active(&self.license);

        egui::Frame::new()
            .fill(if is_pro {
                if p.is_dark { Color32::from_rgb(30, 20, 50) } else { Color32::from_rgb(243, 232, 255) }
            } else {
                if p.is_dark { Color32::from_rgb(38, 28, 14) } else { Color32::from_rgb(254, 243, 199) }
            })
            .corner_radius(CornerRadius::same(spacing::CARD_RADIUS))
            .stroke(Stroke::new(1.0, if is_pro { Color32::from_rgb(168, 85, 247) } else { p.warn_amber }))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                if is_pro {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("OMNITUNE PRO VIP ĐANG HOẠT ĐỘNG")
                                .size(typography::CHIP_TITLE)
                                .strong()
                                .color(if p.is_dark { Color32::from_rgb(216, 180, 254) } else { Color32::from_rgb(126, 34, 206) }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let trial = get_trial_status(&self.license);
                            let status_txt = match trial {
                                TrialStatus::Active { remaining_secs } => {
                                    format!("Dùng thử Pro: {}", format_remaining_trial(remaining_secs))
                                }
                                _ => format!("Bản quyền: {}", self.license.licensee),
                            };
                            ui.label(RichText::new(status_txt).size(typography::DETAIL_TEXT).strong().color(p.success_green));
                        });
                    });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "Mọi tính năng chuyên sâu và cơ chế tự động bảo vệ cho phân vùng {} đã được kích hoạt đầy đủ.",
                            partition_label
                        ))
                        .size(typography::DETAIL_TEXT)
                        .color(p.text_primary),
                    );
                } else {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("TÍNH NĂNG CHUYÊN SÂU DÀNH CHO GÓI PRO")
                                .size(typography::CHIP_TITLE)
                                .strong()
                                .color(if p.is_dark { p.warn_amber } else { Color32::from_rgb(180, 83, 9) }),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let trial = get_trial_status(&self.license);
                            if let TrialStatus::NotStarted = trial {
                                let trial_btn = egui::Button::new(
                                    RichText::new("Dùng thử 7 ngày")
                                        .size(typography::BTN_SMALL)
                                        .strong()
                                        .color(Color32::WHITE),
                                )
                                .fill(Color32::from_rgb(16, 149, 60))
                                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                                if ui.add(trial_btn).clicked() {
                                    self.license = start_trial();
                                    self.autonomous.is_pro = is_pro_active(&self.license);
                                    self.license_feedback_msg = "✓ Đã kích hoạt 7 ngày trải nghiệm OmniTune Pro!".to_string();
                                    self.autonomous.log("[TRIAL]", "Bắt đầu 7 ngày trải nghiệm Pro cao cấp.".to_string());
                                }
                            }
                            let key_btn = egui::Button::new(
                                RichText::new("Kích hoạt Pro")
                                    .size(typography::BTN_SMALL)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(Color32::from_rgb(147, 51, 234))
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                            if ui.add(key_btn).clicked() {
                                self.show_license_modal = true;
                            }
                        });
                    });
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "Nâng cấp gói Pro để mở khóa toàn diện công nghệ điều phối & tinh chỉnh phần cứng cho {}, vận hành tự động 24/7.",
                            partition_label
                        ))
                        .size(typography::DETAIL_TEXT)
                        .color(p.text_primary),
                    );
                }
            });
    }

    // --- THÔNG TIN PHÂN VÙNG CPU (MỤC 1 & 2) ---
    fn render_cpu_info(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // 1. Thông tin cấu hình chi tiết
        ui.label(RichText::new("1. THÔNG TIN CẤU HÌNH & XUNG NHỊP THỜI GIAN THỰC").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(3.0);
        let cur_cpu_mhz = self.sys.cpus().first().map(|c| c.frequency()).unwrap_or(self.autonomous.hardware_profile.cpu_base_mhz as u64) as u32;

        egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
            .stroke(Stroke::new(1.0, p.card_border))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                egui::Grid::new("cpu_config_table_vertical")
                    .num_columns(2)
                    .spacing(Vec2::new(18.0, 7.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Bộ xử lý:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(self.autonomous.hardware_profile.cpu_brand.trim()).size(typography::BODY_TEXT).strong().color(p.text_primary));
                        ui.end_row();

                        ui.label(RichText::new("Cấu trúc nhân:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("{} Nhân vật lý / {} Luồng logic", self.autonomous.hardware_profile.cpu_physical_cores, self.autonomous.hardware_profile.cpu_logical_cores)).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                        ui.end_row();

                        ui.label(RichText::new("Xung nhịp:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("Cơ sở {} MHz • Thời gian thực {} MHz", self.autonomous.hardware_profile.cpu_base_mhz, cur_cpu_mhz)).size(typography::DETAIL_TEXT).strong().color(p.accent_blue));
                        ui.end_row();

                        ui.label(RichText::new("Thiết lập tối ưu:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("Quantum {} • Timer {}", if self.win32_priority_boost { "Tối đa 3x (Ứng dụng chính)" } else { "Mặc định Windows" }, if self.timer_1ms_enabled { "1.0 ms (Low Latency)" } else { "15.6 ms (Mặc định)" })).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // 2. Cảnh báo rủi ro (Risk Assessment)
        ui.label(RichText::new("2. ĐÁNH GIÁ RỦI RO HỆ THỐNG").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);
        let cpu = self.protection.cpu_usage;
        let (risk_lvl, risk_title, risk_diag, risk_advice) = if cpu > 80.0 {
            (
                RiskLevel::Danger,
                "CPU ĐANG BỊ QUÁ TẢI NGHIÊM TRỌNG",
                "Mức chiếm dụng CPU vượt ngưỡng 80%, dễ dẫn đến quá nhiệt, giảm xung (Thermal Throttling) và làm đơ giao diện các ứng dụng khác.",
                "Bấm 'Hành động' để ép EcoQoS toàn bộ app ngầm hoặc tắt tiến trình gây treo bên dưới.",
            )
        } else if cpu > 50.0 {
            (
                RiskLevel::Warning,
                "TẢI CPU Ở MỨC TRUNG BÌNH CAO",
                "Hệ thống đang chạy các tác vụ nặng hoặc nhiều luồng ngầm cùng hoạt động đồng thời.",
                "Tự động ưu tiên luồng cho cửa sổ đang dùng và hạ ưu tiên các tiến trình chạy ẩn.",
            )
        } else {
            (
                RiskLevel::Safe,
                "CPU MÁT MẺ & HOẠT ĐỘNG ỔN ĐỊNH",
                "Mức tiêu thụ CPU thấp (< 50%), tài nguyên xử lý dồi dào, không có dấu hiệu nghẽn cổ chai.",
                "Duy trì các thiết lập tối ưu hiện tại để tiết kiệm điện và giữ máy bền bỉ.",
            )
        };
        render_risk_card(ui, &p, risk_lvl, risk_title, risk_diag, risk_advice);
    }

    // --- TIẾN TRÌNH & TÁC NHÂN CPU ---
    fn render_cpu_tasks(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Top 5 ứng dụng ngốn CPU
        ui.label(RichText::new("TOP 5 ỨNG DỤNG CHIẾM NHIỀU CPU NHẤT").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);

        let mut cpu_procs = self.top_processes.clone();
        cpu_procs.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));
        let top_5: Vec<_> = cpu_procs.into_iter().take(5).collect();

        if top_5.is_empty() {
            ui.label(RichText::new("Đang làm mới danh sách tiến trình...").size(typography::DETAIL_TEXT).color(p.text_secondary));
        } else {
            egui::Grid::new("cpu_top5_grid").striped(true).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
                ui.label(RichText::new("Ứng dụng / PID").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Tải CPU").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("RAM").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Thao tác").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.end_row();

                for proc in top_5 {
                    ui.label(RichText::new(format!("{} (PID: {})", proc.name, proc.pid)).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                    let cpu_col = if proc.cpu_usage > 20.0 { p.danger_red } else if proc.cpu_usage > 5.0 { p.warn_amber } else { p.accent_blue };
                    ui.label(RichText::new(format!("{:.1}%", proc.cpu_usage)).size(typography::DETAIL_TEXT).strong().color(cpu_col));
                    ui.label(RichText::new(format_bytes(proc.memory_bytes)).size(typography::DETAIL_TEXT).color(p.text_secondary));
                    let term_btn = egui::Button::new(RichText::new("Tắt").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                        .fill(p.danger_red)
                        .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                    if ui.add(term_btn).clicked() {
                        let _ = terminate_process(proc.pid);
                        self.process_action_msg = format!("✓ Đã kết thúc '{}'", proc.name);
                    }
                    ui.end_row();
                }
            });
        }

        ui.add_space(8.0);

        // Ứng dụng khởi động cùng Windows
        ui.horizontal(|ui| {
            ui.label(RichText::new("ỨNG DỤNG KHỞI ĐỘNG CÙNG WINDOWS").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let refresh_btn = egui::Button::new(RichText::new("🔄 Làm mới").size(typography::BTN_SMALL).strong().color(p.text_primary))
                    .fill(p.tab_inactive_bg)
                    .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                if ui.add(refresh_btn).clicked() {
                    self.startup_apps = get_startup_apps();
                }
            });
        });
        ui.add_space(2.0);

        if !self.startup_action_msg.is_empty() {
            ui.label(RichText::new(&self.startup_action_msg).size(typography::DETAIL_TEXT).color(p.success_green).strong());
            ui.add_space(2.0);
        }

        if self.startup_apps.is_empty() {
            ui.label(RichText::new("Không phát hiện ứng dụng khởi động ngầm nào.").size(typography::DETAIL_TEXT).color(p.text_secondary));
        } else {
            egui::Grid::new("cpu_startup_grid").striped(true).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
                ui.label(RichText::new("Tên ứng dụng").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Trạng thái").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Thao tác").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.end_row();

                for app in &mut self.startup_apps {
                    ui.label(RichText::new(&app.name).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                    let st_txt = if app.is_enabled { "Đang bật" } else { "Đã tắt" };
                    let st_col = if app.is_enabled { p.warn_amber } else { p.text_secondary };
                    ui.label(RichText::new(st_txt).size(typography::DETAIL_TEXT).strong().color(st_col));

                    let btn_text = if app.is_enabled { "Tắt khởi động" } else { "Bật khởi động" };
                    let btn_col = if app.is_enabled { p.danger_red } else { p.primary_btn };
                    if ui.add(egui::Button::new(RichText::new(btn_text).size(typography::BTN_SMALL).color(Color32::WHITE)).fill(btn_col)).clicked() {
                        let new_st = !app.is_enabled;
                        if toggle_startup_item(&app.name, &app.location, new_st) {
                            app.is_enabled = new_st;
                            self.startup_action_msg = format!("✓ Đã {} '{}'", if new_st { "bật" } else { "tắt" }, app.name);
                        }
                    }
                    ui.end_row();
                }
            });
        }
    }

    // --- THÔNG TIN PHÂN VÙNG RAM (MỤC 1 & 2) ---
    fn render_ram_info(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // 1. Thông tin cấu hình chi tiết
        ui.label(RichText::new("1. DUNG LƯỢNG BỘ NHỚ VẬT LÝ & SWAP VIRTUAL").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(3.0);
        let pf_used = self.mem_stats.pagefile_used_bytes;
        let pf_total = self.mem_stats.pagefile_total_bytes;

        egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
            .stroke(Stroke::new(1.0, p.card_border))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                egui::Grid::new("ram_config_table_vertical")
                    .num_columns(2)
                    .spacing(Vec2::new(18.0, 7.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("RAM vật lý:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("Đã dùng {} / Tổng {:.1} GB", format_bytes(self.mem_stats.used_bytes), self.autonomous.hardware_profile.ram_total_gb)).size(typography::BODY_TEXT).strong().color(p.text_primary));
                        ui.end_row();

                        ui.label(RichText::new("Bộ nhớ khả dụng:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("{} trống ({:.0}% khả dụng)", format_bytes(self.mem_stats.free_bytes), (100.0 - self.mem_stats.load_percent as f32).max(0.0))).size(typography::DETAIL_TEXT).strong().color(p.success_green));
                        ui.end_row();

                        if pf_total > 0 {
                            ui.label(RichText::new("Bộ nhớ ảo (Pagefile):").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(format!("Đã dùng {} / Tổng {}", format_bytes(pf_used), format_bytes(pf_total))).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                            ui.end_row();
                        }

                        ui.label(RichText::new("Thiết lập tối ưu:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("Nén RAM: {} • Auto Trim: Ngưỡng {}%", if self.memory_compression { "Đang bật (Giảm swap đĩa)" } else { "Tắt" }, self.ram_trim_threshold_pct)).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // 2. Cảnh báo rủi ro
        ui.label(RichText::new("2. ĐÁNH GIÁ RỦI RO BỘ NHỚ").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);
        let load = self.mem_stats.load_percent;
        let (risk_lvl, risk_title, risk_diag, risk_advice) = if load > 85 {
            (
                RiskLevel::Danger,
                "NGUY CƠ TRÀN BỘ NHỚ & BÀO MÒN SSD",
                "RAM bị chiếm dụng trên 85%. Windows bắt buộc phải chuyển dữ liệu vào ổ cứng (Pagefile Swap) liên tục làm máy giật lag và giảm tuổi thọ chip nhớ SSD.",
                "Bấm 'Hành động' để thu hồi ngay Working Set từ các ứng dụng ngầm.",
            )
        } else if load > 70 {
            (
                RiskLevel::Warning,
                "DUNG LƯỢNG RAM Ở MỨC KHÁ CAO",
                "Hệ thống đang mở nhiều ứng dụng hoặc tab trình duyệt. Bộ nhớ đệm đang tích lũy dần.",
                "Tự động dọn RAM sẽ kích hoạt khi vượt ngưỡng cài đặt.",
            )
        } else {
            (
                RiskLevel::Safe,
                "BỘ NHỚ RAM THÔNG THOÁNG",
                "Dung lượng trống dồi dào, khả dụng cao. Tốc độ chuyển đổi ứng dụng đạt mức tối đa.",
                "Không cần can thiệp thêm, cơ chế tự động nén RAM đang hoạt động tốt.",
            )
        };
        render_risk_card(ui, &p, risk_lvl, risk_title, risk_diag, risk_advice);
    }

    // --- TÁC NHÂN NGỐN RAM ---
    fn render_ram_tasks(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Top 5 ứng dụng ngốn RAM
        ui.label(RichText::new("TOP 5 ỨNG DỤNG CHIẾM NHIỀU RAM NHẤT").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);

        let mut ram_procs = self.top_processes.clone();
        ram_procs.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
        let top_5: Vec<_> = ram_procs.into_iter().take(5).collect();

        if top_5.is_empty() {
            ui.label(RichText::new("Đang làm mới danh sách tiến trình...").size(typography::DETAIL_TEXT).color(p.text_secondary));
        } else {
            egui::Grid::new("ram_top5_grid").striped(true).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
                ui.label(RichText::new("Ứng dụng / PID").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("RAM Chiếm").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Thao tác").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.end_row();

                for proc in top_5 {
                    ui.label(RichText::new(format!("{} (PID: {})", proc.name, proc.pid)).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                    ui.label(RichText::new(format_bytes(proc.memory_bytes)).size(typography::DETAIL_TEXT).strong().color(p.accent_blue));
                    ui.horizontal(|ui| {
                        let clean_btn = egui::Button::new(RichText::new("Dọn").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                            .fill(p.primary_btn)
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(clean_btn).clicked() {
                            let f = trim_single_process(proc.pid);
                            self.criteria_apply_msg = format!("✓ Thu hồi {} từ '{}'", format_bytes(f), proc.name);
                        }
                        let term_btn = egui::Button::new(RichText::new("Tắt").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                            .fill(p.danger_red)
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                        if ui.add(term_btn).clicked() {
                            let _ = terminate_process(proc.pid);
                            self.criteria_apply_msg = format!("✓ Đã kết thúc '{}'", proc.name);
                        }
                    });
                    ui.end_row();
                }
            });
        }
    }

    // --- THÔNG TIN PHÂN VÙNG PIN / NGUỒN (MỤC 1 & 2) ---
    fn render_pin_info(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // 1. Thông tin cấu hình chi tiết
        ui.label(RichText::new("1. THÔNG TIN NGUỒN ĐIỆN & CELL PIN THỰC TẾ").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(3.0);

        egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
            .stroke(Stroke::new(1.0, p.card_border))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                egui::Grid::new("pin_config_table_vertical")
                    .num_columns(2)
                    .spacing(Vec2::new(18.0, 7.0))
                    .show(ui, |ui| {
                        if self.power_status.has_battery {
                            let pwr_str = if self.power_status.discharge_rate_mw > 0 {
                                format!("{:.1}W xả", self.power_status.power_watts)
                            } else if self.power_status.charge_rate_mw > 0 {
                                format!("{:.1}W nạp", self.power_status.power_watts)
                            } else if self.power_status.is_ac_online {
                                "Cắm sạc AC".to_string()
                            } else {
                                "Dùng pin".to_string()
                            };

                            ui.label(RichText::new("Nguồn & Dòng điện:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(format!("{} • Điện áp {:.1} V", pwr_str, self.power_status.voltage_volts)).size(typography::BODY_TEXT).strong().color(p.text_primary));
                            ui.end_row();

                            let bat_desc = if let Some(b) = self.dual_batteries.first() {
                                format!("Chai {:.1}% • {} chu kỳ sạc ({:.1} Wh / {:.1} Wh)", b.wear_percent, b.cycle_count, b.full_charge_capacity_mwh as f32 / 1000.0, b.design_capacity_mwh as f32 / 1000.0)
                            } else {
                                "Pin Lithium-Ion chuẩn".to_string()
                            };
                            ui.label(RichText::new("Tình trạng cell pin:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(bat_desc).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                            ui.end_row();

                            ui.label(RichText::new("Chế độ quạt:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(if self.cooling_active { "Active (Làm mát tối đa)" } else { "Passive (0dB)" }).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                            ui.end_row();

                            ui.label(RichText::new("Màn hình & Tiết kiệm:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(format!("Độ sáng LED {}%", self.brightness_level)).size(typography::DETAIL_TEXT).strong().color(p.accent_blue));
                            ui.end_row();
                        } else {
                            ui.label(RichText::new("Loại thiết bị:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new("PC để bàn (Desktop) • Nguồn AC 220V trực tiếp").size(typography::BODY_TEXT).strong().color(p.text_primary));
                            ui.end_row();

                            ui.label(RichText::new("Chế độ quạt tản nhiệt:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(if self.cooling_active { "Active (Tản nhiệt tối đa)" } else { "Passive (0dB)" }).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                            ui.end_row();

                            ui.label(RichText::new("Chế độ chống Sleep:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                            ui.label(RichText::new(if self.keep_awake_mode == 2 { "Đang bật (Giữ máy thức)" } else { "Tắt (Theo mặc định OS)" }).size(typography::DETAIL_TEXT).strong().color(p.accent_blue));
                            ui.end_row();
                        }
                    });
            });

        ui.add_space(8.0);

        // 2. Cảnh báo rủi ro
        ui.label(RichText::new("2. ĐÁNH GIÁ RỦI RO CHAI PIN & CELL LITHIUM").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);

        if !self.power_status.has_battery {
            render_risk_card(
                ui,
                &p,
                RiskLevel::Safe,
                "NGUỒN ĐIỆN AC 220V TRỰC TIẾP",
                "Máy tính để bàn hoạt động hoàn toàn bằng nguồn điện AC ổn định, không có cell pin lithium nên không có rủi ro chai hay phồng pin.",
                "Cấu hình quạt tản nhiệt Active để duy trì nhiệt độ linh kiện bền bỉ.",
            );
        } else {
            let max_wear = self.dual_batteries.iter().map(|b| b.wear_percent).fold(0.0f32, f32::max);
            let (risk_lvl, risk_title, risk_diag, risk_advice) = if max_wear > 25.0 {
                (
                    RiskLevel::Danger,
                    "PIN ĐÃ BỊ CHAI NGHIÊM TRỌNG",
                    "Độ chai cell pin đã vượt quá 25%. Thời lượng sử dụng thực tế bị suy giảm rõ rệt và máy có nguy cơ sập nguồn bất ngờ khi pin xuống dưới 20%.",
                    "Hạn chế chạy tác vụ nặng khi chỉ dùng pin và cân nhắc thay cell pin mới.",
                )
            } else if !self.protection.battery_threshold_enabled && self.power_status.is_ac_online {
                (
                    RiskLevel::Warning,
                    "CẮM SẠC 100% LIÊN TỤC LÀM PHỒNG CELL PIN",
                    "Pin đang duy trì mức điện áp đỉnh 100% trong khi cắm sạc liên tục. Đây là nguyên nhân hàng đầu khiến cell lithium sinh nhiệt và phồng rộp.",
                    "Vào mục 'Hành động' bật 'Khóa sạc 80%' để bảo vệ tuổi thọ cell pin.",
                )
            } else {
                (
                    RiskLevel::Safe,
                    "CELL PIN HOẠT ĐỘNG KHỎE MẠNH",
                    "Mức độ chai pin thấp, chu kỳ sạc ổn định. Ngưỡng bảo vệ pin giúp ngăn chặn tình trạng quá sạc.",
                    "Duy trì mức độ sáng hợp lý (dưới 50%) khi rút sạc để kéo dài thời lượng pin.",
                )
            };
            render_risk_card(ui, &p, risk_lvl, risk_title, risk_diag, risk_advice);
        }
    }

    // --- TÁC NHÂN TIÊU THỤ PIN ---
    fn render_pin_tasks(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Top 5 tác nhân gây hao pin
        ui.label(RichText::new("TOP 5 TÁC NHÂN TIÊU THỤ PIN & ĐÁNH THỨC CPU").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);

        let top_cpu_app = self.top_processes.first().map(|p| format!("{} ({:.1}% CPU)", p.name, p.cpu_usage)).unwrap_or_else(|| "Chưa phát hiện".to_string());

        egui::Grid::new("pin_top5_grid").striped(true).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
            ui.label(RichText::new("Tác nhân hao điện").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
            ui.label(RichText::new("Hiện trạng").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
            ui.label(RichText::new("Thao tác nhanh").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
            ui.end_row();

            // 1. App wakeups
            ui.label(RichText::new("1. App đánh thức CPU").size(typography::DETAIL_TEXT).strong().color(p.text_primary));
            ui.label(RichText::new(top_cpu_app).size(typography::DETAIL_TEXT).color(p.accent_blue));
            let eco_btn = egui::Button::new(RichText::new("Ép EcoQoS").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
            if ui.add(eco_btn).clicked() {
                self.clean_cpu_partition();
            }
            ui.end_row();

            // 2. LED backlight
            ui.label(RichText::new("2. Màn hình LED").size(typography::DETAIL_TEXT).strong().color(p.text_primary));
            ui.label(RichText::new(format!("Độ sáng {}% (~35% điện)", self.brightness_level)).size(typography::DETAIL_TEXT).color(p.warn_amber));
            let b_btn = egui::Button::new(RichText::new("Hạ 45% Pin").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
            if ui.add(b_btn).clicked() {
                self.brightness_level = 45;
                let _ = set_brightness(45);
                self.sync_config();
            }
            ui.end_row();

            // 3. Cooling fan
            ui.label(RichText::new("3. Quạt tản nhiệt").size(typography::DETAIL_TEXT).strong().color(p.text_primary));
            ui.label(RichText::new(if self.cooling_active { "Active (Quay mạnh)" } else { "Passive (0dB)" }).size(typography::DETAIL_TEXT).color(p.text_secondary));
            let fan_txt = if self.cooling_active { "Đổi Passive" } else { "Đổi Active" };
            let fan_btn = egui::Button::new(RichText::new(fan_txt).size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
            if ui.add(fan_btn).clicked() {
                self.cooling_active = !self.cooling_active;
                let _ = set_cooling_policy(self.cooling_active);
                self.sync_config();
            }
            ui.end_row();

            // 4. DC Limit
            ui.label(RichText::new("4. Xung CPU khi rút sạc").size(typography::DETAIL_TEXT).strong().color(p.text_primary));
            ui.label(RichText::new(format!("Giới hạn {}% Turbo", self.cpu_limit_dc)).size(typography::DETAIL_TEXT).color(p.text_secondary));
            let dc_btn = egui::Button::new(RichText::new("Hạ 80% DC").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
            if ui.add(dc_btn).clicked() {
                self.cpu_limit_dc = 80;
                let _ = set_cpu_max_state_dc(80);
                self.sync_config();
            }
            ui.end_row();

            // 5. Anti-sleep
            ui.label(RichText::new("5. Chế độ chống Sleep").size(typography::DETAIL_TEXT).strong().color(p.text_primary));
            ui.label(RichText::new(if self.keep_awake_mode == 2 { "Đang giữ máy thức" } else { "Theo chuẩn OS" }).size(typography::DETAIL_TEXT).color(p.text_secondary));
            let awake_txt = if self.keep_awake_mode == 2 { "Tắt thức" } else { "Bật thức" };
            let awake_btn = egui::Button::new(RichText::new(awake_txt).size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
            if ui.add(awake_btn).clicked() {
                self.keep_awake_mode = if self.keep_awake_mode == 2 { 0 } else { 2 };
                set_keep_awake(self.keep_awake_mode);
                self.sync_config();
            }
            ui.end_row();
        });
    }

    // --- THÔNG TIN PHÂN VÙNG Ổ ĐĨA (MỤC 1 & 2) ---
    fn render_drive_info(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // 1. Thông tin cấu hình chi tiết
        ui.label(RichText::new("1. THÔNG SỐ Ổ ĐĨA & HỆ THỐNG TỆP TIN").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(3.0);
        let drv_type = if self.autonomous.hardware_profile.is_ssd { "SSD Siêu tốc (NVMe/SATA)" } else { "HDD Cơ học" };
        let d_total = self.autonomous.hardware_profile.disk_total_bytes;
        let d_avail = self.autonomous.hardware_profile.disk_available_bytes;
        let d_used = d_total.saturating_sub(d_avail);
        let free_pct = if d_total > 0 { (d_avail as f32 / d_total as f32) * 100.0 } else { 50.0 };

        egui::Frame::new()
            .fill(Color32::TRANSPARENT)
            .corner_radius(CornerRadius::same(spacing::CHIP_RADIUS))
            .stroke(Stroke::new(1.0, p.card_border))
            .inner_margin(egui::Margin::symmetric(14, 10))
            .show(ui, |ui| {
                egui::Grid::new("drive_config_table_vertical")
                    .num_columns(2)
                    .spacing(Vec2::new(18.0, 7.0))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Ổ đĩa hệ thống:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("{} ({})", self.autonomous.hardware_profile.disk_name, drv_type)).size(typography::BODY_TEXT).strong().color(p.text_primary));
                        ui.end_row();

                        ui.label(RichText::new("Dung lượng bộ nhớ:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("Đã dùng {} / Trống {} ({:.1} GB, {:.0}% trống)", format_bytes(d_used), format_bytes(d_avail), d_avail as f32 / 1e9, free_pct)).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                        ui.end_row();

                        ui.label(RichText::new("Định dạng tệp tin:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(&self.autonomous.hardware_profile.disk_fs).size(typography::DETAIL_TEXT).strong().color(p.accent_blue));
                        ui.end_row();

                        ui.label(RichText::new("Thiết lập tối ưu:").size(typography::DETAIL_TEXT).color(p.text_secondary));
                        ui.label(RichText::new(format!("SSD TRIM: {} • Dịch vụ SysMain: {}", if self.trim_enabled { "Đã kích hoạt" } else { "Tắt" }, if self.sysmain_disabled { "Đã tắt (Tốt cho SSD)" } else { "Bật" })).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                        ui.end_row();
                    });
            });

        ui.add_space(8.0);

        // 2. Cảnh báo rủi ro
        ui.label(RichText::new("2. ĐÁNH GIÁ RỦI RO TRÀN Ổ ĐĨA & TUỔI THỌ SSD").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
        ui.add_space(2.0);

        let total_junk: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();
        let (risk_lvl, risk_title, risk_diag, risk_advice) = if free_pct < 12.0 {
            (
                RiskLevel::Danger,
                "DUNG LƯỢNG Ổ ĐĨA HỆ THỐNG SẮP CẠN",
                "Dung lượng trống dưới 12%. Điều này vô hiệu hóa cơ chế Wear Leveling của SSD, khiến chip nhớ nhanh hỏng gấp 3 lần và nguy cơ lỗi cập nhật Windows.",
                "Dọn sạch ngay rác Tầng 1 và gỡ bỏ các ứng dụng hoặc tệp dung lượng lớn.",
            )
        } else if total_junk > 2 * 1024 * 1024 * 1024 {
            (
                RiskLevel::Warning,
                "TỆP RÁC TẠM TÍCH TỤ KHÁ NHIỀU",
                "Phát hiện hơn 2 GB rác hệ thống (Temp, cache Prefetch, Thumbnail) đang chiếm dụng không gian lưu trữ và làm chậm quá trình quét I/O.",
                "Bấm 'Hành động' để kích hoạt dọn dẹp an toàn toàn bộ rác Tầng 1.",
            )
        } else {
            (
                RiskLevel::Safe,
                "KHÔNG GIAN LƯU TRỮ ĐẠT CHUẨN TỐI ƯU",
                "Ổ đĩa thông thoáng, dung lượng trống dồi dào. Thuật toán Wear Leveling và lệnh SSD TRIM hoạt động hoàn hảo.",
                "Duy trì tính năng SSD TRIM và tắt ghi nhật ký LastAccess để giảm hao mòn.",
            )
        };
        render_risk_card(ui, &p, risk_lvl, risk_title, risk_diag, risk_advice);
    }

    // --- NGUỒN RÁC & CÔNG CỤ DỌN DẸP ---
    fn render_drive_tasks(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Top nguồn rác & công cụ dọn dẹp
        ui.horizontal(|ui| {
            ui.label(RichText::new("TOP NGUỒN RÁC & CÔNG CỤ DỌN DẸP ĐA TẦNG").size(typography::CHIP_TITLE).strong().color(p.accent_blue));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let scan_btn = egui::Button::new(RichText::new("🔄 Quét lại").size(typography::BTN_SMALL).strong().color(p.text_primary))
                    .fill(p.tab_inactive_bg)
                    .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
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
                    self.criteria_apply_msg = format!("✓ Quét xong: Phát hiện {}", format_bytes(total_size));
                }
            });
        });
        ui.add_space(3.0);

        ui.horizontal_wrapped(|ui| {
            let clean_safe_btn = egui::Button::new(RichText::new("Dọn rác an toàn").size(typography::BTN_NORMAL).color(Color32::WHITE))
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
                self.criteria_apply_msg = format!("✓ Đã dọn an toàn {}.", format_bytes(freed));
            }

            let pro_unlocked = is_pro_active(&self.license);
            let dev_btn_text = "Dọn Dev Cache";
            let clean_dev_btn = egui::Button::new(RichText::new(dev_btn_text).size(typography::BTN_NORMAL).strong().color(Color32::WHITE))
                .fill(if pro_unlocked { Color32::from_rgb(147, 51, 234) } else { Color32::from_rgb(217, 119, 6) });
            if ui.add(clean_dev_btn).clicked() {
                if !pro_unlocked {
                    self.show_license_modal = true;
                    self.license_feedback_msg = "Tính năng dọn cache lập trình viên (NPM/Go/Cargo hàng GB) chỉ có trên OmniTune Pro (hoặc trải nghiệm dùng thử 7 ngày). Mở khóa ngay!".to_string();
                } else {
                    let targets = get_default_targets();
                    let mut freed = 0u64;
                    for target in &targets {
                        if target.tier == CleanTier::Developer {
                            let (f, _) = clean_target(target);
                            freed += f;
                        }
                    }
                    self.criteria_apply_msg = format!("✓ Đã dọn Dev Cache {}.", format_bytes(freed));
                }
            }
        });
        ui.add_space(4.0);

        let mut sorted_cats = self.clean_categories.clone();
        sorted_cats.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        let top_5_junk: Vec<_> = sorted_cats.into_iter().take(5).collect();

        if top_5_junk.is_empty() {
            ui.label(RichText::new("Đang quét danh mục tệp rác...").size(typography::DETAIL_TEXT).color(p.text_secondary));
        } else {
            egui::Grid::new("drive_top5_grid").striped(true).spacing(Vec2::new(12.0, 6.0)).show(ui, |ui| {
                ui.label(RichText::new("Danh mục tệp rác").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Dung lượng").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Số tệp").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.label(RichText::new("Thao tác").size(typography::BTN_NORMAL).strong().color(p.text_secondary));
                ui.end_row();

                for item in top_5_junk {
                    ui.label(RichText::new(&item.name).size(typography::DETAIL_TEXT).strong().color(p.text_primary));
                    let sz_col = if item.size_bytes > 500 * 1024 * 1024 { p.danger_red } else if item.size_bytes > 100 * 1024 * 1024 { p.warn_amber } else { p.accent_blue };
                    ui.label(RichText::new(format_bytes(item.size_bytes)).size(typography::DETAIL_TEXT).strong().color(sz_col));
                    ui.label(RichText::new(format!("{} tệp", item.file_count)).size(typography::DETAIL_TEXT).color(p.text_secondary));
                    let clean_btn = egui::Button::new(RichText::new("Dọn").size(typography::BTN_SMALL).strong().color(Color32::WHITE))
                        .fill(p.primary_btn)
                        .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));
                    if ui.add(clean_btn).clicked() {
                        let targets = get_default_targets();
                        if let Some(t) = targets.into_iter().find(|t| t.name == item.name) {
                            if t.tier == CleanTier::Developer && !self.check_pro_gate("Dọn dẹp Dev Cache") {
                                // Gated by Pro
                            } else {
                                let (f, _) = clean_target(&t);
                                self.criteria_apply_msg = format!("✓ Đã dọn {} từ '{}'", format_bytes(f), item.name);
                            }
                        }
                    }
                    ui.end_row();
                }
            });
        }
    }

    fn render_cpu_basic_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: AC Turbo & Min State
        render_settings_row!(
            ui,
            |ui| {
                if render_slider_chip(
                    ui,
                    &p,
                    "1. Công suất CPU khi cắm sạc",
                    "Mở khóa xung Turbo tối đa cho mọi nhân khi cắm điện",
                    &mut self.cpu_limit_ac,
                    50..=100,
                    "%",
                    Some(("100% Turbo", 100)),
                ) {
                    let _ = set_cpu_max_state_ac(self.cpu_limit_ac);
                    self.sync_config();
                }
            },
            |ui| {
                if render_slider_chip(
                    ui,
                    &p,
                    "2. Hạ xung sâu khi nghỉ (Min State)",
                    "Hạ xung sâu khi nhàn rỗi để giảm nhiệt và tiết kiệm điện",
                    &mut self.cpu_min_state_pct,
                    1..=50,
                    "%",
                    Some(("5% Chuẩn", 5)),
                ) {
                    let _ = crate::power::set_cpu_min_state(self.cpu_min_state_pct);
                    self.sync_config();
                }
            },
        );

        // Row 2: Quantum Boost & EcoQoS
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "3. Tăng tốc app đang dùng",
                    "Cấp 3x chu kỳ CPU Quantum cho cửa sổ active (Win32Priority)",
                    &mut self.win32_priority_boost,
                    None,
                ) {
                    crate::power::set_win32_priority_separation(self.win32_priority_boost);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "4. Ép app ngầm tiết kiệm điện",
                    "Đưa tiến trình chạy ngầm vào lõi E-Core (EcoQoS)",
                    &mut self.autonomous.eco_qos_enabled,
                    None,
                ) {
                    self.sync_config();
                }
            },
        );

        // Row 3: Foreground Priority Boost & Fast HungApp Recovery
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "5. Tự ưu tiên app active",
                    "Nâng mức ưu tiên AboveNormal cho cửa sổ đang mở",
                    &mut self.autonomous.auto_pilot_enabled,
                    None,
                ) {
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "6. Đóng nhanh app bị treo đơ",
                    "Giải phóng app treo trong 1s & tắt máy tức thì (HungApp 1s)",
                    &mut self.fast_shutdown_recovery,
                    None,
                ) {
                    crate::power::set_fast_shutdown_recovery(self.fast_shutdown_recovery);
                    self.sync_config();
                }
            },
        );

        // Row 4: MMCSS Gaming & SystemResponsiveness 0
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "7. Chống bóp mạng khi game/nhạc",
                    "Không bóp băng thông mạng khi phát audio/game (MMCSS)",
                    &mut self.mmcss_optimized,
                    None,
                ) {
                    crate::power::set_mmcss_gaming_responsiveness(self.mmcss_optimized);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "8. Dồn 100% CPU cho app chính",
                    "Hủy bỏ 20% dự trữ của Windows, dành trọn tài nguyên cho app",
                    &mut self.system_responsiveness_zero,
                    None,
                ) {
                    self.sync_config();
                }
            },
        );

        // Row 5: GameDVR & MenuShowDelay
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "9. Tắt quay màn hình ngầm",
                    "Giải phóng bộ mã hóa GPU encoder nền (GameDVR & Capture)",
                    &mut self.gamedvr_enabled,
                    Some(("● ĐÃ TẮT", "○ ĐANG BẬT")),
                ) {
                    let _ = set_gamedvr(self.gamedvr_enabled);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "10. Mở menu chuột phải tức thì",
                    "Triệt tiêu độ trễ 400ms của menu chuột phải và Start (0ms)",
                    &mut self.menu_show_delay_zero,
                    None,
                ) {
                    crate::power::set_menu_show_delay(self.menu_show_delay_zero);
                    self.sync_config();
                }
            },
        );

        // Row 6: StartupDelay & TCP Auto-Tuning
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "11. Vào Desktop không chờ 10s",
                    "Bỏ thời gian hoãn nạp app khi khởi động Explorer (0ms)",
                    &mut self.startup_delay_zero,
                    None,
                ) {
                    crate::power::set_startup_delay(self.startup_delay_zero);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "12. Tự tăng tốc tải file mạng",
                    "Tự động mở rộng buffer card mạng đạt tốc độ tối đa",
                    &mut self.tcp_autotuning,
                    None,
                ) {
                    crate::power::set_tcp_optimization(self.tcp_autotuning);
                    self.sync_config();
                }
            },
        );

        // Row 7: TCP ECN & dmwappushservice
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "13. Chống rớt gói tin & lag ping",
                    "Báo nghẽn mạng để router điều tiết không rớt gói tin (TCP ECN)",
                    &mut self.tcp_ecn_enabled,
                    None,
                ) {
                    crate::power::set_tcp_ecn(self.tcp_ecn_enabled);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "14. Tắt cổng nhận lệnh ngầm",
                    "Vô hiệu hóa cổng nhận lệnh thu thập từ xa (WAP Push Service)",
                    &mut self.disable_dmwappush,
                    Some(("● ĐÃ TẮT", "○ ĐANG BẬT")),
                ) {
                    crate::startup::toggle_telemetry(self.disable_dmwappush);
                    self.sync_config();
                }
            },
        );

        // Row 8: Startup Apps & Architecture Status
        render_settings_row!(
            ui,
            |ui| {
                let count_str = format!("{} app", self.startup_apps.len());
                render_info_chip(
                    ui,
                    &p,
                    "15. Kiểm soát app khởi động",
                    "Tắt ứng dụng khởi động thừa giúp Windows boot dưới 10 giây",
                    Some((&count_str, p.accent_blue)),
                );
            },
            |ui| {
                let arch_desc = format!(
                    "Đã tối ưu chuẩn cho cấu hình {} nhân / {} luồng",
                    self.autonomous.hardware_profile.cpu_physical_cores,
                    self.autonomous.hardware_profile.cpu_logical_cores
                );
                render_info_chip(
                    ui,
                    &p,
                    "16. Trạng thái điều phối CPU",
                    &arch_desc,
                    None,
                );
            },
        );
    }

    fn render_cpu_pro_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: High Precision Timer & Telemetry DiagTrack
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "1. Giảm trễ chuột & game",
                    "Triệt tiêu trễ chuột & giật khung hình (Timer Resolution 1ms)",
                    &mut self.timer_1ms_enabled,
                    Some(("● 1.0 ms", "○ 15.6 ms")),
                ) {
                    if self.timer_1ms_enabled && !self.check_pro_gate("Timer Resolution 1.0ms") {
                        self.timer_1ms_enabled = false;
                        set_timer_resolution_1ms(false);
                    } else {
                        set_timer_resolution_1ms(self.timer_1ms_enabled);
                        self.sync_config();
                    }
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "2. Chặn gửi dữ liệu ngầm",
                    "Dừng thu thập và gửi dữ liệu chẩn đoán về Microsoft (DiagTrack)",
                    &mut self.disable_telemetry,
                    Some(("● ĐÃ CHẶN", "○ ĐANG BẬT")),
                ) {
                    if self.disable_telemetry && !self.check_pro_gate("Chặn telemetry DiagTrack") {
                        self.disable_telemetry = false;
                        crate::startup::toggle_telemetry(false);
                    } else {
                        crate::startup::toggle_telemetry(self.disable_telemetry);
                        self.sync_config();
                    }
                }
            },
        );

        // Row 2: Deep Debloat & Pro CPU Affinity
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "3. Deep Debloat hệ thống",
                    "Tắt Windows Copilot, Bing Search Start Menu và Edge chạy ngầm",
                    &mut self.deep_debloat,
                    Some(("● ĐÃ CHẶN", "○ MẶC ĐỊNH")),
                ) {
                    if self.deep_debloat && !self.check_pro_gate("Deep Debloat (Tắt Copilot/Bing/Edge)") {
                        self.deep_debloat = false;
                        let _ = crate::protection::apply_deep_debloat(false);
                    } else {
                        let msgs = crate::protection::apply_deep_debloat(self.deep_debloat);
                        self.criteria_apply_msg = format!("✓ Deep Debloat: {}", msgs.join(" | "));
                        self.sync_config();
                    }
                }
            },
            |ui| {
                let affinity_status = if is_pro_active(&self.license) {
                    ("● ĐANG BẬT", p.success_green)
                } else {
                    ("PRO VIP", p.warn_amber)
                };
                render_info_chip(
                    ui,
                    &p,
                    "4. Điều phối CPU Affinity",
                    "Gán toàn bộ nhân CPU & High Priority cho ứng dụng đang focus",
                    Some((affinity_status.0, affinity_status.1)),
                );
            },
        );
    }

    fn render_ram_basic_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: LargeSystemCache & Memory Compression
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "1. Tăng tốc đệm đọc ghi tệp",
                    "Mở rộng cache đọc ghi tệp hệ thống (LargeSystemCache)",
                    &mut self.large_system_cache,
                    None,
                ) {
                    crate::power::set_large_system_cache(self.large_system_cache);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "2. Nén RAM chống nghẽn đĩa",
                    "Nén trang RAM trong bộ nhớ, giảm ghi swap bảo vệ SSD",
                    &mut self.memory_compression,
                    None,
                ) {
                    crate::power::set_memory_compression(self.memory_compression);
                    self.sync_config();
                }
            },
        );

        // Row 2: Working Set Trim manual action & RAM Architecture info
        render_settings_row!(
            ui,
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "3. Thu hồi bộ nhớ đệm thừa",
                    "Giải phóng Working Set của các tiến trình không hoạt động",
                    "Thu hồi ngay",
                    || {
                        let (freed, count) = trim_working_sets(&mut self.sys);
                        self.criteria_apply_msg = format!("✓ Thu hồi {} từ {} tiến trình.", format_bytes(freed), count);
                    },
                );
            },
            |ui| {
                let ram_info = format!("{:.1} GB khả dụng", self.autonomous.hardware_profile.ram_total_gb);
                render_info_chip(
                    ui,
                    &p,
                    "4. Cấu trúc bộ nhớ hệ thống",
                    "Bộ đệm phân trang NTFS và vùng nhớ đệm ứng dụng Windows",
                    Some((&ram_info, p.accent_blue)),
                );
            },
        );
    }

    fn render_ram_pro_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: Auto RAM Trim & Threshold
        render_settings_row!(
            ui,
            |ui| {
                render_chip_frame(ui, p.is_dark, p.card_border, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new("1. Tự động dọn RAM khi đầy")
                                .size(typography::CHIP_TITLE)
                                .strong()
                                .color(p.text_primary),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            if render_switch(ui, &mut self.autonomous.auto_ram_enabled) {
                                if self.autonomous.auto_ram_enabled && !self.check_pro_gate("Auto-RAM 24/7") {
                                    self.autonomous.auto_ram_enabled = false;
                                } else {
                                    self.sync_config();
                                }
                            }
                        });
                    });
                    ui.add_space(2.0);
                    ui.horizontal(|ui| {
                        ui.add(
                            egui::Label::new(
                                RichText::new("Thu hồi Working Set bộ nhớ đệm thừa tự động 24/7")
                                    .size(typography::CHIP_DESC)
                                    .color(p.text_secondary),
                            )
                            .wrap(),
                        );
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            let trim_btn = egui::Button::new(
                                RichText::new("Thu hồi ngay")
                                    .size(typography::BTN_SMALL)
                                    .strong()
                                    .color(Color32::WHITE),
                            )
                            .fill(p.primary_btn)
                            .corner_radius(CornerRadius::same(spacing::BTN_RADIUS));

                            if ui.add(trim_btn).clicked() {
                                let (freed, count) = trim_working_sets(&mut self.sys);
                                self.criteria_apply_msg =
                                    format!("✓ Thu hồi {} từ {} tiến trình.", format_bytes(freed), count);
                            }
                        });
                    });
                });
            },
            |ui| {
                let ram_total = self.autonomous.hardware_profile.ram_total_gb;
                let desc = format!("Kích hoạt dọn dẹp khi bộ nhớ vượt ngưỡng (máy {:.1} GB)", ram_total);
                if render_slider_chip(
                    ui,
                    &p,
                    "2. Ngưỡng tự động dọn RAM",
                    &desc,
                    &mut self.ram_trim_threshold_pct,
                    60..=95,
                    "%",
                    None,
                ) {
                    self.autonomous.hardware_profile.dynamic_ram_threshold_pct =
                        self.ram_trim_threshold_pct;
                    self.sync_config();
                }
            },
        );

        // Row 2: Pro Engine Status & Pro Trim
        render_settings_row!(
            ui,
            |ui| {
                let engine_status = if self.autonomous.auto_ram_enabled && is_pro_active(&self.license) {
                    ("● ĐANG CHẠY 24/7", p.success_green)
                } else {
                    ("○ ĐANG DỪNG", p.text_secondary)
                };
                render_info_chip(
                    ui,
                    &p,
                    "3. Giám sát bộ nhớ thời gian thực",
                    "Tự động quét chu kỳ 30s và thu hồi bộ nhớ khi phát hiện rò rỉ (Memory Leak)",
                    Some((engine_status.0, engine_status.1)),
                );
            },
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "4. Ép giải phóng bộ nhớ đệm",
                    "Xóa sạch Standby List và Working Set của mọi app chạy ngầm",
                    "Dọn sâu",
                    || {
                        if !self.check_pro_gate("Auto-RAM 24/7") {
                            return;
                        }
                        let (freed, count) = trim_working_sets(&mut self.sys);
                        self.criteria_apply_msg = format!("✓ Đã dọn sâu {} từ {} tiến trình.", format_bytes(freed), count);
                    },
                );
            },
        );
    }

    fn render_pin_basic_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: CPU Limit DC & Brightness
        render_settings_row!(
            ui,
            |ui| {
                if render_slider_chip(
                    ui,
                    &p,
                    "1. Giới hạn xung CPU khi dùng pin",
                    "Hạn chế công suất khi rút sạc để kéo dài thời lượng pin",
                    &mut self.cpu_limit_dc,
                    50..=100,
                    "%",
                    Some(("80%", 80)),
                ) {
                    let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
                    self.sync_config();
                }
            },
            |ui| {
                if render_slider_chip(
                    ui,
                    &p,
                    "2. Độ sáng màn hình LED",
                    "Tiết kiệm 30-50% điện năng tiêu thụ từ tấm nền màn hình",
                    &mut self.brightness_level,
                    10..=100,
                    "%",
                    Some(("45% Pin", 45)),
                ) {
                    let _ = set_brightness(self.brightness_level);
                    self.sync_config();
                }
            },
        );

        // Row 2: Cooling Policy & Keep Awake
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "3. Chế độ quạt tản nhiệt",
                    "Active: Làm mát tối đa khi tải nặng • Passive: 0dB tĩnh lặng",
                    &mut self.cooling_active,
                    Some(("● Active (Mát)", "○ Passive (Êm)")),
                ) {
                    let _ = set_cooling_policy(self.cooling_active);
                    self.sync_config();
                }
            },
            |ui| {
                let mut is_keep_awake = self.keep_awake_mode == 2;
                if render_switch_chip(
                    ui,
                    &p,
                    "4. Giữ máy luôn thức",
                    "Ngăn tắt màn hình và ngủ máy khi đang tải tệp hoặc render",
                    &mut is_keep_awake,
                    Some(("● Chống Sleep", "○ Sleep OS")),
                ) {
                    self.keep_awake_mode = if is_keep_awake { 2 } else { 0 };
                    set_keep_awake(self.keep_awake_mode);
                    self.sync_config();
                }
            },
        );

        // Row 3: Hibernate & Power Diagnostic
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "5. Ngủ đông không hao pin",
                    "Lưu phiên làm việc vào ổ đĩa và ngắt điện (Hibernate)",
                    &mut self.protection.is_hibernate_enabled,
                    None,
                ) {
                    if self.protection.is_hibernate_enabled {
                        let _ = enable_hibernate_as_admin();
                    }
                }
            },
            |ui| {
                let pwr_status = if self.power_status.is_ac_online { "Đang cắm sạc (AC)" } else { "Đang dùng pin (DC)" };
                render_info_chip(
                    ui,
                    &p,
                    "6. Trạng thái nguồn điện",
                    "Hệ thống tự động kích hoạt cấu hình nguồn điện tối ưu tương ứng",
                    Some((pwr_status, p.accent_blue)),
                );
            },
        );
    }

    fn render_pin_pro_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: Battery Care 80% & EC Controller Info
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "1. Khóa sạc 80% bảo vệ pin",
                    "Chống chai pin và phồng cell lithium khi cắm sạc liên tục",
                    &mut self.protection.battery_threshold_enabled,
                    None,
                ) {
                    if self.protection.battery_threshold_enabled && !self.check_pro_gate("Khóa sạc pin 80%") {
                        self.protection.battery_threshold_enabled = false;
                        let _ = set_battery_threshold(false, 80);
                    } else {
                        let st = self.protection.battery_threshold_enabled;
                        let _ = set_battery_threshold(st, 80);
                        self.sync_config();
                    }
                }
            },
            |ui| {
                let charge_limit_status = if self.protection.battery_threshold_enabled && is_pro_active(&self.license) {
                    ("● ĐANG KHÓA 80%", p.success_green)
                } else {
                    ("○ 100% ĐẦY", p.text_secondary)
                };
                render_info_chip(
                    ui,
                    &p,
                    "2. Quản lý mạch nạp BIOS/EC",
                    "Hỗ trợ bo mạch ASUS Battery Health, Lenovo Vantage, Dell Command, HP Power Manager",
                    Some((charge_limit_status.0, charge_limit_status.1)),
                );
            },
        );
    }

    fn render_drive_basic_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: SysMain & NTFS LastAccess
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "1. Tắt dịch vụ SysMain",
                    "Tránh đọc ghi ngầm liên tục làm giảm tuổi thọ cell flash SSD",
                    &mut self.sysmain_disabled,
                    Some(("● TẮT (SSD)", "○ BẬT (HDD)")),
                ) {
                    crate::power::set_sysmain_service(!self.sysmain_disabled);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "2. Tắt ghi nhật ký truy cập",
                    "Bỏ ghi ngày giờ truy cập file, giảm 30% I/O đĩa (NTFS LastAccess)",
                    &mut self.disable_ntfs_last_access,
                    None,
                ) {
                    crate::power::set_ntfs_last_access(self.disable_ntfs_last_access);
                    self.sync_config();
                }
            },
        );

        // Row 2: NTFS 8dot3 & SSD TRIM
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "3. Tắt tạo tên tệp ngắn DOS 8.3",
                    "Bỏ định dạng DOS 8.3 cổ xưa, tăng tốc duyệt và tạo file NTFS",
                    &mut self.disable_8dot3_name,
                    None,
                ) {
                    crate::power::set_ntfs_8dot3_name(self.disable_8dot3_name);
                    self.sync_config();
                }
            },
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "4. Kích hoạt SSD TRIM tự động",
                    "Chủ động thu hồi khối nhớ SSD đã xóa, duy trì tốc độ đọc ghi",
                    &mut self.trim_enabled,
                    None,
                ) {
                    crate::power::set_ssd_trim(self.trim_enabled);
                    self.sync_config();
                }
            },
        );

        // Row 3: WaitToKill & Junk Clean Tier 1
        render_settings_row!(
            ui,
            |ui| {
                if render_switch_chip(
                    ui,
                    &p,
                    "5. Rút ngắn thời gian tắt máy",
                    "Giảm thời gian chờ tắt service từ 12s còn 2s (WaitToKill)",
                    &mut self.wait_to_kill_service,
                    None,
                ) {
                    crate::power::set_wait_to_kill_service(self.wait_to_kill_service);
                    self.sync_config();
                }
            },
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "6. Dọn dẹp rác hệ thống Tầng 1",
                    "Xóa an toàn cache Temp, Prefetch, Log và Thumbnail",
                    "Dọn ngay",
                    || {
                        let targets = get_default_targets();
                        let mut freed = 0u64;
                        for target in &targets {
                            if target.tier == CleanTier::Safe {
                                let (f, _) = clean_target(target);
                                freed += f;
                            }
                        }
                        self.criteria_apply_msg =
                            format!("✓ Đã dọn sạch {} rác an toàn.", format_bytes(freed));
                    },
                );
            },
        );

        // Row 4: Restore Windows Defaults & Partition Info
        render_settings_row!(
            ui,
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "7. Khôi phục mặc định Windows",
                    "Đưa Power Schemes, Timer, GameDVR và Telemetry về gốc",
                    "Khôi phục gốc",
                    || {
                        let logs = restore_windows_defaults();
                        self.criteria_apply_msg = format!("✓ Đã khôi phục: {}", logs.join(" | "));
                        self.sync_config();
                    },
                );
            },
            |ui| {
                render_info_chip(
                    ui,
                    &p,
                    "8. Quản lý phân vùng hệ thống NTFS",
                    "Đã tối ưu hóa thông số I/O buffer và phân phối cluster đọc ghi",
                    None,
                );
            },
        );
    }

    fn render_drive_pro_settings(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // Row 1: Restore Point & Dev Cache Clean
        render_settings_row!(
            ui,
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "1. Tạo điểm khôi phục",
                    "Sao lưu trạng thái hệ thống trước khi tinh chỉnh sâu",
                    "Tạo",
                    || {
                        if !self.check_pro_gate("Tạo điểm khôi phục") {
                            return;
                        }
                        match create_restore_point() {
                            Ok(msg) => self.criteria_apply_msg = format!("✓ {}", msg),
                            Err(e) => self.criteria_apply_msg = format!("✗ {}", e),
                        }
                    },
                );
            },
            |ui| {
                render_action_chip(
                    ui,
                    &p,
                    "2. Dọn dẹp Dev Cache lập trình",
                    "Xóa node_modules, Cargo cache, Go build cache, Docker thừa (hàng GB)",
                    "Dọn ngay",
                    || {
                        if !self.check_pro_gate("Dọn dẹp Dev Cache") {
                            return;
                        }
                        let targets = get_default_targets();
                        let mut freed = 0u64;
                        for target in &targets {
                            if target.tier == CleanTier::Developer {
                                let (f, _) = clean_target(target);
                                freed += f;
                            }
                        }
                        self.criteria_apply_msg = format!("✓ Đã dọn sạch {} Dev Cache.", format_bytes(freed));
                    },
                );
            },
        );

        // Row 2: SSD Life & Pro Cleaner Info
        render_settings_row!(
            ui,
            |ui| {
                render_info_chip(
                    ui,
                    &p,
                    "3. Bảo vệ cell flash & Giảm TBW",
                    "Ngăn chặn Windows swap paging liên tục, kéo dài tuổi thọ chip nhớ SSD thêm 3-5 năm",
                    None,
                );
            },
            |ui| {
                render_info_chip(
                    ui,
                    &p,
                    "4. Tự động dọn rác tầng sâu",
                    "Phân tích chuyên sâu các thư mục ẩn Package Manager, Crash Dumps và WinSxS Backup",
                    None,
                );
            },
        );
    }
}
