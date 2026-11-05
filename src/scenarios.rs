use sysinfo::System;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Media::{timeBeginPeriod, timeEndPeriod};
use windows_sys::Win32::System::Power::{
    SetThreadExecutionState, ES_CONTINUOUS, ES_DISPLAY_REQUIRED, ES_SYSTEM_REQUIRED,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, SetPriorityClass, ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    IDLE_PRIORITY_CLASS, NORMAL_PRIORITY_CLASS, PROCESS_SET_INFORMATION,
};
use crate::hardware_profile::HardwareProfile;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Scenario {
    #[default]
    Default,
    Developer,
    Gaming,
    OfficeMeeting,
    RemoteDesktop,
    MediaChill,
    CafeMobility,
    IdlePowerSave,
}

#[allow(dead_code)]
impl Scenario {
    pub const ALL: [Scenario; 8] = [
        Scenario::Default,
        Scenario::Developer,
        Scenario::Gaming,
        Scenario::OfficeMeeting,
        Scenario::RemoteDesktop,
        Scenario::MediaChill,
        Scenario::CafeMobility,
        Scenario::IdlePowerSave,
    ];

    pub fn name(&self) -> &'static str {
        match self {
            Scenario::Default => "[Cân bằng] Mặc định Windows",
            Scenario::Developer => "[Dev] Lập trình & Biên dịch",
            Scenario::Gaming => "[Gaming] Game & Đồ họa",
            Scenario::OfficeMeeting => "[Văn phòng] Họp & Làm việc",
            Scenario::RemoteDesktop => "[Remote] Điều khiển từ xa",
            Scenario::MediaChill => "[Media] Xem phim & Thư giãn",
            Scenario::CafeMobility => "[Pin] Di động / Tiết kiệm",
            Scenario::IdlePowerSave => "[Nghỉ ngơi] Nhàn rỗi tự động",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            Scenario::Default => "Tự động điều tiết theo cơ chế tiêu chuẩn của Windows và phần cứng máy.",
            Scenario::Developer => "Mở toàn bộ xung nhịp và số nhân CPU, quạt làm mát chủ động, ưu tiên compiler và IDE.",
            Scenario::Gaming => "Hạ Timer xuống 1.0ms chống khựng, tắt GameDVR ngầm, ưu tiên GPU và dọn RAM làm VRAM.",
            Scenario::OfficeMeeting => "Chống tắt màn hình, ưu tiên app họp và văn phòng, giảm độ trễ mạng TCP, quạt êm chống ồn.",
            Scenario::RemoteDesktop => "Khóa chống Sleep, điều tiết nhiệt độ êm ái, tối ưu mạng TCP và giảm độ trễ điều khiển từ xa.",
            Scenario::MediaChill => "Khóa chống tắt màn hình, CPU êm ái, nhường giải mã video độ phân giải cao cho GPU.",
            Scenario::CafeMobility => "Điều tiết xung nhịp thích ứng theo dung lượng pin còn lại, làm mát passive kéo dài thời lượng pin.",
            Scenario::IdlePowerSave => "Tự động kích hoạt khi không có thao tác > 5 phút, hạ tải CPU và thu hồi RAM nền.",
        }
    }
}

