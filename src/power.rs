use std::os::windows::process::CommandExt;
use std::process::Command;
use std::sync::Mutex;
use crate::config::PowerTimeouts;
use windows_sys::Win32::System::Power::{
    GetSystemPowerStatus, SYSTEM_POWER_STATUS,
    PowerGetActiveScheme, PowerSetActiveScheme,
    PowerWriteACValueIndex, PowerWriteDCValueIndex,
    PowerReadACValueIndex, PowerReadDCValueIndex,
};
use windows_sys::Win32::Foundation::LocalFree;
use windows_sys::core::GUID;

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
    if cfg!(test) {
        return true;
    }
    if let Some(guid) = resolve_scheme_guid(scheme_alias_or_guid) {
        unsafe {
            if PowerSetActiveScheme(std::ptr::null_mut(), &guid) == 0 {
                return true;
            }
        }
    }
    Command::new("powercfg")
        .args(["/setactive", scheme_alias_or_guid])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_cpu_max_state_dc(percent: u32) -> bool {
    let pct = percent.clamp(50, 100);
    apply_subprocessor_settings(&[(&GUID_PROCTHROTTLEMAX, "PROCTHROTTLEMAX", None, Some(pct))])
}

pub fn set_cpu_max_state_ac(percent: u32) -> bool {
    let pct = percent.clamp(50, 100);
    apply_subprocessor_settings(&[(&GUID_PROCTHROTTLEMAX, "PROCTHROTTLEMAX", Some(pct), None)])
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
            "(Get-WmiObject -Namespace root/wmi -Class WmiMonitorBrightness -ErrorAction SilentlyContinue).CurrentBrightness",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(val) = text.parse::<u32>() {
            return val.clamp(10, 100);
        }
    }
    100
}

const POWER_SCHEMES_ROOT: &str = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";
pub const SUB_PROCESSOR_GUID: &str = "54533251-82be-4824-96c1-47b60b740d00";
pub const THROTTLE_MAX_GUID: &str = "bc5038f7-23e0-4960-96da-33abaf5935ec";
#[allow(dead_code)]
pub const THROTTLE_MIN_GUID: &str = "893dee8e-2bef-41e0-89c6-b55d0929964c";
pub const COOLING_POLICY_GUID: &str = "94d3a615-a899-4ac5-ae2b-e4d8f634367f";
pub const PERFEPP_GUID: &str = "36687f9e-e3a5-4dbf-b1dc-15eb381c6863";
pub const PERFBOOSTMODE_GUID: &str = "be337238-0d82-4146-a960-4f3749d470c7";
pub const CPMINCORES_GUID: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
#[allow(dead_code)]
pub const SCHEDPOLICY_GUID: &str = "93b8b6dc-0698-4d1c-9ee4-0644e900c85d";

pub const GUID_SUB_PROCESSOR: GUID = GUID {
    data1: 0x54533251,
    data2: 0x82be,
    data3: 0x4824,
    data4: [0x96, 0xc1, 0x47, 0xb6, 0x0b, 0x74, 0x0d, 0x00],
};
pub const GUID_PROCTHROTTLEMAX: GUID = GUID {
    data1: 0xbc5038f7,
    data2: 0x23e0,
    data3: 0x4960,
    data4: [0x96, 0xda, 0x33, 0xab, 0xaf, 0x59, 0x35, 0xec],
};
pub const GUID_PROCTHROTTLEMIN: GUID = GUID {
    data1: 0x893dee8e,
    data2: 0x2bef,
    data3: 0x41e0,
    data4: [0x89, 0xc6, 0xb5, 0x5d, 0x09, 0x29, 0x96, 0x4c],
};
pub const GUID_SYSCOOLPOL: GUID = GUID {
    data1: 0x94d3a615,
    data2: 0xa899,
    data3: 0x4ac5,
    data4: [0xae, 0x2b, 0xe4, 0xd8, 0xf6, 0x34, 0x36, 0x7f],
};
pub const GUID_PERFEPP: GUID = GUID {
    data1: 0x36687f9e,
    data2: 0xe3a5,
    data3: 0x4dbf,
    data4: [0xb1, 0xdc, 0x15, 0xeb, 0x38, 0x1c, 0x68, 0x63],
};
pub const GUID_PERFBOOSTMODE: GUID = GUID {
    data1: 0xbe337238,
    data2: 0x0d82,
    data3: 0x4146,
    data4: [0xa9, 0x60, 0x4f, 0x37, 0x49, 0xd4, 0x70, 0xc7],
};
pub const GUID_CPMINCORES: GUID = GUID {
    data1: 0x0cc5b647,
    data2: 0xc1df,
    data3: 0x4637,
    data4: [0x89, 0x1a, 0xde, 0xc3, 0x5c, 0x31, 0x85, 0x83],
};
pub const GUID_SCHEDPOLICY: GUID = GUID {
    data1: 0x93b8b6dc,
    data2: 0x0698,
    data3: 0x4d1c,
    data4: [0x9e, 0xe4, 0x06, 0x44, 0xe9, 0x00, 0xc8, 0x5d],
};
pub const GUID_BALANCED: GUID = GUID {
    data1: 0x381b4222,
    data2: 0xf694,
    data3: 0x41f0,
    data4: [0x96, 0x85, 0xff, 0x5b, 0xb2, 0x60, 0xdf, 0x2e],
};
pub const GUID_HIGH_PERF: GUID = GUID {
    data1: 0x8c5e7fda,
    data2: 0xe8bf,
    data3: 0x4a96,
    data4: [0x9a, 0x85, 0xa6, 0xe2, 0x3a, 0x8c, 0x63, 0x5c],
};
pub const GUID_POWER_SAVER: GUID = GUID {
    data1: 0xa1841308,
    data2: 0x3541,
    data3: 0x4fab,
    data4: [0xbc, 0x81, 0xf7, 0x15, 0x56, 0xf2, 0x0b, 0x4a],
};

