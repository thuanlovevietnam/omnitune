use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Default)]
pub struct PowerStatus {
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
            let is_ac = status.ACLineStatus == 1;
            let percent = if status.BatteryLifePercent <= 100 {
                status.BatteryLifePercent
            } else {
                0
            };
            let is_charging = (status.BatteryFlag & 8) != 0;
            let remaining = if status.BatteryLifeTime != u32::MAX && status.BatteryLifeTime > 0 {
                Some(status.BatteryLifeTime)
            } else {
                None
            };

            return PowerStatus {
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
    let res1 = Command::new("powercfg")
        .args([
            "/setdcvalueindex",
            "SCHEME_CURRENT",
            "SUB_PROCESSOR",
            "PROCTHROTTLEMAX",
            &pct_str,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let res2 = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    res1 && res2
}

pub fn set_cpu_max_state_ac(percent: u32) -> bool {
    let pct_str = percent.clamp(50, 100).to_string();
    let res1 = Command::new("powercfg")
        .args([
            "/setacvalueindex",
            "SCHEME_CURRENT",
            "SUB_PROCESSOR",
            "PROCTHROTTLEMAX",
            &pct_str,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    let res2 = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false);

    res1 && res2
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

pub fn get_brightness() -> u32 {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-CimInstance -Namespace root/wmi -ClassName WmiMonitorBrightness).CurrentBrightness",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(val) = text.parse::<u32>() {
            return val;
        }
    }
    50
}