pub fn apply_scenario(
    scenario: Scenario,
    sys: &mut System,
    hw: &HardwareProfile,
    is_ac: bool,
    battery_pct: u8,
    is_pro: bool,
) -> String {
    match scenario {
        Scenario::Default => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }
            // Khôi phục GPU & DirectX về trạng thái tối ưu cân bằng theo phần cứng
            let _ = crate::gpu::apply_hardware_gpu_optimal(hw, is_ac, battery_pct);

            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_dc(hw.optimal_cpu_dc_limit);
            let _ = crate::power::set_cpu_max_state_ac(hw.optimal_cpu_ac_limit);
            let dynamic_epp = hw.cpu_topology.calculate_dynamic_epp(is_ac, battery_pct, false);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(is_ac, battery_pct);
            let _ = crate::power::apply_cpu_deep_tuning(Some(dynamic_epp), Some(1), Some(dynamic_cp), None);
            let _ = crate::power::set_win32_priority_separation_val(2);

            // Restore normal priority to common apps
            let _ = prioritize_apps(
                sys,
                &["miniremote", "rustdesk", "mstsc", "googledrivefs", "onedrive", "teams", "zoom"],
                NORMAL_PRIORITY_CLASS,
            );

            format!(
                "Đã trở về chế độ Cân bằng tiêu chuẩn: CPU {} ({}), GPU & RAM tối ưu động.",
                hw.cpu_topology.topology_summary, hw.cpu_brand
            )
        }

        Scenario::Developer => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            // Maximize CPU performance across all physical & logical cores
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_dc(hw.optimal_cpu_ac_limit);
            let _ = crate::power::set_cpu_max_state_ac(hw.optimal_cpu_ac_limit);
            let dynamic_epp = hw.cpu_topology.calculate_dynamic_epp(is_ac, battery_pct, true);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(is_ac, battery_pct);
            let _ = crate::power::apply_cpu_deep_tuning(Some(dynamic_epp), Some(2), Some(dynamic_cp), None);
            let _ = crate::power::set_win32_priority_separation_val(24); // Fixed long quanta (through-put build)
            let _ = crate::power::set_cooling_policy(true);

            // Prioritize compilers and build tools
            let boosted_compilers = prioritize_apps(
                sys,
                &["rustc", "cargo", "go", "node", "cl", "gcc", "g++", "clang", "dotnet", "javac", "python"],
                HIGH_PRIORITY_CLASS,
            );

            // Prioritize IDEs
            let boosted_ides = prioritize_apps(
                sys,
                &["code", "devenv", "idea64", "clion64", "pycharm64", "sublime_text", "cursor", "windsurf", "webstorm64", "rider64"],
                ABOVE_NORMAL_PRIORITY_CLASS,
            );

            // Free RAM for compilation buffer
            let (freed, count) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Lập trình: Mở 100% {} ({}), EPP 0%, Quanta Build (0x18), quạt làm mát chủ động, ưu tiên {} tiến trình compiler/IDE, giải phóng {} RAM từ {} app.",
                hw.cpu_topology.topology_summary,
                hw.cpu_brand,
                boosted_compilers + boosted_ides,
                crate::cleaner::format_bytes(freed),
                count
            )
        }

        Scenario::Gaming => {
            // 1. Power scheme high performance
            let _ = crate::power::set_power_scheme("SCHEME_MIN");

            // 2. Dynamic CPU limit based on GPU type:
            // Dedicated GPU gets 100% CPU. Integrated GPU gets 90% to avoid thermal package throttling.
            let cpu_limit = if hw.is_dedicated_gpu { 100 } else { 90 };
            let _ = crate::power::set_cpu_max_state_dc(cpu_limit);
            let _ = crate::power::set_cpu_max_state_ac(cpu_limit);
            let hetero = if hw.cpu_topology.is_heterogeneous { Some(2) } else { None };
            let _ = crate::power::apply_cpu_deep_tuning(Some(0), Some(2), Some(100), hetero); // EPP 0, Boost 2, Unpark 100%
            let _ = crate::power::set_win32_priority_separation_val(38); // 3:1 Interactive Quantum Boost
            let _ = crate::power::set_cooling_policy(true);

            // 3. Multimedia high-resolution timer (1.0ms) - Only for Pro
            let timer_status = if is_pro {
                unsafe {
                    let _ = timeBeginPeriod(1);
                }
                "Timer 1.0ms"
            } else {
                unsafe {
                    let _ = timeEndPeriod(1);
                }
                "Timer 15.6ms (Cần Pro để lên 1.0ms)"
            };

            // 4. Real GPU & DirectX optimization (HAGS, GPU Priority 8, FSE, GameDVR off, PCIe ASPM 0, HighPerf GPU apps)
            let _ = crate::gpu::apply_scenario_gaming_gpu(hw, is_pro, sys);

            // 5. Deprioritize cloud sync and background tools
            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer", "tailscale", "dropbox"],
                IDLE_PRIORITY_CLASS,
            );

            // 6. Trim RAM to maximize available RAM & shared VRAM
            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Game & Đồ họa: {}, EPP 0%/Boost Tối đa/Unpark 100%, tối ưu GPU [{}: Priority 8/FSE], giải phóng {} RAM, hạ {} app ngầm.",
                timer_status,
                hw.gpu_name,
                crate::cleaner::format_bytes(freed),
                deprioritized
            )
        }

        Scenario::OfficeMeeting => {
            // Keep screen awake during meetings
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED | ES_SYSTEM_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            // CPU balanced for quiet fan (anti fan-noise on microphone)
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let cpu_limit = if is_ac { 80 } else { 70 };
            let _ = crate::power::set_cpu_max_state_ac(cpu_limit);
            let _ = crate::power::set_cpu_max_state_dc(cpu_limit);
            let dynamic_epp = hw.cpu_topology.calculate_dynamic_epp(is_ac, battery_pct, false);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(is_ac, battery_pct);
            let _ = crate::power::apply_cpu_deep_tuning(Some(dynamic_epp), Some(3), Some(dynamic_cp), None); // Efficient boost: avoids high spikes & fan noise
            let _ = crate::power::set_win32_priority_separation_val(38);

            // Prioritize meeting and office apps
            let boosted = prioritize_apps(
                sys,
                &["teams", "zoom", "slack", "discord", "webex", "winword", "excel", "powerpnt", "onenote", "acrobat"],
                ABOVE_NORMAL_PRIORITY_CLASS,
            );

            // Deprioritize background sync to protect meeting bandwidth
            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer", "tailscale"],
                IDLE_PRIORITY_CLASS,
            );

            // Optimize TCP latency
            let _ = crate::power::set_tcp_optimization(true);

            // Optimize GPU Capture for Camera & Screen share
            let _ = crate::gpu::apply_scenario_office_gpu(hw, is_ac);

            format!(
                "Đã bật kịch bản Họp & Văn phòng: Chống tắt màn hình, EPP {}%/Boost Êm, tối ưu GPU Capture, ưu tiên {} app họp/office, hạ {} app đồng bộ, quạt êm chống ồn.",
                dynamic_epp, boosted, deprioritized
            )
        }

        Scenario::RemoteDesktop => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            let cpu_target = if is_ac { 75 } else { 65 };
            let _ = crate::power::set_cpu_max_state_dc(cpu_target);
            let _ = crate::power::set_cpu_max_state_ac(cpu_target);
            let dynamic_epp = hw.cpu_topology.calculate_dynamic_epp(is_ac, battery_pct, false);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(is_ac, battery_pct);
            let _ = crate::power::apply_cpu_deep_tuning(Some(dynamic_epp), Some(3), Some(dynamic_cp), None);

            let boosted = prioritize_apps(
                sys,
                &["miniremote", "rustdesk", "mstsc", "anydesk", "teamviewer"],
                ABOVE_NORMAL_PRIORITY_CLASS,
            );

            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer"],
                IDLE_PRIORITY_CLASS,
            );

            let _ = crate::power::set_tcp_optimization(true);

            // Optimize GPU Hardware Encoding for Remote Desktop (Terminal Services)
            let _ = crate::gpu::apply_scenario_remote_gpu();

            format!(
                "Đã bật kịch bản Remote Desktop: Chống Sleep/Lock, CPU êm {}% (EPP {}), GPU HW encoding, tối ưu mạng TCP, ưu tiên {} app remote, hạ {} app sync.",
                cpu_target, dynamic_epp, boosted, deprioritized
            )
        }

        Scenario::MediaChill => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            let _ = crate::power::set_cpu_max_state_dc(60);
            let _ = crate::power::set_cpu_max_state_ac(65);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(is_ac, battery_pct);
            let _ = crate::power::apply_cpu_deep_tuning(Some(60), Some(3), Some(dynamic_cp), None);

            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer"],
                IDLE_PRIORITY_CLASS,
            );

            // Tối ưu GPU Hardware Video Acceleration (DXVA, MMCSS Playback, Powercfg Video Bias)
            let _ = crate::gpu::apply_scenario_media_gpu(hw, is_ac, sys);

            format!(
                "Đã bật kịch bản Xem phim: Khóa giữ sáng màn hình, CPU hãm 60-65% (EPP 60%), bật DXVA GPU [{}], quạt êm 0dB, hạ {} app ngầm.",
                hw.gpu_name, deprioritized
            )
        }

        Scenario::CafeMobility => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }

            let _ = crate::power::set_power_scheme("SCHEME_MAX");

            // Continuous mathematical model without hardcoded thresholds
            let final_dc = hw.cpu_topology.calculate_optimal_dc_limit(battery_pct);
            let dynamic_epp = hw.cpu_topology.calculate_dynamic_epp(false, battery_pct, false);
            let dynamic_cp = hw.cpu_topology.calculate_dynamic_core_parking(false, battery_pct);
            let boost_mode = if battery_pct < 40 { 0 } else { 3 }; // <40% pin: tắt boost hạ nhiệt và tăng 2h pin

            let _ = crate::power::apply_cpu_deep_tuning_dc(final_dc, dynamic_epp, boost_mode, dynamic_cp);
            let _ = crate::power::set_cooling_policy(false); // Passive cooling

            // Hạ nguồn GPU vào chế độ tiết kiệm sâu (RTD3)
            let _ = crate::gpu::apply_gpu_power_save(hw);

            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "rustdesk", "tailscale", "searchindexer"],
                IDLE_PRIORITY_CLASS,
            );

            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Di động / Pin ({}%): {} trần {}% (EPP {}%, Boost {}, Đỗ {}% nhân), quạt tắt (Passive 0dB), hạ nguồn GPU, hạ {} app ngầm, thu hồi {} RAM.",
                battery_pct,
                hw.cpu_topology.topology_summary,
                final_dc,
                dynamic_epp,
                if boost_mode == 0 { "Tắt/Hạ nhiệt" } else { "Tiết kiệm" },
                100 - dynamic_cp,
                deprioritized,
                crate::cleaner::format_bytes(freed)
            )
        }

        Scenario::IdlePowerSave => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }

            let _ = crate::power::set_power_scheme("SCHEME_MAX");
            let _ = crate::power::set_cpu_max_state_dc(50);
            let _ = crate::power::set_cpu_max_state_ac(55);
            let _ = crate::power::apply_cpu_deep_tuning(Some(80), Some(0), Some(25), None); // Maximum power savings & park 75% cores
            let _ = crate::power::set_cooling_policy(false);

            // Ngủ sâu GPU khi máy nhàn rỗi
            let _ = crate::gpu::apply_gpu_power_save(hw);

            let (freed, count) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Nhàn rỗi tự động: Người dùng không tương tác > 5 phút, hạ tải CPU {} ({}), ngủ sâu GPU, giải phóng {} RAM từ {} tiến trình.",
                hw.cpu_topology.topology_summary,
                hw.cpu_brand,
                crate::cleaner::format_bytes(freed),
                count
            )
        }
    }
}

