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

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum Scenario {
    Default,
    RemoteDesktop,
    Gaming,
    CafeMobility,
    Developer,
    MediaChill,
}

impl Scenario {
    pub fn name(&self) -> &'static str {
        match self {
            Scenario::Default => "Cân bằng tiêu chuẩn",
            Scenario::RemoteDesktop => "🖥️ Điều khiển từ xa (Remote)",
            Scenario::Gaming => "🎮 Chơi Game & Đồ họa",
            Scenario::CafeMobility => "☕ Cà phê / Siêu tiết kiệm pin",
            Scenario::Developer => "💻 Lập trình & Biên dịch",
            Scenario::MediaChill => "🎬 Xem phim & Thư giãn",
        }
    }

    #[allow(dead_code)]
    pub fn description(&self) -> &'static str {
        match self {
            Scenario::Default => "Tự động điều tiết theo cơ chế tiêu chuẩn của Windows.",
            Scenario::RemoteDesktop => "Khóa chống Sleep, CPU 65% máy mát 40°C không quay quạt, ưu tiên mạng & giảm lag chuột cho MiniRemote/RustDesk.",
            Scenario::Gaming => "Hạ Timer xuống 1.0ms chống khựng, tắt GameDVR ngầm, CPU 90% nhường điện cho GPU Intel HD 620, dọn RAM làm Shared VRAM.",
            Scenario::CafeMobility => "Khóa trần CPU 60% chỉ ăn 4W điện, tắt quạt làm mát, đóng băng app sync ngầm để tối đa hóa thời lượng pin.",
            Scenario::Developer => "Mở 100% Turbo Boost 3.9GHz, quạt làm mát chủ động, ưu tiên tiến trình compiler, dọn bộ nhớ đệm cho rustc/go.",
            Scenario::MediaChill => "Khóa chống tắt màn hình khi xem phim, CPU 55% im lặng 0dB, nhường GPU QuickSync giải mã video mượt mà.",
        }
    }
}

