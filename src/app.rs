use crate::autonomous_engine::AutonomousEngine;
use crate::battery_info::get_dual_battery_info;
use crate::cleaner::{
    clean_target, format_bytes, get_default_targets, scan_target,
};
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process};
use crate::memory::{
    get_memory_stats, get_top_processes, terminate_process, trim_single_process, trim_working_sets,
    MemStats,
};
use crate::power::{
    get_brightness, get_cooling_policy, get_cpu_max_state_ac, get_cpu_max_state_dc, get_gamedvr,
    get_power_status, set_brightness, set_cooling_policy, set_cpu_max_state_ac,
    set_cpu_max_state_dc, set_gamedvr, set_keep_awake, set_power_scheme, set_timer_resolution_1ms,
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
use std::time::{Duration, Instant};
use sysinfo::System;

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

    // Advanced criteria tuning parameters
    cpu_limit_dc: u32,
    cpu_limit_ac: u32,
    timer_1ms_enabled: bool,
    keep_awake_mode: u8,
    cooling_active: bool,
    gamedvr_enabled: bool,

    // Power settings
    brightness_level: u32,

    // Dynamic hardware info
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

        let cpu_cores_info = format!(
            "{}C/{}T",
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
            current_tab: Tab::Widget,
            autonomous: AutonomousEngine::new(),
            foreground_info,
            theme_mode,
            palette,
            sys,
            mem_stats,
            power_status,
            dual_batteries,
            top_processes,
            protection,
            clean_categories,
            clean_status_msg: "Sẵn sàng quét rác.".to_string(),
            process_search_query: String::new(),
            process_action_msg: String::new(),
            startup_apps,
            is_telemetry_running,
            startup_action_msg: String::new(),
            cpu_limit_dc,
            cpu_limit_ac,
            timer_1ms_enabled: false,
            keep_awake_mode: 0,
            cooling_active,
            gamedvr_enabled,
            brightness_level,
            cpu_cores_info,
            quick_opt_msg: String::new(),
            last_refresh: Instant::now(),
            last_battery_poll: Instant::now(),
        }
    }

    fn calculate_health_score(&self) -> (u32, &'static str, Color32, Vec<String>) {
        let mut score = 100i32;
        let mut tips = Vec::new();

        if self.mem_stats.load_percent >= 85 {
            score -= 20;
            tips.push(format!("RAM cao ({}%), bấm 'Tối ưu' để giải phóng.", self.mem_stats.load_percent));
        } else if self.mem_stats.load_percent >= 70 {
            score -= 10;
        }

        let total_junk: u64 = self.clean_categories.iter().map(|c| c.size_bytes).sum();
        if total_junk > 2 * 1024 * 1024 * 1024 {
            score -= 20;
            tips.push(format!("Phát hiện {} rác/cache trên SSD, hãy dọn dẹp.", format_bytes(total_junk)));
        } else if total_junk > 500 * 1024 * 1024 {
            score -= 10;
            tips.push(format!("Tìm thấy {} file rác có thể dọn an toàn.", format_bytes(total_junk)));
        }

        if self.is_telemetry_running {
            score -= 10;
            tips.push("Dịch vụ DiagTrack đang chạy, khuyến nghị tắt để giảm I/O đĩa.".to_string());
        }

        if !self.protection.is_hibernate_enabled {
            score -= 10;
            tips.push("Chưa bật Hibernate, máy có thể bị hao pin khi ngủ Sleep S3.".to_string());
        }

        if self.power_status.is_ac_online && !self.protection.battery_threshold_enabled {
            tips.push("Cắm sạc liên tục: Khuyến nghị bật ngưỡng 80% để chống phù pin.".to_string());
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

    fn refresh_periodic(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_secs(1) {
            self.sys.refresh_cpu_usage();
            self.mem_stats = get_memory_stats();
            self.power_status = get_power_status();
            self.protection.cpu_usage = self.sys.global_cpu_usage();

            // Run autonomous engine tick
            self.autonomous.tick(
                &mut self.sys,
                self.power_status.is_ac_online,
                self.power_status.battery_percent,
                self.mem_stats.load_percent,
            );

            self.foreground_info = get_foreground_process(&self.sys);

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
                power_watts_calc(&mut self.power_status, total_discharge, total_charge, avg_voltage);
                self.last_battery_poll = Instant::now();
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

        let _ = set_cpu_max_state_dc(80);
        if !self.power_status.is_ac_online {
            let _ = set_power_scheme("SCHEME_MAX");
        }

        let (_, eco_count) = apply_foreground_boost_and_eco(
            &self.sys,
            self.foreground_info.as_ref().map(|(pid, _)| *pid),
        );

        self.mem_stats = get_memory_stats();
        self.top_processes = get_top_processes(&mut self.sys, 25);

        let msg = format!(
            "Dọn {} rác ({} files), thu hồi {} RAM từ {} app, ép EcoQoS {} app nền.",
            format_bytes(freed_junk),
            deleted_files,
            format_bytes(freed_ram),
            count,
            eco_count
        );
        self.quick_opt_msg = format!("✓ {}", msg);
        self.autonomous.log("⚡", format!("Tối ưu tức thì: {}", msg));
    }
}

fn power_watts_calc(status: &mut PowerStatus, discharge: u32, charge: u32, avg_v: u32) {
    status.power_watts = if discharge > 0 {
        discharge as f32 / 1000.0
    } else if charge > 0 {
        charge as f32 / 1000.0
    } else {
        0.0
    };
    status.voltage_volts = avg_v as f32 / 1000.0;
}

impl eframe::App for OptimizerApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.refresh_periodic();
        ui.ctx().request_repaint_after(Duration::from_millis(1000));

        let p = self.palette;

        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg_main).inner_margin(egui::Margin::symmetric(14, 12)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.current_tab {
                            Tab::Widget => self.render_widget_view(ui),
                            Tab::AdvancedSettings => self.render_advanced_view(ui),
                        }
                        ui.add_space(24.0);
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

            // Auto-Pilot live pulse badge
            let (ap_text, ap_color, ap_bg) = if self.autonomous.auto_pilot_enabled {
                ("● Auto-Pilot", p.success_green, if p.is_dark { Color32::from_rgb(18, 48, 28) } else { Color32::from_rgb(220, 252, 231) })
            } else {
                ("○ Thủ công", p.warn_amber, if p.is_dark { Color32::from_rgb(52, 38, 18) } else { Color32::from_rgb(254, 243, 199) })
            };

            egui::Frame::new()
                .fill(ap_bg)
                .corner_radius(CornerRadius::same(5))
                .inner_margin(egui::Margin::symmetric(8, 3))
                .show(ui, |ui| {
                    ui.label(RichText::new(ap_text).size(11.0).strong().color(ap_color));
                });

            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                // Settings button
                let gear_btn = egui::Button::new(RichText::new("⚙️ Tinh chỉnh").size(11.5).strong())
                    .fill(p.card_bg)
                    .stroke(Stroke::new(1.0, p.card_border))
                    .corner_radius(CornerRadius::same(5));

                if ui.add(gear_btn).on_hover_text("Mở Bảng điều khiển Chuyên sâu & Ma trận Setup").clicked() {
                    self.current_tab = Tab::AdvancedSettings;
                }

                // Theme toggle
                let (theme_icon, theme_hover) = if self.theme_mode == ThemeMode::Dark {
                    ("☀️", "Giao diện Sáng")
                } else {
                    ("🌙", "Giao diện Tối")
                };

                let theme_btn = egui::Button::new(RichText::new(theme_icon).size(12.0))
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

        // --- HEALTH & CONTEXT BANNER CARD ---
        let (score, score_label, score_color, tips) = self.calculate_health_score();
        p.modern_card(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.label(RichText::new("CHỈ SỐ SỨC KHỎE").size(11.0).strong().color(p.text_secondary));
                ui.horizontal(|ui| {
                    ui.with_layout(egui::Layout::centered_and_justified(egui::Direction::LeftToRight), |ui| {
                        ui.heading(
                            RichText::new(format!("{}/100", score))
                                .size(34.0)
                                .strong()
                                .color(score_color),
                        );
                    });
                });
                ui.label(RichText::new(score_label).size(12.5).strong().color(score_color));

                ui.add_space(4.0);

                // Smart Detected Context Banner
                let scen = self.autonomous.current_scenario;
                let (context_icon, context_name) = match scen {
                    Scenario::RemoteDesktop => ("🖥️", "Đang Remote Desktop (Tối ưu độ trễ chuột)"),
                    Scenario::Gaming => ("🎮", "Đang Chơi Game (Timer 1.0ms • Cấp đỉnh GPU)"),
                    Scenario::CafeMobility => ("☕", "Di động / Cà phê (Siêu tiết kiệm pin ~4W)"),
                    Scenario::Developer => ("💻", "Biên dịch Lập trình (100% Turbo Boost)"),
                    Scenario::MediaChill => ("🎬", "Xem phim (Màn hình luôn sáng • Quạt 0dB)"),
                    Scenario::Default => ("⚖️", "Chế độ Cân bằng tiêu chuẩn"),
                };

                egui::Frame::new()
                    .fill(if p.is_dark { Color32::from_rgb(26, 36, 60) } else { Color32::from_rgb(226, 234, 246) })
                    .corner_radius(CornerRadius::same(6))
                    .inner_margin(egui::Margin::symmetric(10, 5))
                    .show(ui, |ui| {
                        ui.label(
                            RichText::new(format!("{} Ngữ cảnh: {}", context_icon, context_name))
                                .size(11.5)
                                .strong()
                                .color(p.accent_blue),
                        );
                    });

                ui.add_space(6.0);

                // Smart Tip based on condition
                if let Some(tip) = tips.first() {
                    ui.label(RichText::new(format!("💡 {}", tip)).size(11.0).color(p.text_secondary));
                }

                ui.add_space(6.0);

                // Single Hero Action Button
                let opt_btn = egui::Button::new(
                    RichText::new("⚡ TỐI ƯU HÓA TỨC THÌ (1-CLICK)")
                        .size(13.5)
                        .strong()
                        .color(Color32::WHITE),
                )
                .fill(p.primary_btn)
                .min_size(Vec2::new(ui.available_width() - 10.0, 36.0))
                .corner_radius(CornerRadius::same(6));

                if ui.add(opt_btn).clicked() {
                    self.run_quick_optimize();
                }

                if !self.quick_opt_msg.is_empty() {
                    ui.add_space(4.0);
                    ui.label(RichText::new(&self.quick_opt_msg).size(11.0).color(p.success_green).strong());
                }
            });
        });

        ui.add_space(6.0);

        // --- 3 MINIMAL VITALS CHIPS (CPU, RAM, PIN) ---
        ui.columns(3, |cols| {
            // CPU Chip
            p.modern_card(&mut cols[0], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("CPU").size(11.0).strong().color(p.text_secondary));
                    let cpu = self.protection.cpu_usage;
                    let cpu_color = if cpu > 80.0 { p.danger_red } else { p.accent_blue };
                    ui.heading(RichText::new(format!("{:.1}%", cpu)).size(22.0).strong().color(cpu_color));
                    ui.label(RichText::new("i7-7600U").size(11.0).strong().color(p.text_primary));
                    ui.label(RichText::new(&self.cpu_cores_info).size(10.0).color(p.text_secondary));
                });
            });

            // RAM Chip
            p.modern_card(&mut cols[1], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("RAM").size(11.0).strong().color(p.text_secondary));
                    let load = self.mem_stats.load_percent;
                    let ram_color = if load > 85 { p.danger_red } else { p.success_green };
                    ui.heading(RichText::new(format!("{}%", load)).size(22.0).strong().color(ram_color));
                    ui.label(
                        RichText::new(format_bytes(self.mem_stats.used_bytes))
                            .size(11.0)
                            .strong()
                            .color(p.text_primary),
                    );
                    ui.label(
                        RichText::new(format!("Trống {}", format_bytes(self.mem_stats.free_bytes)))
                            .size(10.0)
                            .color(p.text_secondary),
                    );
                });
            });

            // Battery Chip
            p.modern_card(&mut cols[2], |ui| {
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("PIN").size(11.0).strong().color(p.text_secondary));
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
                    ui.label(RichText::new(pwr_str).size(11.0).strong().color(p.text_primary));
                    let volt_str = if self.power_status.voltage_volts > 0.0 {
                        format!("{:.1} V", self.power_status.voltage_volts)
                    } else {
                        "Dual Pack".to_string()
                    };
                    ui.label(RichText::new(volt_str).size(10.0).color(p.text_secondary));
                });
            });
        });

        ui.add_space(6.0);

        // --- FOREGROUND GOVERNOR CARD ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Cửa sổ Focus:").size(11.5).strong().color(p.text_secondary));
                        if let Some((_, ref name)) = self.foreground_info {
                            ui.label(RichText::new(name).size(12.0).strong().color(p.accent_blue));
                        } else {
                            ui.label(RichText::new("Đang xác định").size(11.5).color(p.text_secondary));
                        }
                    });
                    ui.label(
                        RichText::new("Tự động cấp luồng ưu tiên, hạ EcoQoS app ngầm.")
                            .size(10.5)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.checkbox(
                        &mut self.autonomous.eco_qos_enabled,
                        RichText::new("EcoQoS").size(11.0).strong().color(p.accent_blue),
                    );
                });
            });
        });

        ui.add_space(6.0);

        // --- AUTONOMOUS ACTIVITY STREAM (NHẬT KÝ TỰ HÀNH) ---
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new("🤖 NHẬT KÝ TỰ ĐỘNG CHẠY NGẦM (LIVE STREAM)")
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
                for ev in self.autonomous.activity_log.iter().take(4) {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(&ev.time).size(10.5).color(p.text_secondary));
                        ui.label(RichText::new(ev.icon).size(11.0));
                        ui.label(RichText::new(&ev.message).size(11.0).color(p.text_primary));
                    });
                    ui.add_space(1.0);
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
                RichText::new("⬅️ Quay lại Widget")
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
                RichText::new("⚙️ Tinh Chỉnh Chuyên Sâu")
                    .size(15.0)
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

        // --- PHẦN 1: BẢNG MA TRẬN SETUP ---
        ui.label(RichText::new("1. MA TRẬN TINH CHỈNH PHẦN CỨNG").size(12.5).strong().color(p.accent_blue));
        ui.add_space(2.0);
        self.render_criteria_table(ui);

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
    }

    fn render_criteria_table(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;
        let bg_header = if p.is_dark { Color32::from_rgb(26, 36, 60) } else { Color32::from_rgb(226, 234, 246) };
        let bg_even = if p.is_dark { Color32::from_rgb(18, 25, 42) } else { Color32::from_rgb(255, 255, 255) };
        let bg_odd = if p.is_dark { Color32::from_rgb(23, 31, 52) } else { Color32::from_rgb(246, 249, 253) };

        egui::Frame::new()
            .fill(p.card_bg)
            .corner_radius(CornerRadius::same(8))
            .stroke(Stroke::new(1.0, p.card_border))
            .inner_margin(egui::Margin::ZERO)
            .show(ui, |ui| {
                // Header
                egui::Frame::new().fill(bg_header).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("📌 TIÊU CHÍ").size(12.0).strong().color(p.accent_blue));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.label(RichText::new("🛠️ TINH CHỈNH THÔNG SỐ & KIẾN NGHỊ").size(12.0).strong().color(p.accent_blue));
                        });
                    });
                });

                ui.separator();

                // Row 1: CPU
                egui::Frame::new().fill(bg_even).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("1. ⚡ Xung CPU").size(12.5).strong().color(p.text_primary));
                            ui.label(RichText::new("i7-7600U, Turbo 25W.").size(10.5).color(p.text_secondary));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Pin (DC):").size(11.0).strong().color(p.text_primary));
                                if ui.add(egui::Slider::new(&mut self.cpu_limit_dc, 50..=100).suffix("%")).changed() {
                                    let _ = set_cpu_max_state_dc(self.cpu_limit_dc);
                                }
                                if ui.button(RichText::new("60%").size(10.0)).clicked() {
                                    self.cpu_limit_dc = 60;
                                    let _ = set_cpu_max_state_dc(60);
                                }
                                if ui.button(RichText::new("80%").size(10.0)).clicked() {
                                    self.cpu_limit_dc = 80;
                                    let _ = set_cpu_max_state_dc(80);
                                }
                            });
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Sạc (AC):").size(11.0).strong().color(p.text_primary));
                                if ui.add(egui::Slider::new(&mut self.cpu_limit_ac, 50..=100).suffix("%")).changed() {
                                    let _ = set_cpu_max_state_ac(self.cpu_limit_ac);
                                }
                            });
                        });
                    });
                });

                ui.separator();

                // Row 2: Timer
                egui::Frame::new().fill(bg_odd).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("2. ⏱️ Timer & FPS").size(12.5).strong().color(p.text_primary));
                            ui.label(RichText::new("15.6ms vs 1.0ms.").size(10.5).color(p.text_secondary));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                let btn_1ms = egui::Button::new(RichText::new("🚀 1.0 ms (Game & Remote)").size(11.0).color(Color32::WHITE))
                                    .fill(if self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg });
                                if ui.add(btn_1ms).clicked() {
                                    self.timer_1ms_enabled = true;
                                    set_timer_resolution_1ms(true);
                                }
                                let btn_15ms = egui::Button::new(RichText::new("🍃 15.6 ms (Mặc định)").size(11.0).color(if !self.timer_1ms_enabled { Color32::WHITE } else { p.text_primary }))
                                    .fill(if !self.timer_1ms_enabled { p.primary_btn } else { p.tab_inactive_bg });
                                if ui.add(btn_15ms).clicked() {
                                    self.timer_1ms_enabled = false;
                                    set_timer_resolution_1ms(false);
                                }
                            });
                        });
                    });
                });

                ui.separator();

                // Row 3: Brightness
                egui::Frame::new().fill(bg_even).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("3. 🔆 Độ sáng").size(12.5).strong().color(p.text_primary));
                            ui.label(RichText::new("LED (30-50% pin).").size(10.5).color(p.text_secondary));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
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
                        });
                    });
                });

                ui.separator();

                // Row 4: Fan Cooling & Sleep Lock
                egui::Frame::new().fill(bg_odd).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("4. ❄️ Quạt & Chống Sleep").size(12.5).strong().color(p.text_primary));
                            ui.label(RichText::new("Tản nhiệt & Giữ máy.").size(10.5).color(p.text_secondary));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                let btn_p = egui::Button::new(RichText::new("🍃 Quạt 0dB").size(10.5).color(if !self.cooling_active { Color32::WHITE } else { p.text_primary }))
                                    .fill(if !self.cooling_active { Color32::from_rgb(16, 120, 60) } else { p.tab_inactive_bg });
                                if ui.add(btn_p).clicked() {
                                    self.cooling_active = false;
                                    let _ = set_cooling_policy(false);
                                }
                                let btn_a = egui::Button::new(RichText::new("🚀 Quạt Active").size(10.5).color(if self.cooling_active { Color32::WHITE } else { p.text_primary }))
                                    .fill(if self.cooling_active { p.primary_btn } else { p.tab_inactive_bg });
                                if ui.add(btn_a).clicked() {
                                    self.cooling_active = true;
                                    let _ = set_cooling_policy(true);
                                }

                                if ui.selectable_value(&mut self.keep_awake_mode, 0, "Sleep OS").clicked() {
                                    set_keep_awake(0);
                                }
                                if ui.selectable_value(&mut self.keep_awake_mode, 2, "Chống Sleep").clicked() {
                                    set_keep_awake(2);
                                }
                            });
                        });
                    });
                });

                ui.separator();

                // Row 5: GameDVR & Threshold
                egui::Frame::new().fill(bg_even).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.allocate_ui_with_layout(Vec2::new(135.0, 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.set_width(135.0);
                            ui.label(RichText::new("5. 🛡️ Bảo vệ máy").size(12.5).strong().color(p.text_primary));
                            ui.label(RichText::new("GameDVR & Ngưỡng pin.").size(10.5).color(p.text_secondary));
                        });
                        ui.separator();
                        ui.allocate_ui_with_layout(Vec2::new(ui.available_width(), 0.0), egui::Layout::top_down(egui::Align::Min), |ui| {
                            ui.horizontal(|ui| {
                                let gdvr_text = if self.gamedvr_enabled { "GameDVR: Bật" } else { "GameDVR: Tắt" };
                                if ui.button(RichText::new(gdvr_text).size(10.5)).clicked() {
                                    self.gamedvr_enabled = !self.gamedvr_enabled;
                                    let _ = set_gamedvr(self.gamedvr_enabled);
                                }

                                let thresh_text = if self.protection.battery_threshold_enabled { "Ngưỡng 80%: Bật" } else { "Ngưỡng 80%: Tắt" };
                                if ui.button(RichText::new(thresh_text).size(10.5)).clicked() {
                                    let st = !self.protection.battery_threshold_enabled;
                                    let _ = set_battery_threshold(st, 80);
                                    self.protection.battery_threshold_enabled = st;
                                }

                                if !self.protection.is_hibernate_enabled {
                                    if ui.button(RichText::new("Bật Hibernate").size(10.5)).clicked() {
                                        let _ = enable_hibernate_as_admin();
                                        self.protection.is_hibernate_enabled = true;
                                    }
                                }
                            });
                        });
                    });
                });
            });
    }

    fn render_cleaner_section(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.horizontal(|ui| {
            let scan_btn = egui::Button::new(RichText::new("🔍 Quét rác").size(12.0).color(Color32::WHITE))
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

            let clean_safe_btn = egui::Button::new(RichText::new("🛡️ Dọn Rác Tầng 1").size(12.0).color(Color32::WHITE))
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

            let clean_dev_btn = egui::Button::new(RichText::new("💻 Dọn Dev Cache (NPM/Go)").size(12.0).color(Color32::WHITE))
                .fill(Color32::from_rgb(147, 51, 234));
            if ui.add(clean_dev_btn).clicked() {
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
                    ui.add(egui::TextEdit::singleline(&mut self.process_search_query).hint_text("🔍 Lọc app...").desired_width(120.0));
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
                .filter(|p| {
                    if query.is_empty() { true } else { p.name.to_lowercase().contains(&query) || p.pid.to_string().contains(&query) }
                })
                .take(10)
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
                        if ui.button(RichText::new("⚡ Dọn").size(10.0)).clicked() {
                            let f = trim_single_process(proc.pid);
                            self.process_action_msg = format!("✓ Thu hồi {} từ '{}'", format_bytes(f), proc.name);
                        }
                        if ui.add(egui::Button::new(RichText::new("❌").size(10.0).color(p.danger_red))).clicked() {
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
                    if ui.button(RichText::new("🔄").size(11.0)).clicked() {
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
}