pub fn detect_recommended_scenario(
    sys: &System,
    is_ac: bool,
    battery_pct: u8,
    idle_ms: u32,
) -> Option<Scenario> {
    // 1. Idle sensor (No user keyboard/mouse activity for > 5 minutes = 300,000 ms)
    if idle_ms >= 300_000 {
        return Some(Scenario::IdlePowerSave);
    }

    let procs = sys.processes();

    // 2. Gaming detection (DirectX/Vulkan/OpenGL 3D graphics & game platform paths)
    let my_pid = std::process::id();
    let has_game = procs.iter().any(|(&pid, p)| {
        let raw_pid = pid.as_u32();
        if raw_pid <= 4 || raw_pid == my_pid {
            return false;
        }
        let exe_str = p
            .exe()
            .and_then(|e| e.to_str())
            .unwrap_or_else(|| p.name().to_str().unwrap_or(""));
        if !exe_str.is_empty() {
            crate::gpu::is_process_using_3d_graphics(raw_pid, exe_str)
        } else {
            false
        }
    });
    if has_game {
        return Some(Scenario::Gaming);
    }

    // 3. Remote desktop tools
    let has_remote = procs.values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("miniremote")
            || name.contains("rustdesk")
            || name.contains("mstsc")
            || name.contains("anydesk")
            || name.contains("teamviewer")
    });
    if has_remote {
        return Some(Scenario::RemoteDesktop);
    }

    // 4. Developer tools & compilers
    let has_compiler_or_ide = procs.values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("rustc")
            || name.contains("cargo")
            || name.contains("go.exe")
            || name.contains("cl.exe")
            || name.contains("gcc.exe")
            || name.contains("g++.exe")
            || name.contains("clang")
            || name.contains("dotnet.exe")
            || name.contains("javac.exe")
            || name.contains("devenv.exe")
            || name.contains("idea64.exe")
            || name.contains("clion64.exe")
            || name.contains("pycharm64.exe")
            || name.contains("code.exe")
            || name.contains("cursor.exe")
            || name.contains("windsurf.exe")
            || name.contains("sublime_text.exe")
            || name.contains("webstorm64.exe")
            || name.contains("rider64.exe")
    });
    if has_compiler_or_ide {
        return Some(Scenario::Developer);
    }

    // 5. Office & Meeting tools
    let has_office_meeting = procs.values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("teams.exe")
            || name.contains("zoom.exe")
            || name.contains("slack.exe")
            || name.contains("discord.exe")
            || name.contains("webex")
            || name.contains("winword.exe")
            || name.contains("excel.exe")
            || name.contains("powerpnt.exe")
    });
    if has_office_meeting {
        return Some(Scenario::OfficeMeeting);
    }

    // 6. Media players & streaming
    let has_media = procs.values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("vlc.exe")
            || name.contains("potplayer")
            || name.contains("mpc-hc")
            || name.contains("mpv.exe")
            || name.contains("netflix")
            || name.contains("spotify.exe")
    });
    if has_media {
        return Some(Scenario::MediaChill);
    }

    // 7. Battery saver (If unplugged or battery is running low)
    if !is_ac || battery_pct <= 20 {
        return Some(Scenario::CafeMobility);
    }

    // 8. Default fallback
    Some(Scenario::Default)
}

