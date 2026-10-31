use crate::battery_info::get_dual_battery_info;
use crate::cleaner::{
    clean_target, format_bytes, get_default_targets, scan_target,
};
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process, set_process_eco_mode};
use crate::memory::{
    get_memory_stats, get_top_processes, terminate_process, trim_single_process, trim_working_sets,
    MemStats,
};
use crate::power::{
    get_active_scheme_name, get_brightness, get_cooling_policy, get_cpu_max_state_ac,
    get_cpu_max_state_dc, get_gamedvr, get_power_status, set_brightness, set_cooling_policy,
    set_cpu_max_state_ac, set_cpu_max_state_dc, set_gamedvr, set_keep_awake, set_power_scheme,
    set_timer_resolution_1ms, PowerStatus,
};
use crate::protection::{
    enable_hibernate_as_admin, get_protection_overview, set_battery_threshold, ProtectionOverview,
};
use crate::scenarios::{apply_scenario, Scenario};
use crate::startup::{get_startup_apps, is_telemetry_active, toggle_startup_item, toggle_telemetry};
use crate::theme::{
    apply_theme, configure_fonts, ThemeMode, ThemePalette,
};
use crate::types::{BatteryDetails, CleanTier, JunkCategory, ProcessMemInfo, StartupAppItem, Tab};
use eframe::egui::{self, Color32, CornerRadius, ProgressBar, RichText, Stroke, Vec2};
use std::time::{Duration, Instant};
use sysinfo::System;

pub struct OptimizerApp {
    current_tab: Tab,
    current_scenario: Scenario,
    auto_detect_enabled: bool,
    eco_qos_enabled: bool,
    foreground_info: Option<(u32, String)>,
    eco_boost_status: String,

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
    auto_ram_trim_enabled: bool,

    // Startup & Telemetry state
    startup_apps: Vec<StartupAppItem>,
    is_telemetry_running: bool,
    telemetry_status_msg: String,
    startup_action_msg: String,
    battery_thresh_msg: String,

    // Criteria Tuning parameters
    cpu_limit_dc: u32,
    cpu_limit_ac: u32,
    timer_1ms_enabled: bool,
    keep_awake_mode: u8,
    cooling_active: bool,
    gamedvr_enabled: bool,
    auto_ram_threshold: u32,
    criteria_feedback_msg: String,

    // Power settings
    brightness_level: u32,
    power_status_msg: String,

    // Scenario message
    scenario_status_msg: String,

    // Dynamic hardware info
    cpu_brand: String,
    cpu_cores_info: String,

    // General status
    quick_opt_msg: String,
    last_refresh: Instant,
    last_battery_poll: Instant,
}

impl OptimizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);
        let theme_mode = ThemeMode::Light;
        let palette = apply_theme(&cc.egui_ctx, theme_mode);

        let mut sys = System::new().expect("failed to initialize sysinfo");
        sys.refresh_all();

        let mem_stats = get_memory_stats();
        let mut power_status = get_power_status();
        let top_processes = get_top_processes(&mut sys, 25);
        let dual_batteries = get_dual_battery_info();
        let cpu_usage = sys.global_cpu_usage();
        let protection = get_protection_overview(cpu_usage);
        let brightness_level = get_brightness();

        let foreground_info = get_foreground_process(&sys);

        // Compute power wattage and voltage from ACPI status
        let total_discharge: u32 = dual_batteries.iter().map(|b| b.discharge_rate_mw).sum();
        let total_charge: u32 = dual_batteries.iter().map(|b| b.charge_rate_mw).sum();
        let avg_voltage: u32 = if !dual_batteries.is_empty() {
            dual_batteries.iter().map(|b| b.voltage_mv).sum::<u32>() / dual_batteries.len() as u32
        } else {
            0
        };
        power_status.discharge_rate_mw = total_discharge;
        power_status.charge_rate_mw = total_charge;
        power_status.power_watts = if total_discharge > 0 {
            total_discharge as f32 / 1000.0
        } else if total_charge > 0 {
            total_charge as f32 / 1000.0
        } else {
            0.0
        };
        power_status.voltage_volts = avg_voltage as f32 / 1000.0;

        let cpu_brand = sys
            .cpus()
            .first()
            .map(|c| c.brand().trim().to_string())
            .unwrap_or_else(|| "CPU Processor".to_string());

        let cpu_cores_info = format!(
            "{} Cores • {} Threads",
            sysinfo::System::physical_core_count().unwrap_or(2),
            sys.cpus().len()
        );

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

        let startup_apps = get_startup_apps();
        let is_telemetry_running = is_telemetry_active();
        let cpu_limit_dc = get_cpu_max_state_dc();
        let cpu_limit_ac = get_cpu_max_state_ac();
        let cooling_active = get_cooling_policy();
        let gamedvr_enabled = get_gamedvr();

        Self {
            current_tab: Tab::Scenarios,
            current_scenario: Scenario::Default,
            auto_detect_enabled: false,
            eco_qos_enabled: true,
            foreground_info,
            eco_boost_status: String::new(),
            theme_mode,
            palette,
            sys,
            mem_stats,
            power_status,
            dual_batteries,
            top_processes,
            protection,
            clean_categories,
            clean_status_msg: "Sẵn sàng quét rác hệ thống (Đã phân 3 tầng rác).".to_string(),
            process_search_query: String::new(),
            process_action_msg: String::new(),
            auto_ram_trim_enabled: true,
            startup_apps,
            is_telemetry_running,
            telemetry_status_msg: String::new(),
            startup_action_msg: String::new(),
            battery_thresh_msg: String::new(),
            cpu_limit_dc,
            cpu_limit_ac,
            timer_1ms_enabled: false,
            keep_awake_mode: 0,
            cooling_active,
            gamedvr_enabled,
            auto_ram_threshold: 85,
            criteria_feedback_msg: String::new(),
            brightness_level,
            power_status_msg: String::new(),
            scenario_status_msg: "Chọn một kịch bản bên dưới để tối ưu phần cứng theo nhu cầu.".to_string(),
            cpu_brand,
            cpu_cores_info,
            quick_opt_msg: String::new(),
            last_refresh: Instant::now(),
            last_battery_poll: Instant::now(),
        }
    }

    fn calculate_health_score(&self) -> (u32, &'static str, Color32) {
        let mut score = 100i32;

        // RAM load penalty
        if self.mem_stats.load_percent >= 85 {
            score -= 20;
        } else if self.mem_stats.load_percent >= 70 {
            score -= 10;
        }

        // Junk size penalty
        let total_junk: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();
        if total_junk > 2 * 1024 * 1024 * 1024 {
            score -= 20;
        } else if total_junk > 500 * 1024 * 1024 {
            score -= 10;
        }

        // Telemetry penalty
        if self.is_telemetry_running {
            score -= 10;
        }

        // Hibernate penalty on laptop
        if !self.protection.is_hibernate_enabled {
            score -= 10;
        }

        // Battery wear penalty
        let max_wear = self
            .dual_batteries
            .iter()
            .map(|b| b.wear_percent)
            .fold(0.0f32, f32::max);
        if max_wear >= 30.0 {
            score -= 10;
        }

        let final_score = score.clamp(0, 100) as u32;

        let (label, color) = if final_score >= 90 {
            ("XUẤT SẮC (Tối ưu hóa hoàn hảo)", self.palette.success_green)
        } else if final_score >= 75 {
            ("TỐT (Máy hoạt động ổn định)", self.palette.accent_blue)
        } else if final_score >= 60 {
            ("TRUNG BÌNH (Cần dọn dẹp & tối ưu)", self.palette.warn_amber)
        } else {
            ("CẦN TỐI ƯU (Nhiều rác & app nền)", self.palette.danger_red)
        };

        (final_score, label, color)
    }

    fn refresh_periodic(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_secs(2) {
            self.sys.refresh_cpu_usage();
            self.mem_stats = get_memory_stats();
            self.power_status = get_power_status();
            self.protection.cpu_usage = self.sys.global_cpu_usage();

            // Auto-trim RAM if exceeds auto_ram_threshold
            if self.auto_ram_trim_enabled && self.mem_stats.load_percent >= self.auto_ram_threshold {
                let (freed, count) = trim_working_sets(&mut self.sys);
                if freed > 0 {
                    self.quick_opt_msg = format!(
                        "🤖 [Tự động]: RAM vượt {}%, đã thu hồi {} từ {} tiến trình.",
                        self.auto_ram_threshold,
                        format_bytes(freed),
                        count
                    );
                }
            }

            // Track foreground window
            let fg = get_foreground_process(&self.sys);
            if fg != self.foreground_info {
                self.foreground_info = fg.clone();

                if self.eco_qos_enabled {
                    let (_, eco_count) = apply_foreground_boost_and_eco(
                        &self.sys,
                        self.foreground_info.as_ref().map(|(pid, _)| *pid),
                    );
                    if let Some((_, ref name)) = self.foreground_info {
                        self.eco_boost_status = format!(
                            "🎯 Đã ưu tiên '{}' (Áp dụng EcoQoS hạ tải {} app nền)",
                            name, eco_count
                        );
                    }
                }

                if self.auto_detect_enabled {
                    if let Some(scen) = crate::scenarios::detect_recommended_scenario(
                        &self.sys,
                        self.power_status.is_ac_online,
                        self.power_status.battery_percent,
                    ) {
                        if scen != self.current_scenario {
                            self.current_scenario = scen;
                            let msg = apply_scenario(scen, &mut self.sys);
                            self.scenario_status_msg = format!("🤖 [Tự động]: {}", msg);
                        }
                    }
                }
            }

            // Periodic ACPI battery poll (every 10s)
            if self.last_battery_poll.elapsed() >= Duration::from_secs(10) {
                self.dual_batteries = get_dual_battery_info();
                let total_discharge: u32 = self.dual_batteries.iter().map(|b| b.discharge_rate_mw).sum();
                let total_charge: u32 = self.dual_batteries.iter().map(|b| b.charge_rate_mw).sum();
                let avg_voltage: u32 = if !self.dual_batteries.is_empty() {
                    self.dual_batteries.iter().map(|b| b.voltage_mv).sum::<u32>() / self.dual_batteries.len() as u32
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
                self.last_battery_poll = Instant::now();
            }

            self.last_refresh = Instant::now();
        }
    }

    fn run_quick_optimize(&mut self) {
        // 1. Clean Tier 1 Safe Junk
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

        // 2. Rescan junk categories
        for (i, target) in targets.iter().enumerate() {
            let (size, count) = scan_target(target);
            if let Some(cat) = self.clean_categories.get_mut(i) {
                cat.size_bytes = size;
                cat.file_count = count;
            }
        }

        // 3. Trim RAM
        let (freed_ram, count) = trim_working_sets(&mut self.sys);

        // 4. Power Scheme & Throttling
        let _ = set_cpu_max_state_dc(80);
        if !self.power_status.is_ac_online {
            let _ = set_power_scheme("SCHEME_MAX");
        }

        // 5. EcoQoS
        if self.eco_qos_enabled {
            let _ = apply_foreground_boost_and_eco(
                &self.sys,
                self.foreground_info.as_ref().map(|(pid, _)| *pid),
            );
        }

        self.mem_stats = get_memory_stats();
        self.top_processes = get_top_processes(&mut self.sys, 25);

        self.quick_opt_msg = format!(
            "✓ Tối ưu toàn diện thành công! Dọn {} rác ({} files), thu hồi {} RAM từ {} tiến trình & bật EcoQoS.",
            format_bytes(freed_junk),
            deleted_files,
            format_bytes(freed_ram),
            count
        );
    }
}

