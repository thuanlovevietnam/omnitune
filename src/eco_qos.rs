use sysinfo::System;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, SetPriorityClass, SetProcessAffinityMask, SetProcessInformation,
    ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS, PROCESS_POWER_THROTTLING_CURRENT_VERSION,
    PROCESS_POWER_THROTTLING_EXECUTION_SPEED, PROCESS_POWER_THROTTLING_STATE,
    PROCESS_SET_INFORMATION, ProcessPowerThrottling,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub fn get_foreground_process(sys: &System) -> Option<(u32, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid <= 4 {
            return None;
        }

        let name = sys
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| p.name().to_string_lossy().to_string())
            .unwrap_or_else(|| format!("PID {}", pid));

        Some((pid, name))
    }
}

pub fn set_process_eco_mode(pid: u32, enable_eco: bool) -> bool {
    if pid <= 4 {
        return false;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }

        let mut throttling_state: PROCESS_POWER_THROTTLING_STATE = std::mem::zeroed();
        throttling_state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION;
        throttling_state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
        throttling_state.StateMask = if enable_eco {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        };

        let res = SetProcessInformation(
            handle,
            ProcessPowerThrottling,
            &throttling_state as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        CloseHandle(handle);
        res != 0
    }
}

pub fn apply_foreground_boost_and_eco(
    sys: &System,
    fg_pid: Option<u32>,
) -> (usize, usize) {
    let mut boosted = 0;
    let mut eco_throttled = 0;

    // 1. Boost active foreground process
    if let Some(pid) = fg_pid {
        if pid > 4 {
            unsafe {
                let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
                if !handle.is_null() {
                    let _ = SetPriorityClass(handle, ABOVE_NORMAL_PRIORITY_CLASS);
                    CloseHandle(handle);
                }
            }
            if set_process_eco_mode(pid, false) {
                boosted += 1;
            }
        }
    }

    // 2. Target heavy background candidate names for EcoQoS
    let eco_candidates = [
        "googledrivefs",
        "onedrive",
        "searchindexer",
        "tailscale",
        "msedge",
        "chrome",
        "brave",
    ];

    for (&pid, proc) in sys.processes() {
        let raw_pid = pid.as_u32();
        if Some(raw_pid) == fg_pid || raw_pid <= 4 {
            continue;
        }

        let name = proc.name().to_string_lossy().to_lowercase();
        let should_eco = eco_candidates.iter().any(|&c| name.contains(c));

        if should_eco {
            if set_process_eco_mode(raw_pid, true) {
                eco_throttled += 1;
            }
        }
    }

    (boosted, eco_throttled)
}

pub fn apply_pro_foreground_boost(pid: u32, total_cores: usize) -> bool {
    if pid <= 4 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }

        // Cấp High Priority Class cho ứng dụng đang focus
        let _ = SetPriorityClass(handle, HIGH_PRIORITY_CLASS);

        // Gán CPU Affinity toàn bộ core logic
        if total_cores > 0 {
            let mask = if total_cores >= (std::mem::size_of::<usize>() * 8) {
                usize::MAX
            } else {
                (1usize << total_cores).wrapping_sub(1)
            };
            let _ = SetProcessAffinityMask(handle, mask);
        }

        CloseHandle(handle);
        true
    }
}