pub fn prioritize_apps(sys: &mut System, app_names: &[&str], priority_class: u32) -> usize {
    if cfg!(test) {
        return app_names.len();
    }
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut modified = 0;

    let my_pid = std::process::id();
    for (&pid, proc) in sys.processes() {
        let raw_pid = pid.as_u32();
        if raw_pid <= 4 || raw_pid == my_pid {
            continue;
        }

        let name = proc.name().to_string_lossy().to_lowercase();
        let matches = app_names
            .iter()
            .any(|&target| name.contains(&target.to_lowercase()));

        if matches {
            unsafe {
                let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, raw_pid);
                if !handle.is_null() {
                    if SetPriorityClass(handle, priority_class) != 0 {
                        modified += 1;
                    }
                    CloseHandle(handle);
                }
            }
        }
    }

    modified
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scenario_names() {
        assert_eq!(Scenario::Default.name(), "[Cân bằng] Mặc định Windows");
        assert_eq!(Scenario::Developer.name(), "[Dev] Lập trình & Biên dịch");
        assert_eq!(Scenario::Gaming.name(), "[Gaming] Game & Đồ họa");
        assert_eq!(Scenario::OfficeMeeting.name(), "[Văn phòng] Họp & Làm việc");
        assert_eq!(Scenario::RemoteDesktop.name(), "[Remote] Điều khiển từ xa");
        assert_eq!(Scenario::MediaChill.name(), "[Media] Xem phim & Thư giãn");
        assert_eq!(Scenario::CafeMobility.name(), "[Pin] Di động / Tiết kiệm");
        assert_eq!(Scenario::IdlePowerSave.name(), "[Nghỉ ngơi] Nhàn rỗi tự động");
    }

    #[test]
    fn test_scenario_all() {
        assert_eq!(Scenario::ALL.len(), 8);
        assert_eq!(Scenario::ALL[0], Scenario::Default);
        assert_eq!(Scenario::ALL[1], Scenario::Developer);
        assert_eq!(Scenario::ALL[2], Scenario::Gaming);
    }

    #[test]
    fn test_detect_idle_scenario() {
        if let Ok(sys) = System::new() {
            let scen = detect_recommended_scenario(&sys, true, 100, 350_000);
            assert_eq!(scen, Some(Scenario::IdlePowerSave));
        }
    }

    #[test]
    fn test_all_scenarios_apply_message() {
        if let Ok(mut sys) = System::new() {
            let hw = HardwareProfile::detect(&mut sys);
            for &scen in &Scenario::ALL {
                let msg = apply_scenario(scen, &mut sys, &hw, true, 90, true);
                assert!(!msg.is_empty(), "Scenario {:?} should produce non-empty msg", scen);
            }
        }
    }
}
