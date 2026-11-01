use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct PowerStatus {
    pub has_battery: bool,
    pub is_ac_online: bool,
    pub battery_percent: u8,
    pub is_charging: bool,
    pub remaining_seconds: Option<u32>,
    pub discharge_rate_mw: u32,
    pub charge_rate_mw: u32,
    pub power_watts: f32,
    pub voltage_volts: f32,
}

pub fn get_power_status() -> PowerStatus {
    unsafe {
        let mut status: SYSTEM_POWER_STATUS = std::mem::zeroed();
        if GetSystemPowerStatus(&mut status) != 0 {
            let has_battery = (status.BatteryFlag & 128) == 0 && status.BatteryLifePercent <= 100;
            let is_ac = status.ACLineStatus == 1;
            let percent = if has_battery {
                status.BatteryLifePercent
            } else {
                100
            };
            let is_charging = (status.BatteryFlag & 8) != 0;
            let remaining = if status.BatteryLifeTime != u32::MAX && status.BatteryLifeTime > 0 {
                Some(status.BatteryLifeTime)
            } else {
                None
            };

            return PowerStatus {
                has_battery,
                is_ac_online: is_ac,
                battery_percent: percent,
                is_charging,
                remaining_seconds: remaining,
                discharge_rate_mw: 0,
                charge_rate_mw: 0,
                power_watts: 0.0,
                voltage_volts: 0.0,
            };
        }
    }

    PowerStatus::default()
}

#[allow(dead_code)]
pub fn get_active_scheme_name() -> String {
    let output = Command::new("powercfg")
        .arg("/getactivescheme")
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        if let Some(start) = text.find('(') {
            if let Some(end) = text.find(')') {
                if end > start {
                    return text[start + 1..end].trim().to_string();
                }
            }
        }
    }
    "Balanced".to_string()
}

pub fn set_power_scheme(scheme_alias_or_guid: &str) -> bool {
    Command::new("powercfg")
        .args(["/setactive", scheme_alias_or_guid])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_cpu_max_state_dc(percent: u32) -> bool {
    let pct_str = percent.clamp(50, 100).to_string();
    let schemes = [
        "SCHEME_CURRENT",
        "381b4222-f694-41f0-9685-ff5bb260df2e", // Balanced
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", // High Performance
        "a1841308-3541-4fab-bc81-f71556f20b4a", // Power Saver
    ];

    for s in schemes {
        let _ = Command::new("powercfg")
            .args(["/setdcvalueindex", s, "SUB_PROCESSOR", "PROCTHROTTLEMAX", &pct_str])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    let _ = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    true
}

pub fn set_cpu_max_state_ac(percent: u32) -> bool {
    let pct_str = percent.clamp(50, 100).to_string();
    let schemes = [
        "SCHEME_CURRENT",
        "381b4222-f694-41f0-9685-ff5bb260df2e", // Balanced
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c", // High Performance
        "a1841308-3541-4fab-bc81-f71556f20b4a", // Power Saver
    ];

    for s in schemes {
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", s, "SUB_PROCESSOR", "PROCTHROTTLEMAX", &pct_str])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }

    let _ = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    true
}

pub fn set_brightness(level: u32) -> bool {
    let script = format!(
        "(Get-WmiObject -Namespace root/wmi -Class WmiMonitorBrightnessMethods).WmiSetBrightness(1, {})",
        level.clamp(10, 100)
    );

    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[allow(dead_code)]
pub fn get_brightness() -> u32 {
    80
}

pub fn get_cpu_max_state_dc() -> u32 {
    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "PROCTHROTTLEMAX"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Current DC Power Setting Index:") {
                let parts: Vec<&str> = line.split(':').collect();
                if let Some(hex_str) = parts.get(1) {
                    let cleaned = hex_str.trim().trim_start_matches("0x");
                    if let Ok(val) = u32::from_str_radix(cleaned, 16) {
                        return val;
                    }
                }
            }
        }
    }
    80
}

pub fn get_cpu_max_state_ac() -> u32 {
    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "PROCTHROTTLEMAX"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Current AC Power Setting Index:") {
                let parts: Vec<&str> = line.split(':').collect();
                if let Some(hex_str) = parts.get(1) {
                    let cleaned = hex_str.trim().trim_start_matches("0x");
                    if let Ok(val) = u32::from_str_radix(cleaned, 16) {
                        return val;
                    }
                }
            }
        }
    }
    100
}

pub fn set_cooling_policy(active: bool) -> bool {
    let val = if active { "1" } else { "0" };
    let res1 = Command::new("powercfg")
        .args(["/setdcvalueindex", "SCHEME_CURRENT", "SUB_PROCESSOR", "SYSCOOLPOL", val])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let res2 = Command::new("powercfg")
        .args(["/setacvalueindex", "SCHEME_CURRENT", "SUB_PROCESSOR", "SYSCOOLPOL", val])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let res3 = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    res1 && res2 && res3
}

