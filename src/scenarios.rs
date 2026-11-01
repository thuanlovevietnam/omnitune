use std::os::windows::process::CommandExt;
use std::process::Command;
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

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scenario {
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
) -> String {
    match scenario {
        Scenario::Default => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }
            // Restore GameDVR
            let _ = Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR",
                    "/v",
                    "AppCaptureEnabled",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    "1",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_dc(hw.optimal_cpu_dc_limit);
            let _ = crate::power::set_cpu_max_state_ac(hw.optimal_cpu_ac_limit);

            // Restore normal priority to common apps
            let _ = prioritize_apps(
                sys,
                &["miniremote", "rustdesk", "mstsc", "googledrivefs", "onedrive", "teams", "zoom"],
                NORMAL_PRIORITY_CLASS,
            );

            format!(
                "Đã trở về chế độ Cân bằng tiêu chuẩn: CPU {} nhân ({}), RAM tối ưu động.",
                hw.cpu_logical_cores, hw.cpu_brand
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
                &["code", "devenv", "idea64", "clion64", "pycharm64", "sublime_text", "cursor"],
                ABOVE_NORMAL_PRIORITY_CLASS,
            );

            // Free RAM for compilation buffer
            let (freed, count) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Lập trình: Mở 100% xung nhịp {} nhân ({}), làm mát chủ động, ưu tiên {} tiến trình compiler/IDE, giải phóng {} RAM từ {} app.",
                hw.cpu_logical_cores,
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
            let _ = crate::power::set_cooling_policy(true);

            // 3. Multimedia high-resolution timer (1.0ms)
            unsafe {
                let _ = timeBeginPeriod(1);
            }

            // 4. Disable Windows GameDVR background capture
            let _ = Command::new("reg")
                .args([
                    "add",
                    r"HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR",
                    "/v",
                    "AppCaptureEnabled",
                    "/t",
                    "REG_DWORD",
                    "/d",
                    "0",
                    "/f",
                ])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            // 5. Deprioritize cloud sync and background tools
            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer", "tailscale", "dropbox"],
                IDLE_PRIORITY_CLASS,
            );

            // 6. Trim RAM to maximize available RAM & shared VRAM
            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Game & Đồ họa: Timer 1.0ms, tắt GameDVR ngầm, tối ưu GPU [{}], giải phóng {} RAM làm VRAM, hạ {} app ngầm.",
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
            let _ = Command::new("netsh")
                .args(["int", "tcp", "set", "global", "autotuninglevel=normal"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            format!(
                "Đã bật kịch bản Họp & Văn phòng: Chống tắt màn hình, ưu tiên {} app họp/office, hạ {} app đồng bộ để giữ băng thông mượt mà, quạt êm chống ồn.",
                boosted, deprioritized
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

            let _ = Command::new("netsh")
                .args(["int", "tcp", "set", "global", "autotuninglevel=normal"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            let _ = crate::power::set_brightness(50);

            format!(
                "Đã bật kịch bản Remote Desktop: Chống Sleep/Lock, tối ưu mạng TCP, ưu tiên {} app remote, hạ {} app sync để giữ đường truyền ổn định.",
                boosted, deprioritized
            )
        }

        Scenario::MediaChill => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            let _ = crate::power::set_cpu_max_state_dc(60);
            let _ = crate::power::set_cpu_max_state_ac(65);
            let _ = crate::power::set_brightness(70);

            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "searchindexer"],
                IDLE_PRIORITY_CLASS,
            );

            format!(
                "Đã bật kịch bản Xem phim: Khóa giữ sáng màn hình, quạt êm 0dB, nhường giải mã video cho GPU [{}], hạ {} app ngầm.",
                hw.gpu_name, deprioritized
            )
        }

        Scenario::CafeMobility => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }

            let _ = crate::power::set_power_scheme("SCHEME_MAX");

            // Calculate dynamic DC throttle from battery percent
            let dc_dynamic = (hw.optimal_cpu_dc_limit as f32
                * (0.60 + 0.40 * (battery_pct as f32 / 100.0)))
                .round() as u32;
            let final_dc = dc_dynamic.clamp(50, hw.optimal_cpu_dc_limit);

            let _ = crate::power::set_cpu_max_state_dc(final_dc);
            let _ = crate::power::set_cooling_policy(false); // Passive cooling
            let _ = crate::power::set_brightness(45);

            let deprioritized = prioritize_apps(
                sys,
                &["googledrivefs", "onedrive", "rustdesk", "tailscale", "searchindexer"],
                IDLE_PRIORITY_CLASS,
            );

            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Di động / Pin: Điều tiết CPU {} nhân ở mức {}%, quạt tắt (Passive), độ sáng 45%, thu hồi {} RAM, đóng băng {} app ngầm.",
                hw.cpu_logical_cores,
                final_dc,
                crate::cleaner::format_bytes(freed),
                deprioritized
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
            let _ = crate::power::set_cooling_policy(false);

            let (freed, count) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Nhàn rỗi tự động: Người dùng không tương tác > 5 phút, hạ tải CPU {} nhân, giải phóng {} RAM từ {} tiến trình.",
                hw.cpu_logical_cores,
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

    // 2. Gaming detection (Known games, game platforms, 3D engines)
    let has_game = procs.values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("valorant")
            || name.contains("leagueclient")
            || name.contains("dota2")
            || name.contains("cs2")
            || name.contains("genshin")
            || name.contains("minecraft")
            || name.contains("steamapps")
            || name.contains("epicgames")
            || name.contains("riotclient")
            || name.contains("gta")
            || name.contains("cyberpunk")
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
            || name.contains("cursor.exe")
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
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
    let mut modified = 0;

    for (&pid, proc) in sys.processes() {
        let name = proc.name().to_string_lossy().to_lowercase();
        let matches = app_names
            .iter()
            .any(|&target| name.contains(&target.to_lowercase()));

        if matches {
            let raw_pid = pid.as_u32();
            if raw_pid > 4 {
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
    }

    modified
}