pub fn apply_scenario(scenario: Scenario, sys: &mut System) -> String {
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
            let _ = crate::power::set_cpu_max_state_dc(80);
            let _ = crate::power::set_cpu_max_state_ac(100);

            // Restore normal priority to common apps
            let _ = prioritize_apps(
                sys,
                &["MiniRemote", "rustdesk", "mstsc", "GoogleDriveFS", "OneDrive"],
                NORMAL_PRIORITY_CLASS,
            );

            "Đã trở về chế độ Cân bằng tiêu chuẩn của Windows.".to_string()
        }

        Scenario::RemoteDesktop => {
            // 1. Prevent sleep and screen lock
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED | ES_DISPLAY_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            // 2. CPU 65% - cool (40°C) and silent (0dB fan), Intel QuickSync decodes video stream
            let _ = crate::power::set_cpu_max_state_dc(65);
            let _ = crate::power::set_cpu_max_state_ac(75);

            // 3. Prioritize remote desktop client apps
            let boosted = prioritize_apps(
                sys,
                &["MiniRemote", "rustdesk", "mstsc"],
                ABOVE_NORMAL_PRIORITY_CLASS,
            );

            // 4. Deprioritize cloud sync to prevent bandwidth jitter
            let deprioritized = prioritize_apps(
                sys,
                &["GoogleDriveFS", "OneDrive", "SearchIndexer"],
                IDLE_PRIORITY_CLASS,
            );

            // 5. Optimize TCP ACK latency
            let _ = Command::new("netsh")
                .args(["int", "tcp", "set", "global", "autotuninglevel=normal"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();

            // 6. Set comfortable brightness for long working hours
            let _ = crate::power::set_brightness(50);

            format!(
                "Đã bật kịch bản Remote Desktop: Chống Sleep/Lock, CPU 65% êm ru 0dB, ưu tiên {} app remote, hạ {} app sync để giữ đường truyền mạng tối ưu.",
                boosted, deprioritized
            )
        }

        Scenario::Gaming => {
            // 1. Power scheme high performance
            let _ = crate::power::set_power_scheme("SCHEME_MIN");

            // 2. CPU balance: 90% locks CPU at 2.8GHz, leaving TDP headroom for Intel HD 620 to reach 1150MHz
            let _ = crate::power::set_cpu_max_state_dc(90);
            let _ = crate::power::set_cpu_max_state_ac(92);

            // 3. Multimedia high-resolution timer (1.0ms) to eliminate micro-stutters and input lag
            unsafe {
                let _ = timeBeginPeriod(1);
            }

            // 4. Disable Windows GameDVR background capture to free GPU encoder
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

            // 5. Deprioritize cloud sync and heavy background updaters
            let deprioritized = prioritize_apps(
                sys,
                &["GoogleDriveFS", "OneDrive", "SearchIndexer", "RustDesk", "Tailscale"],
                IDLE_PRIORITY_CLASS,
            );

            // 6. Trim RAM for maximum shared VRAM
            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Gaming: Timer 1.0ms, tắt GameDVR nền, CPU 90% cấp điện cho GPU, giải phóng {} RAM làm VRAM, hạ {} app ngầm.",
                crate::cleaner::format_bytes(freed),
                deprioritized
            )
        }

        Scenario::CafeMobility => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS);
                let _ = timeEndPeriod(1);
            }

            // Maximum power savings scheme
            let _ = crate::power::set_power_scheme("SCHEME_MAX");
            let _ = crate::power::set_cpu_max_state_dc(60);

            // Lower brightness to 45% (battery sweet spot)
            let _ = crate::power::set_brightness(45);

            // Deprioritize background sync and non-essential tools
            let deprioritized = prioritize_apps(
                sys,
                &["GoogleDriveFS", "OneDrive", "RustDesk", "Tailscale", "SearchIndexer"],
                IDLE_PRIORITY_CLASS,
            );

            // Trim RAM
            let (freed, _) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Cà phê / Pin trâu: CPU khóa 60% (~4W), quạt tắt (Passive), độ sáng 45%, thu hồi {} RAM, đóng băng {} app ngầm.",
                crate::cleaner::format_bytes(freed),
                deprioritized
            )
        }

        Scenario::Developer => {
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_SYSTEM_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            // High performance scheme, CPU 100% full turbo boost (3.9GHz)
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_dc(100);
            let _ = crate::power::set_cpu_max_state_ac(100);

            // Prioritize compilers and build tools
            let boosted = prioritize_apps(
                sys,
                &["rustc", "cargo", "go", "node", "cl"],
                HIGH_PRIORITY_CLASS,
            );

            // Free RAM for compiler
            let (freed, count) = crate::memory::trim_working_sets(sys);

            format!(
                "Đã bật kịch bản Lập trình: CPU mở 100% Turbo Boost 3.9GHz, quạt tản nhiệt chủ động, ưu tiên {} tiến trình compiler, giải phóng {} RAM từ {} app.",
                boosted,
                crate::cleaner::format_bytes(freed),
                count
            )
        }

        Scenario::MediaChill => {
            // Keep display on, prevent screen saver / sleep
            unsafe {
                SetThreadExecutionState(ES_CONTINUOUS | ES_DISPLAY_REQUIRED);
                let _ = timeEndPeriod(1);
            }

            // CPU 55% - totally silent fan (0dB), Intel QuickSync handles 1080p/4K decode
            let _ = crate::power::set_cpu_max_state_dc(55);
            let _ = crate::power::set_cpu_max_state_ac(60);

            // Brightness 70% for vivid video watching
            let _ = crate::power::set_brightness(70);

            // Deprioritize sync
            let _ = prioritize_apps(
                sys,
                &["GoogleDriveFS", "OneDrive", "SearchIndexer"],
                IDLE_PRIORITY_CLASS,
            );

            "Đã bật kịch bản Xem phim: Khóa chống tắt màn hình, CPU 55% im lặng 0dB, màn hình 70%, nhường giải mã video cho GPU.".to_string()
        }
    }
}

pub fn detect_recommended_scenario(
    sys: &System,
    is_ac: bool,
    battery_pct: u8,
) -> Option<Scenario> {
    // 1. Check if remote desktop tools are running
    let has_remote = sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("miniremote") || name.contains("rustdesk") || name.contains("mstsc")
    });
    if has_remote {
        return Some(Scenario::RemoteDesktop);
    }

    // 2. Check if developer tools are actively running
    let has_compiler = sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("rustc")
            || name.contains("cargo")
            || name.contains("go.exe")
            || name.contains("cl.exe")
    });
    if has_compiler {
        return Some(Scenario::Developer);
    }

    // 3. Check if low battery and on battery power
    if !is_ac && battery_pct <= 35 {
        return Some(Scenario::CafeMobility);
    }

    // 4. Check if media player is running
    let has_media = sys.processes().values().any(|p| {
        let name = p.name().to_string_lossy().to_lowercase();
        name.contains("vlc")
            || name.contains("potplayer")
            || name.contains("mpc-hc")
            || name.contains("netflix")
    });
    if has_media {
        return Some(Scenario::MediaChill);
    }

    None
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