#[inline]
#[allow(dead_code)]
pub fn guid_eq(a: &GUID, b: &GUID) -> bool {
    a.data1 == b.data1 && a.data2 == b.data2 && a.data3 == b.data3 && a.data4 == b.data4
}

pub fn parse_guid_str(s: &str) -> Option<GUID> {
    let clean = s.trim().trim_matches('{').trim_matches('}');
    let parts: Vec<&str> = clean.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let data1 = u32::from_str_radix(parts[0], 16).ok()?;
    let data2 = u16::from_str_radix(parts[1], 16).ok()?;
    let data3 = u16::from_str_radix(parts[2], 16).ok()?;
    let part4 = parts[3];
    let part5 = parts[4];
    if part4.len() != 4 || part5.len() != 12 {
        return None;
    }
    let b0 = u8::from_str_radix(&part4[0..2], 16).ok()?;
    let b1 = u8::from_str_radix(&part4[2..4], 16).ok()?;
    let b2 = u8::from_str_radix(&part5[0..2], 16).ok()?;
    let b3 = u8::from_str_radix(&part5[2..4], 16).ok()?;
    let b4 = u8::from_str_radix(&part5[4..6], 16).ok()?;
    let b5 = u8::from_str_radix(&part5[6..8], 16).ok()?;
    let b6 = u8::from_str_radix(&part5[8..10], 16).ok()?;
    let b7 = u8::from_str_radix(&part5[10..12], 16).ok()?;

    Some(GUID {
        data1,
        data2,
        data3,
        data4: [b0, b1, b2, b3, b4, b5, b6, b7],
    })
}

pub fn resolve_scheme_guid(alias_or_guid: &str) -> Option<GUID> {
    match alias_or_guid.to_uppercase().as_str() {
        "SCHEME_BALANCED" | "BALANCED" => Some(GUID_BALANCED),
        "SCHEME_MIN" | "HIGH_PERFORMANCE" | "HIGH" => Some(GUID_HIGH_PERF),
        "SCHEME_MAX" | "POWER_SAVER" | "SAVER" => Some(GUID_POWER_SAVER),
        _ => parse_guid_str(alias_or_guid),
    }
}

pub fn get_active_scheme_guid_native() -> Option<GUID> {
    unsafe {
        let mut p_guid: *mut GUID = std::ptr::null_mut();
        if PowerGetActiveScheme(std::ptr::null_mut(), &mut p_guid) == 0 && !p_guid.is_null() {
            let guid = *p_guid;
            LocalFree(p_guid as _);
            return Some(guid);
        }
    }
    if let Some(guid_str) = get_active_power_scheme_guid() {
        if let Some(guid) = parse_guid_str(&guid_str) {
            return Some(guid);
        }
    }
    Some(GUID_BALANCED)
}

pub fn read_subprocessor_value_index_native(setting_guid: &GUID) -> (Option<u32>, Option<u32>) {
    let mut ac_val = None;
    let mut dc_val = None;
    if let Some(active_guid) = get_active_scheme_guid_native() {
        unsafe {
            let mut val = 0u32;
            if PowerReadACValueIndex(
                std::ptr::null_mut(),
                &active_guid,
                &GUID_SUB_PROCESSOR,
                setting_guid,
                &mut val,
            ) == 0 {
                ac_val = Some(val);
            }
            if PowerReadDCValueIndex(
                std::ptr::null_mut(),
                &active_guid,
                &GUID_SUB_PROCESSOR,
                setting_guid,
                &mut val,
            ) == 0 {
                dc_val = Some(val);
            }
        }
    }
    (ac_val, dc_val)
}

