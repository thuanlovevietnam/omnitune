use crate::cleaner::format_bytes;
use crate::eco_qos::{apply_foreground_boost_and_eco, get_foreground_process};
use crate::hardware_profile::{get_user_idle_ms, HardwareProfile};
use crate::memory::trim_working_sets;
use crate::scenarios::{apply_scenario, detect_recommended_scenario, Scenario};
use crate::types::ActivityEvent;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};
use sysinfo::System;
use windows_sys::Win32::Foundation::SYSTEMTIME;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OperatingMode {
    /// Chế độ 1: Tối ưu nhất dựa trên cấu hình Hệ điều hành và Máy (Không hardcode)
    HardwareOptimal,
    /// Chế độ 2: Tự động tối ưu dựa trên chế độ làm việc (Workload Adaptive)
    WorkloadAdaptive,
}

impl Default for OperatingMode {
    fn default() -> Self {
        OperatingMode::WorkloadAdaptive
    }
}

#[allow(dead_code)]
impl OperatingMode {
    pub fn name(&self) -> &'static str {
        match self {
            OperatingMode::HardwareOptimal => "Tối ưu nhất (Phần cứng & OS)",
            OperatingMode::WorkloadAdaptive => "Tự động tối ưu (Theo công việc)",
        }
    }

    pub fn short_name(&self) -> &'static str {
        match self {
            OperatingMode::HardwareOptimal => "Tối ưu Phần Cứng",
            OperatingMode::WorkloadAdaptive => "Tự Động Công Việc",
        }
    }
}

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
    pub hardware_profile: HardwareProfile,
    pub operating_mode: OperatingMode,
    pub is_pro: bool,
}

