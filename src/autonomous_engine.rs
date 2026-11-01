use crate::cleaner::format_bytes;
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process};
use crate::memory::trim_working_sets;
use crate::power::{set_brightness, set_cooling_policy, set_cpu_max_state_dc, set_power_scheme};
use crate::scenarios::{apply_scenario, detect_recommended_scenario, Scenario};
use crate::types::ActivityEvent;
use sysinfo::System;
use std::time::{Duration, Instant};
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

pub fn get_current_time_str() -> String {
    unsafe {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        format!("{:02}:{:02}:{:02}", st.wHour, st.wMinute, st.wSecond)
    }
}

pub struct AutonomousEngine {
    pub activity_log: Vec<ActivityEvent>,
    pub current_scenario: Scenario,
    pub last_power_ac: Option<bool>,
    pub last_foreground_pid: Option<u32>,
    pub auto_pilot_enabled: bool,
    pub eco_qos_enabled: bool,
    pub auto_ram_enabled: bool,
    pub last_ram_trim: Instant,
}

impl AutonomousEngine {
    pub fn new() -> Self {
        let mut engine = Self {
            activity_log: Vec::new(),
            current_scenario: Scenario::Default,
            last_power_ac: None,
            last_foreground_pid: None,
            auto_pilot_enabled: true,
            eco_qos_enabled: true,
            auto_ram_enabled: true,
            last_ram_trim: Instant::now(),
        };

        engine.log("[START]", "Hệ thống tự hành OmniTune Auto-Pilot đã khởi động.".to_string());
        engine
    }

    pub fn log(&mut self, icon: &'static str, message: String) {
        let event = ActivityEvent {
            time: get_current_time_str(),
            icon,
            message,
        };
        self.activity_log.insert(0, event);
        if self.activity_log.len() > 30 {
            self.activity_log.truncate(30);
        }
    }

    pub fn tick(
        &mut self,
        sys: &mut System,
        is_ac: bool,
        battery_pct: u8,
        ram_load_pct: u32,
    ) {
        // 1. Cảm biến nguồn điện (AC/DC Transition Sensor)
        if let Some(prev_ac) = self.last_power_ac {
            if prev_ac != is_ac {
                if !is_ac {
                    // Rút sạc -> Tự động kích hoạt chế độ tiết kiệm pin
                    let _ = set_power_scheme("SCHEME_MAX");
                    let _ = set_cpu_max_state_dc(65);
                    let _ = set_cooling_policy(false); // Quạt tắt
                    let _ = set_brightness(45);
                    self.current_scenario = Scenario::CafeMobility;
                    self.log(
                        "[PIN]",
                        format!(
                            "Rút sạc (Pin {}%) -> Tự động tiết kiệm pin: CPU 65% (~4W), quạt 0dB, màn hình 45%.",
                            battery_pct
                        ),
                    );
                } else {
                    // Cắm sạc -> Khôi phục hiệu năng tối đa
                    let _ = set_power_scheme("SCHEME_BALANCED");
                    let _ = set_cooling_policy(true);
                    let _ = set_brightness(70);
                    self.current_scenario = Scenario::Default;
                    self.log(
                        "[NGUỒN]",
                        "Cắm sạc AC -> Tự động khôi phục xung nhịp CPU 100%, quạt tản nhiệt chủ động.".to_string(),
                    );
                }
            }
        }
        self.last_power_ac = Some(is_ac);

        // 2. Nhận diện cửa sổ tương tác (Foreground Watcher & EcoQoS)
        let fg = get_foreground_process(sys);
        let fg_pid = fg.as_ref().map(|(pid, _)| *pid);

        if fg_pid != self.last_foreground_pid {
            self.last_foreground_pid = fg_pid;

            // Áp dụng EcoQoS cho các app nền
            if self.eco_qos_enabled {
                let (boosted, eco_count) = apply_foreground_boost_and_eco(sys, fg_pid);
                if let Some((_, ref name)) = fg {
                    if eco_count > 0 {
                        self.log(
                            "[ECO]",
                            format!(
                                "Tập trung '{}' (Nâng ưu tiên, ép EcoQoS hạ tải {} app nền).",
                                name, eco_count
                            ),
                        );
                    } else if boosted > 0 {
                        self.log("[FOCUS]", format!("Tập trung '{}' (Đã cấp luồng ưu tiên cao).", name));
                    }
                }
            }

            // Tự động nhận diện kịch bản (Auto-Pilot Scenario Detection)
            if self.auto_pilot_enabled {
                if let Some(target_scen) = detect_recommended_scenario(sys, is_ac, battery_pct) {
                    if target_scen != self.current_scenario {
                        self.current_scenario = target_scen;
                        let msg = apply_scenario(target_scen, sys);
                        self.log("[CHUYỂN]", format!("Chuyển sang [{}]: {}", target_scen.name(), msg));
                    }
                }
            }
        }

        // 3. Tự động thu hồi RAM khi vượt ngưỡng 85% (Memory Watchdog có cooldown 60s)
        if self.auto_ram_enabled
            && ram_load_pct >= 85
            && self.last_ram_trim.elapsed() >= Duration::from_secs(60)
        {
            self.last_ram_trim = Instant::now();
            let (freed, count) = trim_working_sets(sys);
            if freed > 10 * 1024 * 1024 {
                self.log(
                    "[RAM]",
                    format!(
                        "RAM chạm {}% -> Tự động thu hồi {} bộ nhớ nhàn rỗi từ {} tiến trình.",
                        ram_load_pct,
                        format_bytes(freed),
                        count
                    ),
                );
            }
        }
    }
}