/// Ghi trực tiếp cài đặt Sub-Processor qua Win32 powrprof.dll và kích hoạt scheme.
/// Trả về true nếu toàn bộ batch và lệnh kích hoạt thành công (mã lỗi 0).
pub fn apply_subprocessor_settings_native(
    active_guid: &GUID,
    settings: &[(&GUID, &str, Option<u32>, Option<u32>)],
) -> bool {
    let mut all_ok = true;
    unsafe {
        for (setting_guid, _, ac_opt, dc_opt) in settings {
            if let Some(ac) = ac_opt {
                let err = PowerWriteACValueIndex(
                    std::ptr::null_mut(),
                    active_guid,
                    &GUID_SUB_PROCESSOR,
                    *setting_guid,
                    *ac,
                );
                if err != 0 {
                    all_ok = false;
                }
            }
            if let Some(dc) = dc_opt {
                let err = PowerWriteDCValueIndex(
                    std::ptr::null_mut(),
                    active_guid,
                    &GUID_SUB_PROCESSOR,
                    *setting_guid,
                    *dc,
                );
                if err != 0 {
                    all_ok = false;
                }
            }
        }

        if all_ok {
            let act_err = PowerSetActiveScheme(std::ptr::null_mut(), active_guid);
            if act_err == 0 {
                return true;
            }
        }
    }
    false
}

/// Ghi cài đặt Sub-Processor qua trực tiếp Win32 powrprof.dll (0ms latency, zero process spawn).
/// Tự động kích hoạt PowerSetActiveScheme một lần duy nhất sau khi ghi toàn bộ batch.
/// Fallback an toàn sang powercfg (CREATE_NO_WINDOW) nếu Win32 API trả mã lỗi.
pub fn apply_subprocessor_settings(settings: &[(&GUID, &str, Option<u32>, Option<u32>)]) -> bool {
    if cfg!(test) {
        return true;
    }

    if settings.is_empty() {
        return true;
    }

    if let Some(active_guid) = get_active_scheme_guid_native() {
        if apply_subprocessor_settings_native(&active_guid, settings) {
            return true;
        }
    }

    // Fallback: Chạy powercfg hoàn toàn trong nền không hiện cửa sổ (CREATE_NO_WINDOW)
    for (_, alias, ac_opt, dc_opt) in settings {
        if let Some(ac) = ac_opt {
            let ac_str = ac.to_string();
            let _ = Command::new("powercfg")
                .args(["/setacvalueindex", "SCHEME_CURRENT", "SUB_PROCESSOR", alias, &ac_str])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        if let Some(dc) = dc_opt {
            let dc_str = dc.to_string();
            let _ = Command::new("powercfg")
                .args(["/setdcvalueindex", "SCHEME_CURRENT", "SUB_PROCESSOR", alias, &dc_str])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
    }
    let _ = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    true
}

pub fn get_active_power_scheme_guid() -> Option<String> {
    crate::reg::reg_read_string(
        windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        POWER_SCHEMES_ROOT,
        "ActivePowerScheme",
    )
}

fn get_power_setting_registry(subgroup: &str, setting: &str, value_name: &str) -> Option<u32> {
    let scheme_guid = get_active_power_scheme_guid()?;
    let key = format!(r"{}\{}\{}\{}", POWER_SCHEMES_ROOT, scheme_guid, subgroup, setting);
    crate::reg::reg_read_u32(
        windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        &key,
        value_name,
    )
}

pub fn extract_hex_value(line: &str) -> Option<u32> {
    let idx = line.find("0x")?;
    let hex_slice = &line[idx + 2..];
    let hex_token: String = hex_slice.chars().take_while(|c| c.is_ascii_hexdigit()).collect();
    if hex_token.is_empty() {
        None
    } else {
        u32::from_str_radix(&hex_token, 16).ok()
    }
}

pub fn parse_powercfg_output(output_text: &str) -> (Option<u32>, Option<u32>) {
    let mut ac_val = None;
    let mut dc_val = None;
    let mut all_hex = Vec::new();

    for line in output_text.lines() {
        if let Some(val) = extract_hex_value(line) {
            all_hex.push(val);
            let upper = line.to_uppercase();
            if upper.contains("DC") || upper.contains("CC") || upper.contains("GLEICHSTROM") || upper.contains("直流") {
                dc_val = Some(val);
            } else if upper.contains("AC") || upper.contains("CA") || upper.contains("WECHSELSTROM") || upper.contains("交流") {
                ac_val = Some(val);
            }
        }
    }

    if ac_val.is_none() && all_hex.len() >= 2 {
        ac_val = Some(all_hex[all_hex.len() - 2]);
    }
    if dc_val.is_none() && !all_hex.is_empty() {
        dc_val = Some(*all_hex.last().unwrap());
    }

    (ac_val, dc_val)
}

pub fn get_cpu_max_state_dc() -> u32 {
    if let Some(val) = get_power_setting_registry(SUB_PROCESSOR_GUID, THROTTLE_MAX_GUID, "DCSettingIndex") {
        return val.clamp(50, 100);
    }
    let (_, dc) = read_subprocessor_value_index_native(&GUID_PROCTHROTTLEMAX);
    if let Some(val) = dc {
        return val.clamp(50, 100);
    }

    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "PROCTHROTTLEMAX"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let (_, dc) = parse_powercfg_output(&text);
        if let Some(val) = dc {
            return val.clamp(50, 100);
        }
    }
    80
}

pub fn get_cpu_max_state_ac() -> u32 {
    if let Some(val) = get_power_setting_registry(SUB_PROCESSOR_GUID, THROTTLE_MAX_GUID, "ACSettingIndex") {
        return val.clamp(50, 100);
    }
    let (ac, _) = read_subprocessor_value_index_native(&GUID_PROCTHROTTLEMAX);
    if let Some(val) = ac {
        return val.clamp(50, 100);
    }

    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "PROCTHROTTLEMAX"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let (ac, _) = parse_powercfg_output(&text);
        if let Some(val) = ac {
            return val.clamp(50, 100);
        }
    }
    100
}

