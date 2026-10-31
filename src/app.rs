use crate::battery_info::get_dual_battery_info;
use crate::cleaner::{
    clean_target, format_bytes, get_default_targets, scan_target,
};
use crate::memory::{get_memory_stats, get_top_processes, trim_working_sets, MemStats};
use crate::power::{
    get_active_scheme_name, get_brightness, get_power_status, set_brightness, set_cpu_max_state_ac,
    set_cpu_max_state_dc, set_power_scheme, PowerStatus,
};
use crate::protection::{enable_hibernate_as_admin, get_protection_overview, ProtectionOverview};
use crate::scenarios::{apply_scenario, Scenario};
use crate::theme::{
    apply_theme, configure_fonts, ThemeMode, ThemePalette,
};
use crate::types::{BatteryDetails, JunkCategory, ProcessMemInfo, Tab};
use eframe::egui::{self, Color32, CornerRadius, ProgressBar, RichText, Stroke, Vec2};
use std::time::{Duration, Instant};
use sysinfo::System;

pub struct OptimizerApp {
    current_tab: Tab,
    current_scenario: Scenario,
    auto_detect_enabled: bool,
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

    // Power settings
    brightness_level: u32,
    cpu_limit_dc: u32,
    power_status_msg: String,

    // Scenario message
    scenario_status_msg: String,

    // Dynamic hardware info
    cpu_brand: String,
    cpu_cores_info: String,

    // General status
    quick_opt_msg: String,
    last_refresh: Instant,
}

impl OptimizerApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        configure_fonts(&cc.egui_ctx);
        let theme_mode = ThemeMode::Light;
        let palette = apply_theme(&cc.egui_ctx, theme_mode);

        let mut sys = System::new().expect("failed to initialize sysinfo");
        sys.refresh_all();

        let mem_stats = get_memory_stats();
        let power_status = get_power_status();
        let top_processes = get_top_processes(&mut sys, 10);
        let dual_batteries = get_dual_battery_info();
        let cpu_usage = sys.global_cpu_usage();
        let protection = get_protection_overview(cpu_usage);
        let brightness_level = get_brightness();

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
            })
            .collect();

        Self {
            current_tab: Tab::Scenarios,
            current_scenario: Scenario::Default,
            auto_detect_enabled: false,
            theme_mode,
            palette,
            sys,
            mem_stats,
            power_status,
            dual_batteries,
            top_processes,
            protection,
            clean_categories,
            clean_status_msg: "Sẵn sàng quét rác hệ thống.".to_string(),
            brightness_level,
            cpu_limit_dc: 80,
            power_status_msg: String::new(),
            scenario_status_msg: "Chọn một kịch bản bên dưới để tối ưu phần cứng theo nhu cầu.".to_string(),
            cpu_brand,
            cpu_cores_info,
            quick_opt_msg: String::new(),
            last_refresh: Instant::now(),
        }
    }

    fn refresh_periodic(&mut self) {
        if self.last_refresh.elapsed() >= Duration::from_secs(2) {
            self.sys.refresh_cpu_usage();
            self.mem_stats = get_memory_stats();
            self.power_status = get_power_status();
            self.protection.cpu_usage = self.sys.global_cpu_usage();

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

            self.last_refresh = Instant::now();
        }
    }

    fn run_quick_optimize(&mut self) {
        let (freed_ram, count) = trim_working_sets(&mut self.sys);
        let _ = set_cpu_max_state_dc(80);
        if !self.power_status.is_ac_online {
            let _ = set_power_scheme("SCHEME_MAX");
        }
        self.mem_stats = get_memory_stats();
        self.quick_opt_msg = format!(
            "✓ Đã tối ưu! Thu hồi {} RAM từ {} tiến trình & áp trần xung 80% cho pin.",
            format_bytes(freed_ram),
            count
        );
    }
}