pub fn get_cooling_policy() -> bool {
    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "SYSCOOLPOL"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        for line in text.lines() {
            if line.contains("Current DC Power Setting Index:") {
                return line.contains("0x00000001");
            }
        }
    }
    true
}

pub fn set_gamedvr(enable: bool) -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let val_num = if enable { 1 } else { 0 };

    // 1. HKCU AppCaptureEnabled
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
        "AppCaptureEnabled",
        val_num,
    );

    // 2. HKCU GameConfigStore GameDVR_Enabled
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"System\GameConfigStore",
        "GameDVR_Enabled",
        val_num,
    );

    // 3. FSE Behavior Mode
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"System\GameConfigStore",
        "GameDVR_FSEBehaviorMode",
        if enable { 0 } else { 2 },
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"System\GameConfigStore",
        "GameDVR_HonorUserFSEBehaviorMode",
        if enable { 0 } else { 1 },
    );

    // 4. HKLM Policy AllowGameDVR (Permanent system-wide policy)
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Windows\GameDVR",
        "AllowGameDVR",
        val_num,
    );

    true
}

pub fn get_gamedvr() -> bool {
    crate::reg::get_gamedvr_native()
}

pub fn set_timer_resolution_1ms(enable: bool) {
    unsafe {
        if enable {
            let _ = windows_sys::Win32::Media::timeBeginPeriod(1);
        } else {
            let _ = windows_sys::Win32::Media::timeEndPeriod(1);
        }
    }

    // Ghi vĩnh viễn vào hệ điều hành Windows: BCDEDIT & Multimedia SystemProfile
    // Giữ hiệu lực 100% vĩnh viễn trên Windows kể cả khi tắt hoàn toàn app!
    if enable {
        let _ = Command::new("bcdedit").args(["/set", "disabledynamictick", "yes"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("bcdedit").args(["/set", "useplatformtick", "yes"]).creation_flags(CREATE_NO_WINDOW).status();
    } else {
        let _ = Command::new("bcdedit").args(["/deletevalue", "disabledynamictick"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("bcdedit").args(["/deletevalue", "useplatformtick"]).creation_flags(CREATE_NO_WINDOW).status();
    }

    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let multimedia_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
    let tasks_games_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";

    if enable {
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, multimedia_key, "SystemResponsiveness", 0);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, multimedia_key, "NetworkThrottlingIndex", 0xFFFFFFFF);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "GPU Priority", 8);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 6);
        crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "High");
    } else {
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, multimedia_key, "SystemResponsiveness", 20);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, multimedia_key, "NetworkThrottlingIndex", 10);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 2);
        crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "Medium");
    }
}

pub fn set_keep_awake(mode: u8) {
    unsafe {
        match mode {
            1 => {
                windows_sys::Win32::System::Power::SetThreadExecutionState(
                    windows_sys::Win32::System::Power::ES_CONTINUOUS
                        | windows_sys::Win32::System::Power::ES_SYSTEM_REQUIRED,
                );
            }
            2 => {
                windows_sys::Win32::System::Power::SetThreadExecutionState(
                    windows_sys::Win32::System::Power::ES_CONTINUOUS
                        | windows_sys::Win32::System::Power::ES_SYSTEM_REQUIRED
                        | windows_sys::Win32::System::Power::ES_DISPLAY_REQUIRED,
                );
            }
            _ => {
                windows_sys::Win32::System::Power::SetThreadExecutionState(
                    windows_sys::Win32::System::Power::ES_CONTINUOUS,
                );
            }
        }
    }

    // Ghi vĩnh viễn vào Windows Power Timeout: Tắt app Windows vẫn không sleep!
    if mode == 2 {
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", "0"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
    } else if mode == 1 {
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", "0"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", "15"]).creation_flags(CREATE_NO_WINDOW).status();
    } else {
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", "30"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", "15"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", "15"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("powercfg").args(["/change", "monitor-timeout-dc", "5"]).creation_flags(CREATE_NO_WINDOW).status();
    }
}

pub fn apply_permanent_windows_system_tweaks() {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

    // 1. Permanent Foreground Priority Separation (38 = Max Quantum Boost for active apps)
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\PriorityControl",
        "Win32PrioritySeparation",
        38,
    );

    // 2. Remove Explorer Menu Show Delay
    crate::reg::reg_set_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        "MenuShowDelay",
        "0",
    );

    // 3. Remove Windows Startup Application Delay
    crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
        "StartupDelayInMSec",
        0,
    );
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
        "StartupDelayInMSec",
        0,
    );

    // 4. Fast App Shutdown & Hang Recovery
    crate::reg::reg_set_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        "AutoEndTasks",
        "1",
    );
    crate::reg::reg_set_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        "WaitToKillAppTimeout",
        "2000",
    );
    crate::reg::reg_set_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        "HungAppTimeout",
        "1000",
    );
    crate::reg::reg_set_string(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control",
        "WaitToKillServiceTimeout",
        "2000",
    );
}