pub fn set_cooling_policy(active: bool) -> bool {
    let val = if active { 1 } else { 0 };
    apply_subprocessor_settings(&[(&GUID_SYSCOOLPOL, "SYSCOOLPOL", Some(val), Some(val))])
}

pub fn get_cooling_policy() -> bool {
    if let Some(val) = get_power_setting_registry(SUB_PROCESSOR_GUID, COOLING_POLICY_GUID, "DCSettingIndex") {
        return val == 1;
    }
    let (_, dc) = read_subprocessor_value_index_native(&GUID_SYSCOOLPOL);
    if let Some(val) = dc {
        return val == 1;
    }

    let output = Command::new("powercfg")
        .args(["/q", "SCHEME_CURRENT", "SUB_PROCESSOR", "SYSCOOLPOL"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        let (_, dc) = parse_powercfg_output(&text);
        if let Some(val) = dc {
            return val == 1;
        }
    }
    true
}

// ==================== CPU SUB_PROCESSOR DEEP TUNING ====================

pub fn apply_cpu_deep_tuning(
    epp: Option<u32>,
    boost_mode: Option<u32>,
    core_parking_min: Option<u32>,
    hetero_sched: Option<u32>,
) -> bool {
    let mut batch = Vec::new();
    if let Some(e) = epp {
        let val = e.clamp(0, 100);
        batch.push((&GUID_PERFEPP, "PERFEPP", Some(val), Some(val)));
    }
    if let Some(b) = boost_mode {
        let val = b.clamp(0, 6);
        batch.push((&GUID_PERFBOOSTMODE, "PERFBOOSTMODE", Some(val), Some(val)));
    }
    if let Some(cp) = core_parking_min {
        let val = cp.clamp(0, 100);
        batch.push((&GUID_CPMINCORES, "CPMINCORES", Some(val), Some(val)));
    }
    if let Some(h) = hetero_sched {
        let val = h.clamp(0, 5);
        batch.push((&GUID_SCHEDPOLICY, "SCHEDPOLICY", Some(val), Some(val)));
    }

    if batch.is_empty() {
        return true;
    }
    apply_subprocessor_settings(&batch)
}

pub fn apply_cpu_deep_tuning_dc(
    dc_max_pct: u32,
    epp: u32,
    boost_mode: u32,
    core_parking_min: u32,
) -> bool {
    let dc_val = dc_max_pct.clamp(50, 100);
    let e_val = epp.clamp(0, 100);
    let b_val = boost_mode.clamp(0, 6);
    let cp_val = core_parking_min.clamp(0, 100);

    apply_subprocessor_settings(&[
        (&GUID_PROCTHROTTLEMAX, "PROCTHROTTLEMAX", None, Some(dc_val)),
        (&GUID_PERFEPP, "PERFEPP", None, Some(e_val)),
        (&GUID_PERFBOOSTMODE, "PERFBOOSTMODE", None, Some(b_val)),
        (&GUID_CPMINCORES, "CPMINCORES", None, Some(cp_val)),
    ])
}

#[allow(dead_code)]
pub fn set_cpu_epp_dc(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_PERFEPP, "PERFEPP", None, Some(p))])
}

#[allow(dead_code)]
pub fn set_cpu_epp_ac(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_PERFEPP, "PERFEPP", Some(p), None)])
}

#[allow(dead_code)]
pub fn set_cpu_epp(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_PERFEPP, "PERFEPP", Some(p), Some(p))])
}