impl AutonomousEngine {
    pub fn new(
        hw: HardwareProfile,
        mode: OperatingMode,
        saved_scenario: Scenario,
        auto_pilot_enabled: bool,
        eco_qos_enabled: bool,
        auto_ram_enabled: bool,
        is_pro: bool,
    ) -> Self {
        let mut engine = Self {
            activity_log: Vec::new(),
            current_scenario: saved_scenario,
            last_power_ac: None,
            last_foreground_pid: None,
            auto_pilot_enabled,
            eco_qos_enabled,
            auto_ram_enabled,
            last_ram_trim: Instant::now(),
            hardware_profile: hw,
            operating_mode: mode,
            is_pro,
        };

        engine.log(
            "[START]",
            format!(
                "Khởi động: Chế độ [{}] • Ngữ cảnh [{}] • OS {} • CPU {} nhân.",
                engine.operating_mode.short_name(),
                engine.current_scenario.name(),
                engine.hardware_profile.os_name,
                engine.hardware_profile.cpu_logical_cores
            ),
        );
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

    pub fn set_operating_mode(
        &mut self,
        mode: OperatingMode,
        sys: &mut System,
        is_ac: bool,
        battery_pct: u8,
    ) {
        self.operating_mode = mode;
        match mode {
            OperatingMode::HardwareOptimal => {
                let msgs = self.hardware_profile.apply_hardware_optimal(is_ac, battery_pct);
                self.log(
                    "[PHẦN CỨNG]",
                    format!(
                        "Kích hoạt Chế độ Tối ưu Phần Cứng & OS ({} nhân, {:.1}GB RAM, SSD: {}).",
                        self.hardware_profile.cpu_logical_cores,
                        self.hardware_profile.ram_total_gb,
                        self.hardware_profile.is_ssd
                    ),
                );
                for msg in msgs {
                    self.log("[ÁP DỤNG]", msg);
                }
            }
            OperatingMode::WorkloadAdaptive => {
                let idle_ms = get_user_idle_ms();
                if let Some(target_scen) = detect_recommended_scenario(sys, is_ac, battery_pct, idle_ms) {
                    self.current_scenario = target_scen;
                    let msg = apply_scenario(target_scen, sys, &self.hardware_profile, is_ac, battery_pct, self.is_pro);
                    self.log(
                        "[TỰ ĐỘNG]",
                        format!("Kích hoạt Tự Động Theo Công Việc -> [{}]: {}", target_scen.name(), msg),
                    );
                }
            }
        }
    }

    #[allow(dead_code)]
    pub fn set_manual_scenario(
        &mut self,
        scenario: Scenario,
        sys: &mut System,
        is_ac: bool,
        battery_pct: u8,
    ) {
        self.current_scenario = scenario;
        let msg = apply_scenario(scenario, sys, &self.hardware_profile, is_ac, battery_pct, self.is_pro);
        self.log(
            "[CHỌN]",
            format!("Người dùng chọn kịch bản [{}]: {}", scenario.name(), msg),
        );
    }

    pub fn tick(
        &mut self,
        sys: &mut System,
        is_ac: bool,
        battery_pct: u8,
        ram_load_pct: u32,
    ) -> bool {
        let mut state_changed = false;
        let idle_ms = get_user_idle_ms();

        // 1. Cảm biến nguồn điện (AC/DC Transition Sensor)
        if let Some(prev_ac) = self.last_power_ac {
            if prev_ac != is_ac {
                state_changed = true;
                if self.operating_mode == OperatingMode::HardwareOptimal {
                    let msgs = self.hardware_profile.apply_hardware_optimal(is_ac, battery_pct);
                    for m in msgs {
                        self.log("[NGUỒN]", m);
                    }
                } else {
                    // WorkloadAdaptive mode on AC/DC change
                    if !is_ac {
                        self.current_scenario = Scenario::CafeMobility;
                        let msg = apply_scenario(
                            Scenario::CafeMobility,
                            sys,
                            &self.hardware_profile,
                            is_ac,
                            battery_pct,
                            self.is_pro,
                        );
                        self.log(
                            "[PIN]",
                            format!("Rút sạc (Pin {}%) -> Tự động chuyển kịch bản di động: {}", battery_pct, msg),
                        );
                    } else {
                        self.current_scenario = Scenario::Default;
                        let msg = apply_scenario(
                            Scenario::Default,
                            sys,
                            &self.hardware_profile,
                            is_ac,
                            battery_pct,
                            self.is_pro,
                        );
                        self.log(
                            "[NGUỒN]",
                            format!("Cắm sạc AC -> Khôi phục hiệu năng tối đa: {}", msg),
                        );
                    }
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
                if let Some((pid, ref name)) = fg {
                    if self.is_pro {
                        crate::eco_qos::apply_pro_foreground_boost(pid, self.hardware_profile.cpu_logical_cores);
                        self.log(
                            "[PRO VIP]",
                            format!(
                                "Ưu tiên tối đa '{}' (High Priority, {} nhân CPU, hạ tải {} app nền).",
                                name, self.hardware_profile.cpu_logical_cores, eco_count
                            ),
                        );
                    } else if eco_count > 0 {
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

            // Tự động nhận diện theo chế độ làm việc (chỉ khi đang ở chế độ WorkloadAdaptive và auto_pilot_enabled)
            if self.operating_mode == OperatingMode::WorkloadAdaptive && self.auto_pilot_enabled {
                if let Some(target_scen) = detect_recommended_scenario(sys, is_ac, battery_pct, idle_ms) {
                    if target_scen != self.current_scenario {
                        self.current_scenario = target_scen;
                        state_changed = true;
                        let msg = apply_scenario(
                            target_scen,
                            sys,
                            &self.hardware_profile,
                            is_ac,
                            battery_pct,
                            self.is_pro,
                        );
                        self.log("[CHUYỂN]", format!("Phát hiện tác vụ [{}]: {}", target_scen.name(), msg));
                    }
                }
            }
        }

        // Kiểm tra nhàn rỗi (Idle) định kỳ trong chế độ tự động
        if self.operating_mode == OperatingMode::WorkloadAdaptive
            && self.auto_pilot_enabled
            && idle_ms >= 300_000
            && self.current_scenario != Scenario::IdlePowerSave
        {
            self.current_scenario = Scenario::IdlePowerSave;
            state_changed = true;
            let msg = apply_scenario(
                Scenario::IdlePowerSave,
                sys,
                &self.hardware_profile,
                is_ac,
                battery_pct,
                self.is_pro,
            );
            self.log("[NGHỈ]", msg);
        }

        // 3. Tự động thu hồi RAM khi vượt ngưỡng động (Tính theo dung lượng RAM thực tế của máy, KHÔNG hardcode)
        let ram_threshold = self.hardware_profile.dynamic_ram_threshold_pct;
        if self.auto_ram_enabled
            && ram_load_pct >= ram_threshold
            && self.last_ram_trim.elapsed() >= Duration::from_secs(60)
        {
            self.last_ram_trim = Instant::now();
            if self.is_pro {
                let (freed, count) = trim_working_sets(sys);
                if freed > 10 * 1024 * 1024 {
                    self.log(
                        "[PRO RAM]",
                        format!(
                            "RAM chạm {}% (ngưỡng máy {:.1}GB) -> Tự động thu hồi {} bộ nhớ từ {} tiến trình.",
                            ram_load_pct,
                            self.hardware_profile.ram_total_gb,
                            format_bytes(freed),
                            count
                        ),
                    );
                }
            }
        }

        state_changed
    }
}