fn render_tab_button(
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
        (palette.tab_inactive_bg, palette.tab_inactive_fg)
    };

    let btn = egui::Button::new(RichText::new(label).size(12.5).strong().color(fg))
        .fill(bg)
        .stroke(Stroke::new(1.0, palette.card_border))
        .corner_radius(CornerRadius::same(5));

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
                // Top Header Row
                ui.horizontal(|ui| {
                    ui.heading(
                        RichText::new("⚡ OmniTune")
                            .size(19.0)
                            .strong()
                            .color(p.accent_blue),
                    );
                    ui.label(
                        RichText::new("System & Hardware Optimizer")
                            .size(12.0)
                            .color(p.text_secondary),
                    );

                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Theme Toggle Button
                        let (theme_btn_text, theme_btn_bg) = if self.theme_mode == ThemeMode::Dark {
                            ("☀️ Sáng", Color32::from_rgb(234, 179, 8))
                        } else {
                            ("🌙 Tối", Color32::from_rgb(30, 41, 59))
                        };

                        let theme_btn = egui::Button::new(
                            RichText::new(theme_btn_text)
                                .size(11.5)
                                .strong()
                                .color(if self.theme_mode == ThemeMode::Dark { Color32::BLACK } else { Color32::WHITE }),
                        )
                        .fill(theme_btn_bg)
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
                                "🔌 AC",
                                if p.is_dark { Color32::from_rgb(20, 83, 45) } else { Color32::from_rgb(220, 252, 231) },
                                if p.is_dark { p.success_green } else { Color32::from_rgb(21, 128, 61) },
                            )
                        } else {
                            (
                                "🔋 Pin",
                                if p.is_dark { Color32::from_rgb(124, 45, 18) } else { Color32::from_rgb(254, 243, 199) },
                                if p.is_dark { p.warn_amber } else { Color32::from_rgb(180, 83, 9) },
                            )
                        };

                        egui::Frame::new()
                            .fill(badge_bg)
                            .corner_radius(CornerRadius::same(5))
                            .stroke(Stroke::new(1.0, badge_fg))
                            .inner_margin(egui::Margin::symmetric(8, 3))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!(
                                        "{} {}%",
                                        status_text, self.power_status.battery_percent
                                    ))
                                    .size(11.5)
                                    .strong()
                                    .color(badge_fg),
                                );
                            });
                    });
                });

                ui.add_space(4.0);

                // Tab Navigation Bar
                ui.horizontal(|ui| {
                    render_tab_button(ui, &mut self.current_tab, Tab::Scenarios, "🎯 Kịch bản", &p);
                    render_tab_button(ui, &mut self.current_tab, Tab::Dashboard, "📊 Tổng quan", &p);
                    render_tab_button(ui, &mut self.current_tab, Tab::Cleaner, "🧹 Dọn rác", &p);
                    render_tab_button(ui, &mut self.current_tab, Tab::Memory, "🧠 RAM", &p);
                    render_tab_button(ui, &mut self.current_tab, Tab::Power, "🔋 Pin & CPU", &p);
                    render_tab_button(ui, &mut self.current_tab, Tab::Protection, "🛡️ Bảo vệ", &p);
                });

                ui.add_space(6.0);

                // Main Content View
                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        match self.current_tab {
                            Tab::Scenarios => self.render_scenarios(ui),
                            Tab::Dashboard => self.render_dashboard(ui),
                            Tab::Cleaner => self.render_cleaner(ui),
                            Tab::Memory => self.render_memory(ui),
                            Tab::Power => self.render_power(ui),
                            Tab::Protection => self.render_protection(ui),
                        }
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
        ui.add_space(6.0);

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

    fn render_dashboard(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

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
                        "Đã cắm sạc đầy".to_string()
                    } else if let Some(sec) = self.power_status.remaining_seconds {
                        format!("Còn ~{}h {:02}m", sec / 3600, (sec % 3600) / 60)
                    } else {
                        "Đang dùng pin".to_string()
                    };
                    ui.label(RichText::new(status).size(12.0).strong().color(p.text_primary));
                    let pack_count_str = if self.dual_batteries.len() > 1 {
                        format!("{} pack pin", self.dual_batteries.len())
                    } else {
                        "1 pack pin".to_string()
                    };
                    ui.label(RichText::new(pack_count_str).size(11.5).color(p.text_secondary));
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

        // 1-Click Optimize Banner Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.heading(RichText::new("⚡ Tối ưu 1-Click").size(15.0).strong().color(p.text_primary));
                    ui.label(
                        RichText::new("Thu hồi Working Set RAM và khóa trần 80% CPU khi dùng pin.")
                            .size(12.0)
                            .color(p.text_secondary),
                    );
                });

                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let opt_btn = egui::Button::new(
                        RichText::new("⚡ TỐI ƯU NGAY").size(13.0).strong().color(Color32::WHITE),
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
                        .size(12.5)
                        .color(p.success_green)
                        .strong(),
                );
            }
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

        ui.heading(RichText::new("🧹 Dọn dẹp rác & Cache hệ thống").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Quét và dọn các file tạm, cache ứng dụng để giải phóng SSD.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        ui.horizontal(|ui| {
            let scan_btn = egui::Button::new(
                RichText::new("🔍 Quét rác").size(12.5).strong().color(Color32::WHITE),
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

            let clean_btn = egui::Button::new(
                RichText::new("🗑️ Dọn sạch").size(12.5).strong().color(Color32::WHITE),
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

        p.modern_card(ui, |ui| {
            for cat in &mut self.clean_categories {
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
        });
    }

    fn render_memory(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🧠 Tối ưu RAM").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Thu hồi Working Set nhàn rỗi để giảm áp lực bộ nhớ.")
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

            let trim_btn = egui::Button::new(
                RichText::new("⚡ Thu hồi RAM ngay").size(13.0).strong().color(Color32::WHITE),
            )
            .fill(p.primary_btn)
            .corner_radius(CornerRadius::same(6));

            if ui.add(trim_btn).clicked() {
                let (freed, count) = trim_working_sets(&mut self.sys);
                self.mem_stats = get_memory_stats();
                self.top_processes = get_top_processes(&mut self.sys, 10);
                self.quick_opt_msg = format!("✓ Đã thu hồi {} từ {} tiến trình!", format_bytes(freed), count);
            }

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
        ui.label(RichText::new("TOP ỨNG DỤNG CHIẾM RAM").size(12.0).strong().color(p.accent_blue));
        ui.add_space(4.0);

        p.modern_card(ui, |ui| {
            egui::Grid::new("proc_grid")
                .striped(true)
                .min_col_width(60.0)
                .spacing(Vec2::new(14.0, 5.0))
                .show(ui, |ui| {
                    ui.label(RichText::new("PID").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Ứng dụng").size(11.5).strong().color(p.text_secondary));
                    ui.label(RichText::new("Bộ nhớ").size(11.5).strong().color(p.text_secondary));
                    ui.end_row();

                    for proc in &self.top_processes {
                        ui.label(RichText::new(proc.pid.to_string()).size(11.5).color(p.text_secondary));
                        ui.label(RichText::new(&proc.name).size(11.5).strong().color(p.text_primary));
                        ui.label(RichText::new(format_bytes(proc.memory_bytes)).size(11.5).color(p.accent_blue).strong());
                        ui.end_row();
                    }
                });
        });
    }

    fn render_power(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🔋 Pin & Công suất CPU").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Điều khiển xung nhịp CPU, độ sáng và giám sát cảm biến pin.")
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
        ui.label(RichText::new("CẢM BIẾN PIN PHẦN CỨNG").size(12.0).strong().color(p.accent_blue));
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
            });
            ui.add_space(4.0);
        }
    }

    fn render_protection(&mut self, ui: &mut egui::Ui) {
        let p = self.palette;

        ui.heading(RichText::new("🛡️ Bảo vệ máy & Giám sát").size(16.0).strong().color(p.text_primary));
        ui.label(
            RichText::new("Bảo vệ ổ SSD NVMe, chế độ ngủ đông và tuổi thọ pin.")
                .size(12.0)
                .color(p.text_secondary),
        );
        ui.add_space(8.0);

        // Hibernate Protection Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.vertical(|ui| {
                    ui.label(RichText::new("Chế độ ngủ đông (Hibernate)").size(13.5).strong().color(p.text_primary));
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

        ui.add_space(8.0);

        // NVMe Disk Storage Card
        p.modern_card(ui, |ui| {
            ui.label(RichText::new("SỨC KHỎE Ổ CỨNG SSD").size(12.0).strong().color(p.accent_blue));
            ui.separator();
            ui.add_space(2.0);

            for disk in &self.protection.disks {
                let used = disk.total_space.saturating_sub(disk.available_space);
                let pct = if disk.total_space > 0 {
                    (used as f32 / disk.total_space as f32) * 100.0
                } else {
                    0.0
                };

                ui.horizontal(|ui| {
                    ui.label(RichText::new(format!("Ổ {} ({})", disk.mount_point, disk.name)).size(12.0).strong().color(p.text_primary));
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
                ui.add(ProgressBar::new(pct / 100.0).desired_height(12.0).text(format!("{:.1}%", pct)));
                ui.add_space(4.0);
            }
        });

        ui.add_space(8.0);

        // Lenovo Battery Threshold Card
        p.modern_card(ui, |ui| {
            ui.label(RichText::new("KHUYẾN NGHỊ BẢO VỆ PIN (NGƯỠNG SẠC 80%)").size(12.0).strong().color(p.warn_amber));
            ui.add_space(2.0);
            ui.label(
                RichText::new("Nếu cắm sạc thường xuyên tại bàn làm việc, hãy bật 'Battery Charge Threshold' (ngưỡng 75-80%) trong phần mềm quản lý pin để tránh sạc nhồi điện áp cao, chống phù pin và tăng tuổi thọ.")
                    .size(11.5)
                    .color(p.text_secondary),
            );
        });

        ui.add_space(8.0);

        // Startup apps Card
        p.modern_card(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label(
                    RichText::new(format!(
                        "Khởi động cùng Windows: {} ứng dụng",
                        self.protection.startup_apps_count
                    ))
                    .size(12.0)
                    .strong()
                    .color(p.text_primary),
                );
            });
            ui.label(
                RichText::new("💡 Tắt bớt ứng dụng đồng bộ đám mây và điều khiển từ xa để mở máy nhanh hơn.")
                    .size(11.5)
                    .color(p.text_secondary),
            );
        });
    }
}