#[allow(dead_code)]
pub fn get_cpu_epp() -> (Option<u32>, Option<u32>) {
    let ac = get_power_setting_registry(SUB_PROCESSOR_GUID, PERFEPP_GUID, "ACSettingIndex");
    let dc = get_power_setting_registry(SUB_PROCESSOR_GUID, PERFEPP_GUID, "DCSettingIndex");
    if ac.is_some() || dc.is_some() {
        return (ac, dc);
    }
    let (ac_nat, dc_nat) = read_subprocessor_value_index_native(&GUID_PERFEPP);
    if ac_nat.is_some() || dc_nat.is_some() {
        return (ac_nat, dc_nat);
    }
    let output = Command::new("powercfg")
        .args(["/qh", "SCHEME_CURRENT", "SUB_PROCESSOR", "PERFEPP"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        return parse_powercfg_output(&text);
    }
    (None, None)
}

#[allow(dead_code)]
pub fn set_cpu_boost_mode_dc(mode: u32) -> bool {
    let m = mode.clamp(0, 6);
    apply_subprocessor_settings(&[(&GUID_PERFBOOSTMODE, "PERFBOOSTMODE", None, Some(m))])
}

#[allow(dead_code)]
pub fn set_cpu_boost_mode_ac(mode: u32) -> bool {
    let m = mode.clamp(0, 6);
    apply_subprocessor_settings(&[(&GUID_PERFBOOSTMODE, "PERFBOOSTMODE", Some(m), None)])
}

#[allow(dead_code)]
pub fn set_cpu_boost_mode(mode: u32) -> bool {
    let m = mode.clamp(0, 6);
    apply_subprocessor_settings(&[(&GUID_PERFBOOSTMODE, "PERFBOOSTMODE", Some(m), Some(m))])
}

#[allow(dead_code)]
pub fn get_cpu_boost_mode() -> (Option<u32>, Option<u32>) {
    let ac = get_power_setting_registry(SUB_PROCESSOR_GUID, PERFBOOSTMODE_GUID, "ACSettingIndex");
    let dc = get_power_setting_registry(SUB_PROCESSOR_GUID, PERFBOOSTMODE_GUID, "DCSettingIndex");
    if ac.is_some() || dc.is_some() {
        return (ac, dc);
    }
    let (ac_nat, dc_nat) = read_subprocessor_value_index_native(&GUID_PERFBOOSTMODE);
    if ac_nat.is_some() || dc_nat.is_some() {
        return (ac_nat, dc_nat);
    }
    let output = Command::new("powercfg")
        .args(["/qh", "SCHEME_CURRENT", "SUB_PROCESSOR", "PERFBOOSTMODE"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        return parse_powercfg_output(&text);
    }
    (None, None)
}

#[allow(dead_code)]
pub fn set_cpu_core_parking_min_dc(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_CPMINCORES, "CPMINCORES", None, Some(p))])
}

#[allow(dead_code)]
pub fn set_cpu_core_parking_min_ac(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_CPMINCORES, "CPMINCORES", Some(p), None)])
}

#[allow(dead_code)]
pub fn set_cpu_core_parking_min(percent: u32) -> bool {
    let p = percent.clamp(0, 100);
    apply_subprocessor_settings(&[(&GUID_CPMINCORES, "CPMINCORES", Some(p), Some(p))])
}

#[allow(dead_code)]
pub fn get_cpu_core_parking_min() -> (Option<u32>, Option<u32>) {
    let ac = get_power_setting_registry(SUB_PROCESSOR_GUID, CPMINCORES_GUID, "ACSettingIndex");
    let dc = get_power_setting_registry(SUB_PROCESSOR_GUID, CPMINCORES_GUID, "DCSettingIndex");
    if ac.is_some() || dc.is_some() {
        return (ac, dc);
    }
    let (ac_nat, dc_nat) = read_subprocessor_value_index_native(&GUID_CPMINCORES);
    if ac_nat.is_some() || dc_nat.is_some() {
        return (ac_nat, dc_nat);
    }
    let output = Command::new("powercfg")
        .args(["/qh", "SCHEME_CURRENT", "SUB_PROCESSOR", "CPMINCORES"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();
    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        return parse_powercfg_output(&text);
    }
    (None, None)
}

#[allow(dead_code)]
pub fn set_hetero_sched_policy(policy: u32) -> bool {
    let p = policy.clamp(0, 5);
    apply_subprocessor_settings(&[(&GUID_SCHEDPOLICY, "SCHEDPOLICY", Some(p), Some(p))])
}

pub fn set_win32_priority_separation_val(val: u32) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\PriorityControl",
        "Win32PrioritySeparation",
        val,
    )
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
        let _ = Command::new("bcdedit").args(["/deletevalue", "useplatformtick"]).creation_flags(CREATE_NO_WINDOW).status();
    } else {
        let _ = Command::new("bcdedit").args(["/deletevalue", "disabledynamictick"]).creation_flags(CREATE_NO_WINDOW).status();
        let _ = Command::new("bcdedit").args(["/deletevalue", "useplatformtick"]).creation_flags(CREATE_NO_WINDOW).status();
    }

    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let tasks_games_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";

    if enable {
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "GPU Priority", 8);
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 6);
        crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "High");
    } else {
        crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 2);
        crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "Medium");
    }
}