fn render_sidebar_tab(
    ui: &mut egui::Ui,
    current: &mut Tab,
    target: Tab,
    label: &str,
    palette: &ThemePalette,
) {
    let is_active = *current == target;
    let (bg, fg) = if is_active {
        (palette.tab_active_bg, palette.tab_active_fg)
    } else {
        (
            if palette.is_dark {
                Color32::from_rgb(18, 25, 42)
            } else {
                Color32::from_rgb(255, 255, 255)
            },
            palette.tab_inactive_fg,
        )
    };

    let btn = egui::Button::new(
        RichText::new(label)
            .size(12.5)
            .strong()
            .color(fg),
    )
    .fill(bg)
    .min_size(Vec2::new(ui.available_width(), 34.0))
    .stroke(Stroke::new(
        1.0,
        if is_active { palette.accent_blue } else { palette.card_border },
    ))
    .corner_radius(CornerRadius::same(6));

    if ui.add(btn).clicked() {
        *current = target;
    }
}

impl eframe::App for OptimizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh_periodic();
        ui.ctx().request_repaint_after(Duration::from_millis(1000));

        let p = self.palette;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg_main).inner_margin(egui::Margin::same(8)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    // --- CỘT 1: SIDEBAR TABS (180px) ---
                    ui.allocate_ui_with_layout(
                        Vec2::new(180.0, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            ui.set_width(180.0);

                            // App Logo & Branding
                            ui.add_space(2.0);
                            ui.heading(
                                RichText::new("⚡ OmniTune")
                                    .size(20.0)
                                    .strong()
                                    .color(p.accent_blue),
                            );
                            ui.label(
                                RichText::new("System Optimizer")
                                    .size(11.5)
                                    .color(p.text_secondary),
                            );
                            ui.add_space(4.0);
                            ui.separator();
                            ui.add_space(4.0);

                            // Vertical Tab Navigation List
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Scenarios, "🎯 Kịch bản", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Criteria, "📋 Tiêu chí setup", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Dashboard, "📊 Tổng quan", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Cleaner, "🧹 Dọn rác", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Memory, "🧠 RAM & Tiến trình", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Power, "🔋 Pin & ACPI", &p);
                            ui.add_space(2.0);
                            render_sidebar_tab(ui, &mut self.current_tab, Tab::Protection, "🚀 Khởi động & Bảo vệ", &p);

                            // Bottom Utilities in Sidebar
                            ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
                                ui.add_space(4.0);

                                // Theme Toggle Button
                                let (theme_btn_text, theme_btn_bg) = if self.theme_mode == ThemeMode::Dark {
                                    ("☀️ Giao diện Sáng", Color32::from_rgb(234, 179, 8))
                                } else {
                                    ("🌙 Giao diện Tối", Color32::from_rgb(30, 41, 59))
                                };

                                let theme_btn = egui::Button::new(
                                    RichText::new(theme_btn_text)
                                        .size(12.0)
                                        .strong()
                                        .color(if self.theme_mode == ThemeMode::Dark { Color32::BLACK } else { Color32::WHITE }),
                                )
                                .fill(theme_btn_bg)
                                .min_size(Vec2::new(ui.available_width(), 30.0))
                                .corner_radius(CornerRadius::same(5));

                                if ui.add(theme_btn).clicked() {
                                    self.theme_mode = match self.theme_mode {
                                        ThemeMode::Dark => ThemeMode::Light,
                                        ThemeMode::Light => ThemeMode::Dark,
                                    };
                                    self.palette = apply_theme(ui.ctx(), self.theme_mode);
                                }

                                ui.add_space(4.0);

                                // Power Status Badge
                                let (status_text, badge_bg, badge_fg) = if self.power_status.is_ac_online {
                                    (
                                        "🔌 Nguồn AC",
                                        if p.is_dark { Color32::from_rgb(20, 83, 45) } else { Color32::from_rgb(220, 252, 231) },
                                        if p.is_dark { p.success_green } else { Color32::from_rgb(21, 128, 61) },
                                    )
                                } else {
                                    (
                                        "🔋 Dùng Pin",
                                        if p.is_dark { Color32::from_rgb(124, 45, 18) } else { Color32::from_rgb(254, 243, 199) },
                                        if p.is_dark { p.warn_amber } else { Color32::from_rgb(180, 83, 9) },
                                    )
                                };

                                egui::Frame::new()
                                    .fill(badge_bg)
                                    .corner_radius(CornerRadius::same(5))
                                    .stroke(Stroke::new(1.0, badge_fg))
                                    .inner_margin(egui::Margin::symmetric(8, 4))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(
                                                RichText::new(format!("{} {}%", status_text, self.power_status.battery_percent))
                                                    .size(11.5)
                                                    .strong()
                                                    .color(badge_fg),
                                            );
                                        });
                                    });

                                ui.add_space(4.0);
                                ui.separator();
                            });
                        },
                    );

                    ui.separator();

                    // --- CỘT 2: NỘI DUNG TAB (Full remaining width) ---
                    ui.allocate_ui_with_layout(
                        Vec2::new(ui.available_width(), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            egui::ScrollArea::vertical()
                                .auto_shrink([false, false])
                                .show(ui, |ui| {
                                    match self.current_tab {
                                        Tab::Scenarios => self.render_scenarios(ui),
                                        Tab::Criteria => self.render_criteria(ui),
                                        Tab::Dashboard => self.render_dashboard(ui),
                                        Tab::Cleaner => self.render_cleaner(ui),
                                        Tab::Memory => self.render_memory(ui),
                                        Tab::Power => self.render_power(ui),
                                        Tab::Protection => self.render_protection(ui),
                                    }
                                });
                        },
                    );
                });
            });
    }
}