static SAVED_TIMEOUTS: Mutex<Option<PowerTimeouts>> = Mutex::new(None);

pub fn init_keep_awake_timeouts(saved: Option<PowerTimeouts>) {
    if let Ok(mut lock) = SAVED_TIMEOUTS.lock() {
        if saved.is_some() {
            *lock = saved;
        }
    }
}

pub fn get_saved_timeouts() -> Option<PowerTimeouts> {
    if let Ok(lock) = SAVED_TIMEOUTS.lock() {
        *lock
    } else {
        None
    }
}

pub fn read_current_power_timeouts() -> Option<PowerTimeouts> {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let schemes_key = r"SYSTEM\CurrentControlSet\Control\Power\User\PowerSchemes";
    let active_scheme = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, schemes_key, "ActivePowerScheme")?;

    let sleep_sub = format!(r"{}\{}\238c9fa8-0aad-41ed-83f4-97be242c8f20\29f6c1db-86da-48c5-9fdb-f2b67b1f44da", schemes_key, active_scheme);
    let video_sub = format!(r"{}\{}\7516b95f-f776-4464-8c53-06167f40cc99\3c0bc021-c8a8-4e07-a973-6b14cbcb2b7e", schemes_key, active_scheme);

    let standby_ac = crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, &sleep_sub, "ACSettingIndex").unwrap_or(1800) / 60;
    let standby_dc = crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, &sleep_sub, "DCSettingIndex").unwrap_or(900) / 60;
    let monitor_ac = crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, &video_sub, "ACSettingIndex").unwrap_or(900) / 60;
    let monitor_dc = crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, &video_sub, "DCSettingIndex").unwrap_or(300) / 60;

    Some(PowerTimeouts {
        standby_ac,
        standby_dc,
        monitor_ac,
        monitor_dc,
    })
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

    if mode == 1 || mode == 2 {
        if let Ok(mut lock) = SAVED_TIMEOUTS.lock() {
            if lock.is_none() {
                if let Some(current) = read_current_power_timeouts() {
                    *lock = Some(current);
                }
            }
        }

        if mode == 2 {
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", "0"]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
        } else {
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", "0"]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", "0"]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", "15"]).creation_flags(CREATE_NO_WINDOW).status();
        }
    } else {
        let orig = if let Ok(mut lock) = SAVED_TIMEOUTS.lock() {
            lock.take()
        } else {
            None
        };

        if let Some(orig) = orig {
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-ac", &orig.standby_ac.to_string()]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "standby-timeout-dc", &orig.standby_dc.to_string()]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "monitor-timeout-ac", &orig.monitor_ac.to_string()]).creation_flags(CREATE_NO_WINDOW).status();
            let _ = Command::new("powercfg").args(["/change", "monitor-timeout-dc", &orig.monitor_dc.to_string()]).creation_flags(CREATE_NO_WINDOW).status();
        }
    }
}

pub fn apply_permanent_windows_system_tweaks() {
    if cfg!(test) {
        return;
    }
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

    // 4. Cleanup potentially destructive bcdedit useplatformtick & useplatformclock
    let _ = Command::new("bcdedit")
        .args(["/deletevalue", "useplatformtick"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    let _ = Command::new("bcdedit")
        .args(["/deletevalue", "useplatformclock"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
}

pub fn set_tcp_optimization(enable: bool) -> bool {
    if cfg!(test) {
        return true;
    }
    let level = if enable { "autotuninglevel=normal" } else { "autotuninglevel=disabled" };
    Command::new("netsh")
        .args(["int", "tcp", "set", "global", level])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_sysmain_service(enable: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let start_val = if enable { 2 } else { 4 };
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Services\SysMain",
        "Start",
        start_val,
    );
    let action = if enable { "start" } else { "stop" };
    Command::new("sc")
        .args([action, "SysMain"])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_ntfs_last_access(disable: bool) -> bool {
    let val = if disable { "1" } else { "0" };
    Command::new("fsutil")
        .args(["behavior", "set", "disablelastaccess", val])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_cpu_min_state(percent: u32) -> bool {
    if cfg!(test) {
        return true;
    }
    let p = percent.clamp(1, 100);
    let _ = apply_subprocessor_settings(&[(&GUID_PROCTHROTTLEMIN, "PROCTHROTTLEMIN", Some(p), Some(p))]);

    let pct_str = p.to_string();
    let schemes = [
        "SCHEME_CURRENT",
        "381b4222-f694-41f0-9685-ff5bb260df2e",
        "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c",
        "a1841308-3541-4fab-bc81-f71556f20b4a",
    ];

    for s in schemes {
        let _ = Command::new("powercfg")
            .args(["/setdcvalueindex", s, "SUB_PROCESSOR", "PROCTHROTTLEMIN", &pct_str])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
        let _ = Command::new("powercfg")
            .args(["/setacvalueindex", s, "SUB_PROCESSOR", "PROCTHROTTLEMIN", &pct_str])
            .creation_flags(CREATE_NO_WINDOW)
            .status();
    }
    let _ = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    true
}

pub fn set_win32_priority_separation(boost: bool) -> bool {
    let val = if boost { 38 } else { 2 };
    set_win32_priority_separation_val(val)
}

pub fn set_fast_shutdown_recovery(enable: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    let auto_end = if enable { "1" } else { "0" };
    let hung_to = if enable { "1000" } else { "5000" };
    let wait_app = if enable { "2000" } else { "10000" };
    crate::reg::reg_set_string(HKEY_CURRENT_USER, r"Control Panel\Desktop", "AutoEndTasks", auto_end);
    crate::reg::reg_set_string(HKEY_CURRENT_USER, r"Control Panel\Desktop", "HungAppTimeout", hung_to);
    crate::reg::reg_set_string(HKEY_CURRENT_USER, r"Control Panel\Desktop", "WaitToKillAppTimeout", wait_app);
    true
}

pub fn set_large_system_cache(enable: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let val = if enable { 1 } else { 0 };
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Session Manager\Memory Management",
        "LargeSystemCache",
        val,
    )
}

pub fn set_memory_compression(enable: bool) -> bool {
    let cmd = if enable {
        "Enable-MMAgent -MemoryCompression; exit $LASTEXITCODE"
    } else {
        "Disable-MMAgent -MemoryCompression; exit $LASTEXITCODE"
    };
    crate::protection::run_powershell_elevated(cmd)
        .map(|code| code == 0)
        .unwrap_or(false)
}

pub fn set_ntfs_8dot3_name(disable: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let val = if disable { 1 } else { 0 };
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\FileSystem",
        "NtfsDisable8dot3NameCreation",
        val,
    );
    let opt = if disable { "1" } else { "0" };
    Command::new("fsutil")
        .args(["behavior", "set", "disable8dot3", opt])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_ssd_trim(enable: bool) -> bool {
    // 0 = DisableDeleteNotify is disabled (meaning TRIM is ENABLED)
    let val = if enable { "0" } else { "1" };
    Command::new("fsutil")
        .args(["behavior", "set", "DisableDeleteNotify", val])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_wait_to_kill_service(fast: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let val = if fast { "2000" } else { "5000" };
    crate::reg::reg_set_string(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control",
        "WaitToKillServiceTimeout",
        val,
    )
}

pub fn set_menu_show_delay(zero: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_CURRENT_USER;
    let val = if zero { "0" } else { "400" };
    crate::reg::reg_set_string(
        HKEY_CURRENT_USER,
        r"Control Panel\Desktop",
        "MenuShowDelay",
        val,
    )
}

pub fn set_startup_delay(zero: bool) -> bool {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let val = if zero { 0 } else { 10000 };
    crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
        "StartupDelayInMSec",
        val,
    );
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Explorer\Serialize",
        "StartupDelayInMSec",
        val,
    );
    true
}

pub fn set_tcp_ecn(enable: bool) -> bool {
    let state = if enable { "enabled" } else { "disabled" };
    Command::new("netsh")
        .args(["int", "tcp", "set", "global", &format!("ecncapability={}", state)])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn set_mmcss_gaming_responsiveness(enable: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let net_val = if enable { 0xFFFFFFFF } else { 10 };
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
        "NetworkThrottlingIndex",
        net_val,
    )
}

pub fn set_system_responsiveness(zero: bool) -> bool {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    let resp_val = if zero { 0 } else { 20 };
    crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
        "SystemResponsiveness",
        resp_val,
    )
}

#[allow(dead_code)]
pub fn get_system_responsiveness() -> Option<u32> {
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
    crate::reg::reg_read_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile",
        "SystemResponsiveness",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_hex_value() {
        assert_eq!(extract_hex_value("Current AC Power Setting Index: 0x00000064"), Some(100));
        assert_eq!(extract_hex_value("Current DC Power Setting Index: 0x00000050"), Some(80));
        assert_eq!(extract_hex_value("Some value 0x1"), Some(1));
        assert_eq!(extract_hex_value("No hex value here"), None);
    }

    #[test]
    fn test_parse_powercfg_output_multilingual() {
        // English
        let en_output = "Current AC Power Setting Index: 0x00000064\nCurrent DC Power Setting Index: 0x00000050";
        let (ac, dc) = parse_powercfg_output(en_output);
        assert_eq!(ac, Some(100));
        assert_eq!(dc, Some(80));

        // German
        let de_output = "Index der aktuellen Wechselstromeinstellung: 0x00000064\nIndex der aktuellen Gleichstromeinstellung: 0x00000050";
        let (ac, dc) = parse_powercfg_output(de_output);
        assert_eq!(ac, Some(100));
        assert_eq!(dc, Some(80));

        // French (CA / CC)
        let fr_output = "Index du paramètre d’alimentation CA actuel : 0x00000064\nIndex du paramètre d’alimentation CC actuel : 0x00000050";
        let (ac, dc) = parse_powercfg_output(fr_output);
        assert_eq!(ac, Some(100));
        assert_eq!(dc, Some(80));

        // Chinese
        let zh_output = "当前交流电源设置索引: 0x00000064\n当前直流电源设置索引: 0x00000050";
        let (ac, dc) = parse_powercfg_output(zh_output);
        assert_eq!(ac, Some(100));
        assert_eq!(dc, Some(80));
    }

    #[test]
    fn test_cpu_subprocessor_settings_read() {
        let (epp_ac, epp_dc) = get_cpu_epp();
        println!("EPP: AC={:?}, DC={:?}", epp_ac, epp_dc);
        let (bm_ac, bm_dc) = get_cpu_boost_mode();
        println!("Boost Mode: AC={:?}, DC={:?}", bm_ac, bm_dc);
        let (cp_ac, cp_dc) = get_cpu_core_parking_min();
        println!("Core Parking: AC={:?}, DC={:?}", cp_ac, cp_dc);
    }

    #[test]
    fn test_guid_parsing() {
        let sub_guid = parse_guid_str(SUB_PROCESSOR_GUID);
        assert!(sub_guid.is_some());
        let g = sub_guid.unwrap();
        assert_eq!(g.data1, 0x54533251);
        assert_eq!(g.data2, 0x82be);
        assert_eq!(g.data3, 0x4824);
        assert_eq!(g.data4, [0x96, 0xc1, 0x47, 0xb6, 0x0b, 0x74, 0x0d, 0x00]);

        // With braces
        let braced = format!("{{{}}}", SUB_PROCESSOR_GUID);
        let parsed_braced = parse_guid_str(&braced);
        assert!(parsed_braced.is_some() && guid_eq(&parsed_braced.unwrap(), &g));

        // Invalid strings
        assert!(parse_guid_str("invalid-guid").is_none());
        assert!(parse_guid_str("").is_none());
    }

    #[test]
    fn test_resolve_scheme_guid() {
        let b = resolve_scheme_guid("SCHEME_BALANCED");
        assert!(b.is_some() && guid_eq(&b.unwrap(), &GUID_BALANCED));

        let h = resolve_scheme_guid("SCHEME_MIN");
        assert!(h.is_some() && guid_eq(&h.unwrap(), &GUID_HIGH_PERF));

        let s = resolve_scheme_guid("SCHEME_MAX");
        assert!(s.is_some() && guid_eq(&s.unwrap(), &GUID_POWER_SAVER));

        let custom = resolve_scheme_guid("381b4222-f694-41f0-9685-ff5bb260df2e");
        assert!(custom.is_some() && guid_eq(&custom.unwrap(), &GUID_BALANCED));
    }

    #[test]
    fn test_apply_subprocessor_settings_test_safe() {
        // Must succeed safely without side effects in test mode
        assert!(apply_subprocessor_settings(&[]));
        assert!(apply_subprocessor_settings(&[(&GUID_PROCTHROTTLEMAX, "PROCTHROTTLEMAX", Some(100), Some(80))]));
        assert!(apply_cpu_deep_tuning_dc(80, 50, 0, 50));
        assert!(apply_cpu_deep_tuning(Some(33), Some(2), Some(100), Some(2)));
        assert!(set_cpu_max_state_dc(75));
        assert!(set_cpu_max_state_ac(100));
        assert!(set_cpu_min_state(5));
        assert!(set_cooling_policy(true));
        assert!(set_power_scheme("SCHEME_BALANCED"));
    }

    #[test]
    fn test_native_win32_power_write_roundtrip() {
        if let Some(active_guid) = get_active_scheme_guid_native() {
            let (ac_orig, dc_orig) = read_subprocessor_value_index_native(&GUID_PERFEPP);
            println!("Native Read EPP: AC={:?}, DC={:?}", ac_orig, dc_orig);
            if let (Some(ac), Some(dc)) = (ac_orig, dc_orig) {
                let success = apply_subprocessor_settings_native(
                    &active_guid,
                    &[(&GUID_PERFEPP, "PERFEPP", Some(ac), Some(dc))],
                );
                println!("apply_subprocessor_settings_native success: {}", success);
                assert!(success, "Native Win32 power setting write must succeed on active power scheme");
            }
        }
    }
}