impl OptimizerApp {
    fn render_scenarios(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            ui.heading(RichText::new("🎯 Tối ưu theo Kịch bản thực tế").size(16.0).strong().color(p.text_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(
                    &mut self.auto_detect_enabled,
                    RichText::new("🤖 Tự động nhận diện").size(12.0).strong().color(p.accent_blue),
                );
            });
        });

        ui.horizontal(|ui| {
            ui.label(
                RichText::new("Tự động tinh chỉnh CPU, GPU, bộ nhớ, mạng và quyền ưu tiên theo tác vụ bạn đang làm.")
                    .size(12.0)
                    .color(p.text_secondary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(RichText::new(format!("Đang chạy: {}", self.current_scenario.name())).size(12.0).strong().color(p.accent_blue));
            });
        });
        ui.add_space(4.0);

        ui.horizontal(|ui| {
            if ui.button(RichText::new("📋 Xem bảng ma trận 10 tiêu chí setup chi tiết (2 Cột) ->").size(11.5).strong().color(p.accent_blue)).clicked() {
                self.current_tab = Tab::Criteria;
            }
        });
        ui.add_space(4.0);

        if !self.scenario_status_msg.is_empty() {
            p.modern_card(ui, |ui| {
                ui.label(RichText::new(&self.scenario_status_msg).size(12.5).strong().color(p.success_green));
            });
            ui.add_space(6.0);
        }

        let scenarios = [
            (
                Scenario::RemoteDesktop,
                "🖥️ Điều khiển từ xa (Remote Desktop)",
                "Dành riêng cho MiniRemote, RustDesk, RDP Client",
                "• Khóa chống Sleep & Lock màn hình (SetThreadExecutionState)\n• CPU 65% máy mát 40°C, quạt êm ru 0dB, pin dùng 6 - 8 tiếng\n• Intel QuickSync giải mã video mượt mà, ưu tiên mạng & giảm trễ chuột\n• Đóng băng app sync ngầm để giữ trọn băng thông điều khiển",
            ),
            (
                Scenario::Gaming,
                "🎮 Chơi Game & Đồ họa",
                "Chống tụt FPS, giảm giật khung hình, mở rộng VRAM",
                "• Windows Timer 1.0ms triệt tiêu độ trễ chuột & hiện tượng micro-stutter\n• Tắt GameDVR background capture ngầm để giải phóng bộ mã hóa GPU\n• Khóa CPU 90% (2.8GHz) nhường trọn 12W cho GPU Intel HD 620 đạt đỉnh 1150MHz\n• Thu hồi Working Set RAM để mở rộng dung lượng Shared VRAM",
            ),
            (
                Scenario::CafeMobility,
                "☕ Cà phê / Siêu tiết kiệm pin",
                "Tối đa từng % pin khi làm việc di động không cắm sạc",
                "• Khóa trần CPU 60% chỉ tiêu thụ 3W - 5W điện năng\n• Tắt quạt tản nhiệt (Passive cooling), độ sáng tự động 45%\n• Đóng băng Google Drive, OneDrive, Tailscale và các app chạy ngầm",
            ),
            (
                Scenario::Developer,
                "💻 Lập trình & Biên dịch (Developer)",
                "Tối đa tốc độ build code cho Rust, Go, Web",
                "• Mở 100% Turbo Boost 3.9GHz toàn bộ 2 nhân 4 luồng\n• Quạt làm mát chủ động (Active), ưu tiên CPU cho rustc, cargo, go\n• Dọn sạch RAM để compiler đọc ghi file SSD NVMe nhanh nhất",
            ),
            (
                Scenario::MediaChill,
                "🎬 Xem phim & Thư giãn",
                "Thưởng thức video êm ái, màn hình không tắt",
                "• Khóa chống tắt màn hình và ngăn sleep khi đang xem video\n• CPU 55% im lặng tuyệt đối 0dB, màn hình sáng 70% sống động",
            ),
            (
                Scenario::Default,
                "⚖️ Cân bằng tiêu chuẩn (Windows Default)",
                "Khôi phục cài đặt mặc định của Windows",
                "• Phục hồi cơ chế điều tiết năng lượng tiêu chuẩn của Windows\n• Mở lại GameDVR và trả lại quyền ưu tiên ứng dụng bình thường",
            ),
        ];

        for (scen, title, subtitle, details) in scenarios {
            let is_active = self.current_scenario == scen;

            let card_frame = egui::Frame::new()
                .inner_margin(egui::Margin::same(10))
                .corner_radius(CornerRadius::same(8))
                .fill(if is_active {
                    if p.is_dark { Color32::from_rgb(24, 38, 68) } else { Color32::from_rgb(238, 246, 255) }
                } else {
                    p.card_bg
                })
                .stroke(Stroke::new(
                    if is_active { 2.0 } else { 1.0 },
                    if is_active { p.accent_blue } else { p.card_border },
                ));

            card_frame.show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.label(RichText::new(title).size(14.0).strong().color(if is_active { p.accent_blue } else { p.text_primary }));
                        ui.label(RichText::new(subtitle).size(11.5).strong().color(p.text_secondary));
                    });

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if is_active {
                            ui.label(RichText::new("✓ ĐANG HOẠT ĐỘNG").size(12.0).strong().color(p.success_green));
                        } else {
                            let btn = egui::Button::new(
                                RichText::new("⚡ KÍCH HOẠT").size(12.0).strong().color(Color32::WHITE),
                            )
                            .fill(p.primary_btn)
                            .corner_radius(CornerRadius::same(5));

                            if ui.add(btn).clicked() {
                                self.current_scenario = scen;
                                self.scenario_status_msg = apply_scenario(scen, &mut self.sys);
                                self.mem_stats = get_memory_stats();
                                self.power_status = get_power_status();
                            }
                        }
                    });
                });

                ui.add_space(4.0);
                ui.label(RichText::new(details).size(11.5).color(p.text_secondary));
            });

            ui.add_space(4.0);
        }
    }

    fn render_criteria(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            ui.heading(
                RichText::new("📋 Bảng Tiêu Chí Setup & Tinh Chỉnh Thông Số")
                    .size(16.0)
                    .strong()
                    .color(p.text_primary),
            );
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let btn = egui::Button::new(
                    RichText::new("🎯 Đến trang Kịch bản").size(11.5).strong().color(Color32::WHITE),
                )
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(5));

                if ui.add(btn).clicked() {
                    self.current_tab = Tab::Scenarios;
                }
            });
        });

        ui.label(
            RichText::new(
                "Cột 1: Tiêu chí phần cứng (gọn gàng) • Cột 2: Bộ tinh chỉnh thông số tương tác trực tiếp & kiến nghị tối ưu.",
            )
            .size(12.0)
            .color(p.text_secondary),
        );
        ui.add_space(4.0);

        if !self.criteria_feedback_msg.is_empty() {
            p.modern_card(ui, |ui| {
                ui.label(RichText::new(&self.criteria_feedback_msg).size(12.0).strong().color(p.success_green));
            });
            ui.add_space(4.0);
        }

        // Header Row for the 2-column table
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(
                    Vec2::new(160.0, 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.set_width(160.0);
                        ui.label(
                            RichText::new("📌 TIÊU CHÍ")
                                .size(12.0)
                                .strong()
                                .color(p.accent_blue),
                        );
                    },
                );
                ui.separator();
                ui.allocate_ui_with_layout(
                    Vec2::new(ui.available_width(), 0.0),
                    egui::Layout::top_down(egui::Align::Min),
                    |ui| {
                        ui.label(
                            RichText::new("🛠️ TINH CHỈNH THÔNG SỐ TRỰC TIẾP & KIẾN NGHỊ TỐI ƯU")
                                .size(12.0)
                                .strong()
                                .color(p.accent_blue),
                        );
                    },
                );
            });
        });
        ui.add_space(4.0);

        // --- ROW 1: CPU XUNG NHỊP & TDP ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("1. ⚡ Xung CPU & TDP").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("i7-7600U (50-100%), trần Turbo, TDP 15-25W.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("powercfg PROCTHROTTLEMAX").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Pin (DC):").size(11.5).strong().color(p.text_primary));
                        if ui.add(egui::Slider::new(&mut self.cpu_limit_dc, 50..=100).suffix("%")).changed() {
                            let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
                            self.criteria_feedback_msg = format!("✓ Đã áp dụng trần CPU pin: {}%", self.cpu_limit_dc);
                        }
                        if ui.button(RichText::new("60% Pin").size(10.5)).clicked() {
                            self.cpu_limit_dc = 60;
                            let _ = set_cpu_max_state_dc(60);
                        }
                        if ui.button(RichText::new("80%").size(10.5)).clicked() {
                            self.cpu_limit_dc = 80;
                            let _ = set_cpu_max_state_dc(80);
                        }
                        if ui.button(RichText::new("100%").size(10.5)).clicked() {
                            self.cpu_limit_dc = 100;
                            let _ = set_cpu_max_state_dc(100);
                        }
                    });

                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Sạc (AC):").size(11.5).strong().color(p.text_primary));
                        if ui.add(egui::Slider::new(&mut self.cpu_limit_ac, 50..=100).suffix("%")).changed() {
                            let _ = set_cpu_max_state_ac(self.cpu_limit_ac);
                            self.criteria_feedback_msg = format!("✓ Đã áp dụng trần CPU sạc: {}%", self.cpu_limit_ac);
                        }
                        if ui.button(RichText::new("80% Mát").size(10.5)).clicked() {
                            self.cpu_limit_ac = 80;
                            let _ = set_cpu_max_state_ac(80);
                        }
                        if ui.button(RichText::new("100% Turbo").size(10.5)).clicked() {
                            self.cpu_limit_ac = 100;
                            let _ = set_cpu_max_state_ac(100);
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Pin 60-80% để triệt tiêu Turbo 25W; Sạc 100% để max xung 3.9GHz.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 2: TIMER RESOLUTION ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("2. ⏱️ Timer & FPS").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Chu kỳ ngắt nhân Kernel (15.6ms vs 1.0ms tối ưu).").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("Win32 timeBeginPeriod(1)").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        let btn_1ms = egui::Button::new(RichText::new("🚀 1.0 ms (Game & Remote)").size(11.0).strong().color(Color32::WHITE))
                            .fill(if self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_1ms).clicked() {
                            self.timer_1ms_enabled = true;
                            set_timer_resolution_1ms(true);
                            self.criteria_feedback_msg = "✓ Đã kích hoạt Timer 1.0ms (Triệt tiêu micro-stutter & lag chuột).".to_string();
                        }

                        let btn_15ms = egui::Button::new(RichText::new("🍃 15.6 ms (Mặc định OS)").size(11.0).strong().color(if !self.timer_1ms_enabled { Color32::WHITE } else { p.text_primary }))
                            .fill(if !self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_15ms).clicked() {
                            self.timer_1ms_enabled = false;
                            set_timer_resolution_1ms(false);
                            self.criteria_feedback_msg = "✓ Đã đưa Timer về 15.6ms mặc định để tiết kiệm pin.".to_string();
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: 1.0ms triệt tiêu giật khung hình & trễ chuột; 15.6ms giảm wake CPU tiết kiệm pin.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 3: ECOQOS BACKGROUND THROTTLING ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("3. 🍃 EcoQoS App Nền").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Ép tiến trình chạy ngầm vào chế độ tiết kiệm điện.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("SetProcessInformation (ProcessPowerThrottling)").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.eco_qos_enabled, RichText::new("Bật EcoQoS tự động").size(11.5).strong().color(p.accent_blue));
                        if ui.button(RichText::new("⚡ Ép EcoQoS app ngầm ngay").size(11.0)).clicked() {
                            let (_, count) = apply_foreground_boost_and_eco(&self.sys, self.foreground_info.as_ref().map(|(pid, _)| *pid));
                            self.criteria_feedback_msg = format!("✓ Đã áp dụng EcoQoS hạ tải {} tiến trình chạy ngầm.", count);
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Luôn bật để chặn Chrome/Edge/GoogleDrive chạy ngầm ngốn trộm CPU và pin.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 4: RAM WORKING SET & AUTO TRIM ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("4. 🧠 RAM & Shared VRAM").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Thu hồi Working Set nhàn rỗi, mở rộng VRAM cho iGPU.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("Win32 K32EmptyWorkingSet").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        ui.checkbox(&mut self.auto_ram_trim_enabled, RichText::new("Tự dọn khi RAM >").size(11.5).strong().color(p.text_primary));
                        ui.add(egui::Slider::new(&mut self.auto_ram_threshold, 70..=95).suffix("%"));
                        if ui.button(RichText::new("⚡ Thu hồi ngay").size(11.0)).clicked() {
                            let (freed, count) = trim_working_sets(&mut self.sys);
                            self.mem_stats = get_memory_stats();
                            self.criteria_feedback_msg = format!("✓ Đã thu hồi {} từ {} tiến trình!", format_bytes(freed), count);
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Đặt ngưỡng 80-85% để tránh đầy RAM mà không gây gián đoạn ứng dụng.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 5: NETWORK BUFFER & SYNC FREEZE ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("5. 🌐 Băng thông & Mạng").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("TCP buffer & đóng băng sync chống nghẽn đường truyền.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("netsh int tcp & Background Freeze").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("⚡ Tối ưu TCP Low-Latency").size(11.0)).clicked() {
                            let _ = std::process::Command::new("netsh")
                                .args(["int", "tcp", "set", "global", "autotuninglevel=normal"])
                                .status();
                            self.criteria_feedback_msg = "✓ Đã tối ưu TCP ACK buffer giảm độ trễ mạng.".to_string();
                        }
                        if ui.button(RichText::new("⏸️ Đóng băng Sync").size(11.0)).clicked() {
                            let c = crate::scenarios::prioritize_apps(&mut self.sys, &["GoogleDriveFS", "OneDrive", "SearchIndexer"], windows_sys::Win32::System::Threading::IDLE_PRIORITY_CLASS);
                            self.criteria_feedback_msg = format!("✓ Đã hạ ưu tiên {} app sync để nhường mạng.", c);
                        }
                        if ui.button(RichText::new("▶️ Phục hồi").size(11.0)).clicked() {
                            let c = crate::scenarios::prioritize_apps(&mut self.sys, &["GoogleDriveFS", "OneDrive", "SearchIndexer"], windows_sys::Win32::System::Threading::NORMAL_PRIORITY_CLASS);
                            self.criteria_feedback_msg = format!("✓ Đã phục hồi {} app sync về bình thường.", c);
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Bật TCP Low-Latency & đóng băng sync khi remote hoặc chơi game để giữ ping ổn định.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 6: SLEEP & DISPLAY LOCK ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("6. 🔌 Chống Sleep / Khóa").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Giữ máy/màn hình luôn sáng khi remote/xem video.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("Win32 SetThreadExecutionState").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        if ui.selectable_value(&mut self.keep_awake_mode, 0, "Mặc định OS").clicked() {
                            set_keep_awake(0);
                            self.criteria_feedback_msg = "✓ Chế độ ngủ theo cài đặt mặc định của Windows.".to_string();
                        }
                        if ui.selectable_value(&mut self.keep_awake_mode, 1, "Giữ máy thức").clicked() {
                            set_keep_awake(1);
                            self.criteria_feedback_msg = "✓ Đã bật giữ máy luôn chạy (không sleep).".to_string();
                        }
                        if ui.selectable_value(&mut self.keep_awake_mode, 2, "Giữ cả Màn hình").clicked() {
                            set_keep_awake(2);
                            self.criteria_feedback_msg = "✓ Đã bật giữ cả màn hình và máy luôn sáng.".to_string();
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Chọn 'Giữ cả Màn hình' khi remote máy chủ hoặc xem phim để không bị đứt kết nối.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 7: FAN & COOLING POLICY ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("7. ❄️ Quạt tản nhiệt").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Chế độ làm mát bo mạch chủ (Active vs Passive).").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("powercfg SYSCOOLPOL").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        let btn_pass = egui::Button::new(RichText::new("🍃 Thụ động (Passive 0dB)").size(11.0).strong().color(if !self.cooling_active { Color32::WHITE } else { p.text_primary }))
                            .fill(if !self.cooling_active { Color32::from_rgb(16, 120, 60) } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_pass).clicked() {
                            self.cooling_active = false;
                            let _ = set_cooling_policy(false);
                            self.criteria_feedback_msg = "✓ Đã bật tản nhiệt thụ động: Tắt quạt, tiết kiệm điện, êm ru 0dB.".to_string();
                        }

                        let btn_act = egui::Button::new(RichText::new("🚀 Chủ động (Active)").size(11.0).strong().color(if self.cooling_active { Color32::WHITE } else { p.text_primary }))
                            .fill(if self.cooling_active { p.primary_btn } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_act).clicked() {
                            self.cooling_active = true;
                            let _ = set_cooling_policy(true);
                            self.criteria_feedback_msg = "✓ Đã bật tản nhiệt chủ động: Quạt làm mát sớm chống hãm xung CPU.".to_string();
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Passive cho cafe/remote êm ái; Active cho game & compile để chống nghẽn nhiệt.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 8: DISPLAY BRIGHTNESS ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("8. 🔆 Độ sáng Màn hình").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Mức tiêu thụ panel LED (chiếm 30-50% tổng điện pin).").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("WMI ACPI WmiSetBrightness").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        if ui.add(egui::Slider::new(&mut self.brightness_level, 10..=100).suffix("%")).changed() {
                            let _ = set_brightness(self.brightness_level);
                        }
                        if ui.button(RichText::new("45% Pin").size(10.5)).clicked() {
                            self.brightness_level = 45;
                            let _ = set_brightness(45);
                        }
                        if ui.button(RichText::new("50% VP").size(10.5)).clicked() {
                            self.brightness_level = 50;
                            let _ = set_brightness(50);
                        }
                        if ui.button(RichText::new("70% Phim").size(10.5)).clicked() {
                            self.brightness_level = 70;
                            let _ = set_brightness(70);
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Cà phê/Pin: 40-45% là ngưỡng tối ưu nhất; Văn phòng: 50%; Phim: 70%.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 9: GAMEDVR BACKGROUND CAPTURE ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("9. 🎬 Windows GameDVR").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Quay màn hình nền giải phóng encoder GPU Intel.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("Registry GameDVR (AppCaptureEnabled)").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        let btn_off = egui::Button::new(RichText::new("❌ Tắt GameDVR (Tối ưu FPS)").size(11.0).strong().color(if !self.gamedvr_enabled { Color32::WHITE } else { p.text_primary }))
                            .fill(if !self.gamedvr_enabled { p.danger_red } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_off).clicked() {
                            self.gamedvr_enabled = false;
                            let _ = set_gamedvr(false);
                            self.criteria_feedback_msg = "✓ Đã tắt GameDVR nền để giải phóng GPU Intel HD 620.".to_string();
                        }

                        let btn_on = egui::Button::new(RichText::new("✓ Bật GameDVR").size(11.0).strong().color(if self.gamedvr_enabled { Color32::WHITE } else { p.text_primary }))
                            .fill(if self.gamedvr_enabled { p.primary_btn } else { p.tab_inactive_bg })
                            .corner_radius(CornerRadius::same(4));
                        if ui.add(btn_on).clicked() {
                            self.gamedvr_enabled = true;
                            let _ = set_gamedvr(true);
                            self.criteria_feedback_msg = "✓ Đã bật lại GameDVR theo mặc định của Windows.".to_string();
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Tắt GameDVR khi chơi game để giải phóng GPU encoder, tránh sụt frame.").size(11.0).color(p.text_secondary));
                });
            });
        });
        ui.add_space(4.0);

        // --- ROW 10: BATTERY THRESHOLD & HIBERNATE ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.allocate_ui_with_layout(Vec2::new(160.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.set_width(160.0);
                    ui.label(RichText::new("10. 🛡️ Ngưỡng sạc & Ngủ đông").size(12.5).strong().color(p.text_primary));
                    ui.add_space(2.0);
                    ui.label(RichText::new("Khóa sạc 80% chống phồng pin & ngủ đông Hibernate.").size(11.0).color(p.text_secondary));
                });
                ui.separator();
                ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Công cụ:").size(11.0).color(p.text_secondary));
                        ui.label(RichText::new("Lenovo PM Service & powercfg /hibernate").size(11.0).strong().color(p.accent_blue));
                    });

                    ui.horizontal(|ui| {
                        let thresh_text = if self.protection.battery_threshold_enabled { "Tắt ngưỡng 80%" } else { "Bật ngưỡng 80% (Admin)" };
                        let thresh_color = if self.protection.battery_threshold_enabled { p.danger_red } else { Color32::from_rgb(16, 120, 60) };
                        if ui.add(egui::Button::new(RichText::new(thresh_text).size(11.0).strong().color(Color32::WHITE)).fill(thresh_color).corner_radius(CornerRadius::same(4))).clicked() {
                            let new_st = !self.protection.battery_threshold_enabled;
                            let _ = set_battery_threshold(new_st, 80);
                            self.protection.battery_threshold_enabled = new_st;
                            self.criteria_feedback_msg = format!("✓ Đã {} ngưỡng sạc 80% bảo vệ pin.", if new_st { "bật" } else { "tắt" });
                        }

                        if !self.protection.is_hibernate_enabled {
                            if ui.button(RichText::new("Bật Hibernate (Admin)").size(11.0)).clicked() {
                                let _ = enable_hibernate_as_admin();
                                self.protection.is_hibernate_enabled = true;
                                self.criteria_feedback_msg = "✓ Đã gửi lệnh bật chế độ ngủ đông Hibernate.".to_string();
                            }
                        } else {
                            ui.label(RichText::new("✓ Hibernate Đã bật").size(11.5).strong().color(p.success_green));
                        }
                    });

                    ui.label(RichText::new("💡 Kiến nghị: Bật ngưỡng 80% khi cắm sạc bàn làm việc; Dùng Hibernate thay vì Sleep S3.").size(11.0).color(p.text_secondary));
                });
            });
        });
    }

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        // PC Health Score Card
        let (score, score_label, score_color) = self.calculate_health_score();
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("CHỈ SỐ SỨC KHỎE HỆ THỐNG (PC HEALTH SCORE)").size(11.5).strong().color(p.accent_blue));
                    ui.horizontal(|ui| {
                        ui.heading(RichText::new(format!("{}/100", score)).size(28.0).strong().color(score_color));
                        ui.vertical(|ui| {
                            ui.label(RichText::new(score_label).size(12.5).strong().color(score_color));
                            let hint = if score >= 90 {
                                "Hệ thống đang hoạt động ở trạng thái tối ưu lý tưởng."
                            } else {
                                "Khuyến nghị bấm 'Tối ưu toàn diện' để dọn rác và giải phóng RAM."
                            };
                            ui.label(RichText::new(hint).size(11.0).color(p.text_secondary));
                        });
                    });
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let opt_btn = egui::Button::new(
                        RichText::new("⚡ TỐI ƯU TOÀN DIỆN").size(13.0).strong().color(Color32::WHITE),
                    )
                    .fill(p.primary_btn)
                    .corner_radius(CornerRadius::same(6));

                    if ui.add(opt_btn).clicked() {
                        self.run_quick_optimize();
                    }
                });
            });

            if !self.quick_opt_msg.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&self.quick_opt_msg)
                        .size(11.5)
                        .color(p.success_green)
                        .strong(),
                );
            }
        });

        ui.add_space(8.0);

        // Metric Cards Row
        ui.columns(3, |cols| {
            // CPU Card
            p.modern_card(&mut cols[0], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("CPU").size(12.0).strong().color(p.text_secondary));
                    let cpu = self.protection.cpu_usage;
                    let cpu_color = if cpu > 80.0 { p.danger_red } else { p.accent_blue };
                    ui.heading(RichText::new(format!("{:.1}%", cpu)).size(26.0).strong().color(cpu_color));
                    ui.add_space(2.0);
                    ui.label(RichText::new(&self.cpu_brand).size(12.0).strong().color(p.text_primary));
                    ui.label(RichText::new(&self.cpu_cores_info).size(11.5).color(p.text_secondary));
                });
            });

            // RAM Card
            p.modern_card(&mut cols[1], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("RAM").size(12.0).strong().color(p.text_secondary));
                    let load = self.mem_stats.load_percent;
                    let ram_color = if load > 85 { p.danger_red } else { p.success_green };
                    ui.heading(RichText::new(format!("{}%", load)).size(26.0).strong().color(ram_color));
                    ui.add_space(2.0);
                    ui.label(
                        RichText::new(format!(
                            "{} / {}",
                            format_bytes(self.mem_stats.used_bytes),
                            format_bytes(self.mem_stats.total_bytes)
                        ))
                        .size(12.0)
                        .strong()
                        .color(p.text_primary),
                    );
                    ui.label(
                        RichText::new(format!("Trống: {}", format_bytes(self.mem_stats.free_bytes)))
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });
            });

            // Battery Card
            p.modern_card(&mut cols[2], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("PIN").size(12.0).strong().color(p.text_secondary));
                    let bat = self.power_status.battery_percent;
                    let bat_color = if bat < 20 { p.danger_red } else { p.warn_amber };
                    ui.heading(RichText::new(format!("{}%", bat)).size(26.0).strong().color(bat_color));
                    ui.add_space(2.0);
                    let status = if self.power_status.is_charging {
                        "⚡ Đang nạp sạc...".to_string()
                    } else if self.power_status.is_ac_online {
                        "Đã cắm sạc đầy (AC)".to_string()
                    } else if let Some(sec) = self.power_status.remaining_seconds {
                        format!("Còn ~{}h {:02}m", sec / 3600, (sec % 3600) / 60)
                    } else {
                        "Đang dùng pin".to_string()
                    };
                    ui.label(RichText::new(status).size(12.0).strong().color(p.text_primary));

                    let power_rate_str = if self.power_status.discharge_rate_mw > 0 {
                        format!("Tiêu thụ: {:.1} W", self.power_status.power_watts)
                    } else if self.power_status.charge_rate_mw > 0 {
                        format!("Nạp vào: {:.1} W", self.power_status.power_watts)
                    } else if self.power_status.voltage_volts > 0.0 {
                        format!("Điện áp: {:.1} V", self.power_status.voltage_volts)
                    } else if self.dual_batteries.len() > 1 {
                        format!("{} pack pin", self.dual_batteries.len())
                    } else {
                        "1 pack pin".to_string()
                    };
                    ui.label(RichText::new(power_rate_str).size(11.5).color(p.text_secondary));
                });
            });
        });

        ui.add_space(8.0);

        // Foreground Window & EcoQoS Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("ỨNG DỤNG ĐANG FOCUS:").size(11.5).strong().color(p.accent_blue));
                        if let Some((_, ref name)) = self.foreground_info {
                            ui.label(RichText::new(name).size(12.5).strong().color(p.text_primary));
                        } else {
                            ui.label(RichText::new("Chưa xác định").size(12.0).color(p.text_secondary));
                        }
                    });
                    if !self.eco_boost_status.is_empty() {
                        ui.label(RichText::new(&self.eco_boost_status).size(11.5).color(p.success_green));
                    } else {
                        ui.label(RichText::new("Tự động ưu tiên xung nhịp cho cửa sổ đang dùng, hạ tải EcoQoS app ngầm.").size(11.5).color(p.text_secondary));
                    }
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(&mut self.eco_qos_enabled, RichText::new("EcoQoS Bật").size(12.0).strong().color(p.accent_blue));
                });
            });
        });

        ui.add_space(8.0);

        // Active Scenario Banner on Dashboard
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("KỊCH BẢN ĐANG HOẠT ĐỘNG").size(11.5).strong().color(p.accent_blue));
                    ui.label(RichText::new(self.current_scenario.name()).size(14.0).strong().color(p.text_primary));
                    ui.label(RichText::new(self.current_scenario.description()).size(12.0).color(p.text_secondary));
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn = egui::Button::new(
                        RichText::new("🎯 ĐỔI KỊCH BẢN").size(12.0).strong().color(Color32::WHITE),
                    )
                    .fill(p.primary_btn)
                    .corner_radius(CornerRadius::same(5));

                    if ui.add(btn).clicked() {
                        self.current_tab = Tab::Scenarios;
                    }
                });
            });
        });

        ui.add_space(8.0);

        // Hardware Highlights Card
        p.modern_card(ui, |ui| {
            ui.label(RichText::new("THÔNG TIN THIẾT BỊ").size(12.0).strong().color(p.accent_blue));
            ui.separator();
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Thiết bị:").size(12.0).strong().color(p.text_secondary));
                ui.label(RichText::new(&self.protection.device_name).size(12.0).strong().color(p.text_primary));
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new("Pin thực tế:").size(12.0).strong().color(p.text_secondary));
                if !self.dual_batteries.is_empty() {
                    let full: u64 = self.dual_batteries.iter().map(|b| b.full_charge_capacity_mwh).sum();
                    let design: u64 = self.dual_batteries.iter().map(|b| b.design_capacity_mwh).sum();
                    ui.label(
                        RichText::new(format!("{} mWh / {} mWh gốc", full, design))
                            .size(12.0)
                            .strong()
                            .color(p.text_primary),
                    );
                } else {
                    ui.label("Đang đọc cảm biến ACPI...");
                }
            });

            ui.horizontal(|ui| {
                ui.label(RichText::new("Hibernate:").size(12.0).strong().color(p.text_secondary));
                if self.protection.is_hibernate_enabled {
                    ui.label(RichText::new("✓ Đã bật (Ngắt điện chống sụt pin)").size(12.0).color(p.success_green).strong());
                } else {
                    ui.label(RichText::new("✗ Chưa bật").size(12.0).color(p.danger_red).strong());
                }
            });
        });
    }

    fn render_cleaner(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🧹 Dọn dẹp rác 3 tầng & Cache hệ thống").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Phân loại 3 tầng: Rác an toàn 100%, Cache lập trình viên (NPM/Go/Rust) và Bảo trì hệ thống.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let scan_btn = egui::Button::new(
                RichText::new("🔍 Quét toàn bộ").size(12.5).strong().color(Color32::WHITE),
            )
            .fill(p.primary_btn)
            .corner_radius(CornerRadius::same(5));

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
                self.clean_status_msg = format!("✓ Quét xong! Tìm thấy {}.", format_bytes(total_size));
            }

            let clean_safe_btn = egui::Button::new(
                RichText::new("🛡️ Dọn Rác An Toàn (Tầng 1)").size(12.5).strong().color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(16, 120, 60))
            .corner_radius(CornerRadius::same(5));

            if ui.add(clean_safe_btn).clicked() {
                let targets = get_default_targets();
                let mut freed_total = 0u64;
                let mut files_total = 0usize;

                for target in &targets {
                    if target.tier == CleanTier::Safe {
                        let (f_bytes, f_count) = clean_target(target);
                        freed_total += f_bytes;
                        files_total += f_count;
                    }
                }

                for (i, target) in targets.iter().enumerate() {
                    let (size, count) = scan_target(target);
                    if let Some(cat) = self.clean_categories.get_mut(i) {
                        cat.size_bytes = size;
                        cat.file_count = count;
                    }
                }

                self.clean_status_msg = format!(
                    "✓ Đã dọn an toàn {} ({} files).",
                    format_bytes(freed_total),
                    files_total
                );
            }

            let clean_dev_btn = egui::Button::new(
                RichText::new("💻 Dọn Dev Cache (Tầng 2)").size(12.5).strong().color(Color32::WHITE),
            )
            .fill(Color32::from_rgb(147, 51, 234))
            .corner_radius(CornerRadius::same(5));

            if ui.add(clean_dev_btn).clicked() {
                let targets = get_default_targets();
                let mut freed_total = 0u64;
                let mut files_total = 0usize;

                for target in &targets {
                    if target.tier == CleanTier::Developer {
                        let (f_bytes, f_count) = clean_target(target);
                        freed_total += f_bytes;
                        files_total += f_count;
                    }
                }

                for (i, target) in targets.iter().enumerate() {
                    let (size, count) = scan_target(target);
                    if let Some(cat) = self.clean_categories.get_mut(i) {
                        cat.size_bytes = size;
                        cat.file_count = count;
                    }
                }

                self.clean_status_msg = format!(
                    "✓ Đã dọn Dev Cache {} ({} files).",
                    format_bytes(freed_total),
                    files_total
                );
            }

            let clean_btn = egui::Button::new(
                RichText::new("🗑️ Dọn tất cả đã chọn").size(12.5).strong().color(Color32::WHITE),
            )
            .fill(p.danger_red)
            .corner_radius(CornerRadius::same(5));

            if ui.add(clean_btn).clicked() {
                let targets = get_default_targets();
                let mut freed_total = 0u64;
                let mut files_total = 0usize;

                for (i, target) in targets.iter().enumerate() {
                    if let Some(cat) = self.clean_categories.get(i) {
                        if cat.selected {
                            let (f_bytes, f_count) = clean_target(target);
                            freed_total += f_bytes;
                            files_total += f_count;
                        }
                    }
                }

                for (i, target) in targets.iter().enumerate() {
                    let (size, count) = scan_target(target);
                    if let Some(cat) = self.clean_categories.get_mut(i) {
                        cat.size_bytes = size;
                        cat.file_count = count;
                    }
                }

                self.clean_status_msg = format!(
                    "✓ Đã dọn thành công {} ({} files).",
                    format_bytes(freed_total),
                    files_total
                );
            }
        });

        if !self.clean_status_msg.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new(&self.clean_status_msg)
                    .size(12.5)
                    .strong()
                    .color(p.accent_blue),
            );
        }

        ui.add_space(8.0);

        // Group by Tiers: Safe, Developer, SystemMaintenance
        let tiers = [
            (CleanTier::Safe, "🛡️ TẦNG 1: RÁC AN TOÀN 100% (TEMP & BROWSER)", p.success_green),
            (CleanTier::Developer, "💻 TẦNG 2: RÁC PHÁT TRIỂN PHẦN MỀM (NPM, GO, RUST CACHE)", Color32::from_rgb(168, 85, 247)),
            (CleanTier::SystemMaintenance, "⚙️ TẦNG 3: BẢO TRÌ HỆ THỐNG SÂU (UPDATE & PREFETCH)", p.warn_amber),
        ];

        for (tier, header, color) in tiers {
            p.modern_card(ui, |ui| {
                ui.label(RichText::new(header).size(12.0).strong().color(color));
                ui.separator();

                for cat in &mut self.clean_categories {
                    if cat.tier == tier {
                        ui.horizontal(|ui| {
                            ui.checkbox(&mut cat.selected, "");
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&cat.name).size(13.0).strong().color(p.text_primary));
                                ui.label(RichText::new(&cat.path_desc).size(11.0).color(p.text_secondary));
                            });

                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let text = if cat.size_bytes > 0 {
                                    format!("{} ({} f)", format_bytes(cat.size_bytes), cat.file_count)
                                } else {
                                    "0 B".to_string()
                                };
                                ui.label(RichText::new(text).size(12.5).strong().color(p.accent_blue));
                            });
                        });
                        ui.separator();
                    }
                }
            });
            ui.add_space(6.0);
        }
    }

    fn render_memory(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            ui.heading(RichText::new("🧠 Tối ưu RAM & Tiến trình").size(16.0).strong().color(p.text_primary));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.checkbox(
                    &mut self.auto_ram_trim_enabled,
                    RichText::new("Tự động dọn khi RAM > 85%").size(11.5).strong().color(p.accent_blue),
                );
            });
        });

        ui.label(
            RichText::new("Thu hồi Working Set nhàn rỗi, giám sát và quản lý trực tiếp từng tiến trình.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        p.modern_card(ui, |ui| {
            let frac = self.mem_stats.load_percent as f32 / 100.0;
            ui.horizontal(|ui| {
                ui.label(RichText::new("Sử dụng RAM:").strong().color(p.text_primary));
                ui.label(
                    RichText::new(format!(
                        "{} / {} ({}%)",
                        format_bytes(self.mem_stats.used_bytes),
                        format_bytes(self.mem_stats.total_bytes),
                        self.mem_stats.load_percent
                    ))
                    .strong()
                    .color(if frac > 0.85 { p.danger_red } else { p.success_green }),
                );
            });

            ui.add_space(4.0);
            ui.add(ProgressBar::new(frac).desired_height(14.0));
            ui.add_space(8.0);

            ui.horizontal(|ui| {
                let trim_btn = egui::Button::new(
                    RichText::new("⚡ Thu hồi RAM toàn hệ thống").size(13.0).strong().color(Color32::WHITE),
                )
                .fill(p.primary_btn)
                .corner_radius(CornerRadius::same(6));

                if ui.add(trim_btn).clicked() {
                    let (freed, count) = trim_working_sets(&mut self.sys);
                    self.mem_stats = get_memory_stats();
                    self.top_processes = get_top_processes(&mut self.sys, 25);
                    self.quick_opt_msg = format!("✓ Đã thu hồi {} từ {} tiến trình!", format_bytes(freed), count);
                }

                if ui.button(RichText::new("🔄 Làm mới").size(12.0)).clicked() {
                    self.top_processes = get_top_processes(&mut self.sys, 25);
                }
            });

            if !self.quick_opt_msg.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&self.quick_opt_msg)
                        .size(12.5)
                        .color(p.success_green)
                        .strong(),
                );
            }
        });

        ui.add_space(8.0);

        // Process Inspector Section
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("GIÁM SÁT TIẾN TRÌNH (PROCESS INSPECTOR)").size(12.0).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.process_search_query)
                            .hint_text("🔍 Lọc tên app hoặc PID...")
                            .desired_width(160.0),
                    );
                });
            });

            if !self.process_action_msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&self.process_action_msg).size(12.0).strong().color(p.success_green));
            }

            ui.separator();
            ui.add_space(2.0);

            let query = self.process_search_query.trim().to_lowercase();
            let filtered_procs: Vec<_> = self
                .top_processes
                .iter()
                .filter(|p| {
                    if query.is_empty() {
                        true
                    } else {
                        p.name.to_lowercase().contains(&query) || p.pid.to_string().contains(&query)
                    }
                })
                .collect();

            egui::Grid::new("proc_grid")
                .striped(true)
                .min_col_width(50.0)
                .spacing(Vec2::new(10.0, 5.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("PID").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Ứng dụng").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("RAM").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Hành động").size(11.5).strong().color(p.text_secondary));
                    ui.end_row();

                    for proc in filtered_procs {
                        ui.label(RichText::new(proc.pid.to_string()).size(11.0).color(p.text_secondary));
                        ui.label(RichText::new(&proc.name).size(11.5).strong().color(p.text_primary));
                        ui.label(RichText::new(format_bytes(proc.memory_bytes)).size(11.5).color(p.accent_blue).strong());

                        ui.horizontal(|ui| {
                            // Trim button
                            if ui.button(RichText::new("⚡ Dọn").size(10.5)).clicked() {
                                let freed = trim_single_process(proc.pid);
                                self.process_action_msg = format!("✓ Đã thu hồi {} từ '{}' (PID {})", format_bytes(freed), proc.name, proc.pid);
                                self.mem_stats = get_memory_stats();
                            }

                            // Eco button
                            if ui.button(RichText::new("🍃 Eco").size(10.5)).clicked() {
                                if set_process_eco_mode(proc.pid, true) {
                                    self.process_action_msg = format!("✓ Đã bật EcoQoS cho '{}' (PID {})", proc.name, proc.pid);
                                }
                            }

                            // Terminate button
                            if ui.add(egui::Button::new(RichText::new("❌").size(10.5).color(p.danger_red))).clicked() {
                                if terminate_process(proc.pid) {
                                    self.process_action_msg = format!("✓ Đã kết thúc tiến trình '{}' (PID {})", proc.name, proc.pid);
                                }
                            }
                        });

                        ui.end_row();
                    }
                });
        });
    }

    fn render_power(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🔋 Pin & Cảm biến ACPI").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Điều khiển xung nhịp CPU, độ sáng và giám sát dòng xả Watt thời gian thực.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        // Power Profiles Card
        p.modern_card(ui, |ui| {
            let active_scheme = get_active_scheme_name();
            ui.horizontal(|ui| {
                ui.label(RichText::new("CẤU HÌNH NGUỒN").size(12.0).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(RichText::new(format!("Hiện tại: {}", active_scheme)).size(11.5).strong().color(p.success_green));
                });
            });
            ui.add_space(4.0);

            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("🍃 Tiết kiệm").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(16, 120, 60))
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    let _ = set_power_scheme("SCHEME_MAX");
                    let _ = set_cpu_max_state_dc(75);
                    self.power_status_msg = "✓ Đã bật Power Saver (Khóa trần 75% CPU)".to_string();
                }

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("⚖️ Cân bằng").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(p.primary_btn)
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    let _ = set_power_scheme("SCHEME_BALANCED");
                    let _ = set_cpu_max_state_dc(80);
                    self.power_status_msg = "✓ Đã bật Balanced (80% CPU pin, 100% sạc)".to_string();
                }

                if ui
                    .add(
                        egui::Button::new(
                            RichText::new("🚀 Tối đa").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(147, 51, 234))
                        .corner_radius(CornerRadius::same(5)),
                    )
                    .clicked()
                {
                    let _ = set_power_scheme("SCHEME_MIN");
                    let _ = set_cpu_max_state_dc(100);
                    let _ = set_cpu_max_state_ac(100);
                    self.power_status_msg = "✓ Đã mở tối đa 100% CPU Turbo Boost".to_string();
                }
            });

            if !self.power_status_msg.is_empty() {
                ui.add_space(4.0);
                ui.label(
                    RichText::new(&self.power_status_msg)
                        .size(12.0)
                        .color(p.accent_blue)
                        .strong(),
                );
            }
        });

        ui.add_space(8.0);

        // Hardware Controls Card
        p.modern_card(ui, |ui| {
            ui.label(RichText::new("TÙY CHỈNH THỦ CÔNG").size(12.0).strong().color(p.accent_blue));
            ui.separator();
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Giới hạn CPU khi dùng pin:").size(12.0).strong().color(p.text_primary));
                if ui.add(egui::Slider::new(&mut self.cpu_limit_dc, 50..=100).suffix("%")).changed() {
                    let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
                }
            });
            ui.label(
                RichText::new("💡 Mức 80% tắt Turbo 25W, giữ máy 45°C và tăng ~35% pin.")
                    .size(11.0)
                    .color(p.text_secondary),
            );

            ui.add_space(6.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Độ sáng màn hình:").size(12.0).strong().color(p.text_primary));
                if ui.add(egui::Slider::new(&mut self.brightness_level, 10..=100).suffix("%")).changed() {
                    let _ = set_brightness(self.brightness_level);
                }
            });
            ui.label(
                RichText::new("💡 Màn hình ngốn điện nhất. Mức 40% - 60% là tối ưu nhất.")
                    .size(11.0)
                    .color(p.text_secondary),
            );
        });

        ui.add_space(8.0);
        ui.label(RichText::new("CẢM BIẾN PIN PHẦN CỨNG (ACPI)").size(12.0).strong().color(p.accent_blue));
        ui.add_space(4.0);

        for (i, bat) in self.dual_batteries.iter().enumerate() {
            p.modern_card(ui, |ui| {
                ui.horizontal(|ui| {
                    let pack_name = if self.dual_batteries.len() > 1 {
                        if i == 0 { "Pack 1 (Trong)" } else { "Pack 2 (Ngoài)" }
                    } else {
                        "Pack chính"
                    };
                    ui.label(RichText::new(format!("{} - {}", pack_name, bat.id)).size(13.0).strong().color(p.text_primary));

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let wear_color = if bat.wear_percent > 30.0 { p.danger_red } else { p.success_green };
                        ui.label(
                            RichText::new(format!("Chai: {:.1}%", bat.wear_percent))
                                .size(12.5)
                                .strong()
                                .color(wear_color),
                        );
                    });
                });

                ui.separator();

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Sạc: {}%", bat.current_charge_percent)).size(11.5).color(p.text_primary));
                    ui.separator();
                    ui.label(RichText::new(format!("Chu kỳ: {} lần", bat.cycle_count)).size(11.5).color(p.text_primary));
                    ui.separator();
                    ui.label(
                        RichText::new(format!("Dung lượng: {} / {} mWh", bat.full_charge_capacity_mwh, bat.design_capacity_mwh))
                            .size(11.5)
                            .color(p.text_primary),
                    );
                });

                ui.add_space(2.0);
                ui.horizontal(|ui| {
                    let volt = if bat.voltage_mv > 0 {
                        format!("{:.2} V", bat.voltage_mv as f32 / 1000.0)
                    } else {
                        "N/A".to_string()
                    };
                    ui.label(RichText::new(format!("Điện áp: {}", volt)).size(11.5).color(p.text_primary));
                    ui.separator();
                    if bat.discharge_rate_mw > 0 {
                        ui.label(
                            RichText::new(format!("Dòng xả: {} mW ({:.2} W)", bat.discharge_rate_mw, bat.discharge_rate_mw as f32 / 1000.0))
                                .size(11.5)
                                .strong()
                                .color(p.warn_amber),
                        );
                    } else if bat.charge_rate_mw > 0 {
                        ui.label(
                            RichText::new(format!("Dòng nạp: {} mW ({:.2} W)", bat.charge_rate_mw, bat.charge_rate_mw as f32 / 1000.0))
                                .size(11.5)
                                .strong()
                                .color(p.success_green),
                        );
                    } else {
                        ui.label(RichText::new("Dòng điện: 0 mW (Cắm nguồn AC)").size(11.5).color(p.text_secondary));
                    }
                });
            });
            ui.add_space(4.0);
        }
    }

    fn render_protection(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🚀 Khởi động & Bảo vệ máy").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Quản lý ứng dụng khởi động, chặn Telemetry ngầm của Windows, bảo vệ SSD và Pin.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        // Windows Telemetry Debloat Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("🛡️ WINDOWS TELEMETRY & DIAGTRACK (CHỐNG THEO DÕI NỀN)").size(13.0).strong().color(p.accent_blue));
                    ui.label(
                        RichText::new("Dịch vụ DiagTrack âm thầm thu thập nhật ký và gửi về Microsoft, tiêu tốn CPU và I/O ổ đĩa nền.")
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if self.is_telemetry_running {
                        let btn = egui::Button::new(
                            RichText::new("Tắt DiagTrack (Admin)").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(p.danger_red)
                        .corner_radius(CornerRadius::same(5));

                        if ui.add(btn).clicked() {
                            let _ = toggle_telemetry(true);
                            self.is_telemetry_running = false;
                            self.telemetry_status_msg = "✓ Đã gửi lệnh vô hiệu hóa DiagTrack.".to_string();
                        }
                    } else {
                        let btn = egui::Button::new(
                            RichText::new("Bật lại DiagTrack").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(Color32::from_rgb(16, 120, 60))
                        .corner_radius(CornerRadius::same(5));

                        if ui.add(btn).clicked() {
                            let _ = toggle_telemetry(false);
                            self.is_telemetry_running = true;
                            self.telemetry_status_msg = "✓ Đã bật lại DiagTrack.".to_string();
                        }
                    }
                });
            });

            if !self.telemetry_status_msg.is_empty() {
                ui.add_space(4.0);
                ui.label(RichText::new(&self.telemetry_status_msg).size(12.0).color(p.success_green));
            }
        });

        ui.add_space(8.0);

        // Startup Applications List Card with Direct Toggle
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("ỨNG DỤNG KHỞI ĐỘNG CÙNG WINDOWS").size(12.5).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.button(RichText::new("🔄 Làm mới").size(11.5)).clicked() {
                        self.startup_apps = get_startup_apps();
                    }
                });
            });

            if !self.startup_action_msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&self.startup_action_msg).size(12.0).strong().color(p.success_green));
            }

            ui.separator();
            ui.add_space(2.0);

            egui::Grid::new("startup_grid")
                .striped(true)
                .min_col_width(60.0)
                .spacing(Vec2::new(10.0, 5.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("Tên ứng dụng").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Trạng thái").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Hành động").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Vị trí").size(11.5).strong().color(p.text_secondary));
                    ui.end_row();

                    for app in &mut self.startup_apps {
                        ui.label(RichText::new(&app.name).size(11.5).strong().color(p.text_primary));

                        let (status_str, status_color) = if app.is_enabled {
                            ("✓ Đang bật", p.success_green)
                        } else {
                            ("✗ Đã tắt", p.text_secondary)
                        };
                        ui.label(RichText::new(status_str).size(11.0).strong().color(status_color));

                        let toggle_btn = if app.is_enabled {
                            egui::Button::new(RichText::new("Tắt").size(10.5).color(Color32::WHITE))
                                .fill(p.danger_red)
                                .corner_radius(CornerRadius::same(4))
                        } else {
                            egui::Button::new(RichText::new("Bật").size(10.5).color(Color32::WHITE))
                                .fill(p.primary_btn)
                                .corner_radius(CornerRadius::same(4))
                        };

                        if ui.add(toggle_btn).clicked() {
                            let new_state = !app.is_enabled;
                            if toggle_startup_item(&app.name, &app.location, new_state) {
                                app.is_enabled = new_state;
                                self.startup_action_msg = format!(
                                    "✓ Đã {} khởi động cho '{}'",
                                    if new_state { "bật" } else { "tắt" },
                                    app.name
                                );
                            }
                        }

                        let short_loc = if app.location.contains("Startup") {
                            "Startup Folder".to_string()
                        } else if app.location.contains("HKLM") {
                            "Registry HKLM".to_string()
                        } else {
                            "Registry HKCU".to_string()
                        };
                        ui.label(RichText::new(short_loc).size(11.0).color(p.accent_blue));

                        ui.end_row();
                    }
                });
        });

        ui.add_space(8.0);

        // S.M.A.R.T NVMe SSD Storage Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new("SỨC KHỎE Ổ CỨNG SSD NVME (S.M.A.R.T)").size(12.5).strong().color(p.accent_blue));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let health_color = if self.protection.physical_disk_health == "Healthy" {
                        p.success_green
                    } else {
                        p.warn_amber
                    };
                    ui.label(
                        RichText::new(format!("Tình trạng: {} ({})", self.protection.physical_disk_health, self.protection.physical_disk_status))
                            .size(12.0)
                            .strong()
                            .color(health_color),
                    );
                });
            });
            ui.separator();
            ui.add_space(2.0);

            ui.horizontal(|ui| {
                ui.label(RichText::new("Tên phần cứng:").size(11.5).strong().color(p.text_secondary));
                ui.label(RichText::new(&self.protection.physical_disk_name).size(11.5).strong().color(p.text_primary));
            });

            for disk in &self.protection.disks {
                let used = disk.total_space.saturating_sub(disk.available_space);
                let pct = if disk.total_space > 0 {
                    (used as f32 / disk.total_space as f32) * 100.0
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Phân vùng {} ({})", disk.mount_point, disk.name)).size(11.5).strong().color(p.text_primary));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        ui.label(
                            RichText::new(format!(
                                "Trống: {} / {}",
                                format_bytes(disk.available_space),
                                format_bytes(disk.total_space)
                            ))
                            .size(11.5)
                            .color(p.text_secondary),
                        );
                    });
                });

                ui.add_space(2.0);
                ui.add(ProgressBar::new(pct / 100.0).desired_height(12.0).text(format!("{:.1}% đã dùng", pct)));
                ui.add_space(4.0);
            }
        });

        ui.add_space(8.0);

        // Lenovo Battery Charge Threshold Card (Interactive)
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("NGƯỠNG SẠC PIN 80% (BATTERY THRESHOLD)").size(13.0).strong().color(p.warn_amber));
                    ui.label(
                        RichText::new("Ngắt sạc khi pin đạt 80% để loại bỏ hiện tượng nhồi điện 4.2V liên tục, chống phù và kéo dài tuổi thọ.")
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let btn_text = if self.protection.battery_threshold_enabled {
                        "Tắt ngưỡng sạc"
                    } else {
                        "Bật ngưỡng 80% (Admin)"
                    };
                    let btn_color = if self.protection.battery_threshold_enabled {
                        p.danger_red
                    } else {
                        Color32::from_rgb(16, 120, 60)
                    };

                    let btn = egui::Button::new(RichText::new(btn_text).size(11.5).strong().color(Color32::WHITE))
                        .fill(btn_color)
                        .corner_radius(CornerRadius::same(5));

                    if ui.add(btn).clicked() {
                        let new_state = !self.protection.battery_threshold_enabled;
                        let _ = set_battery_threshold(new_state, 80);
                        self.protection.battery_threshold_enabled = new_state;
                        self.battery_thresh_msg = format!(
                            "✓ Đã gửi lệnh {} ngưỡng sạc 80% bảo vệ pin.",
                            if new_state { "bật" } else { "tắt" }
                        );
                    }
                });
            });

            if !self.battery_thresh_msg.is_empty() {
                ui.add_space(2.0);
                ui.label(RichText::new(&self.battery_thresh_msg).size(11.5).color(p.success_green));
            }
        });

        ui.add_space(8.0);

        // Hibernate Protection Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Chế độ ngủ đông (Hibernate)").size(13.0).strong().color(p.text_primary));
                    ui.label(
                        RichText::new("Khi gập máy, Hibernate ngắt điện 100%, chống sụt pin.")
                            .size(11.5)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if !self.protection.is_hibernate_enabled {
                        let btn = egui::Button::new(
                            RichText::new("Bật (Admin)").size(12.0).strong().color(Color32::WHITE),
                        )
                        .fill(p.primary_btn)
                        .corner_radius(CornerRadius::same(5));

                        if ui.add(btn).clicked() {
                            let _ = enable_hibernate_as_admin();
                        }
                    } else {
                        ui.label(RichText::new("✓ ĐÃ BẬT").size(12.5).strong().color(p.success_green));
                    }
                });
            });
        });
    }
}
