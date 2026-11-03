use crate::reg::{
    get_device_oem_vendor, reg_enum_subkeys, reg_key_exists, reg_read_string, reg_read_u32,
    OemVendor,
};
use std::path::Path;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BatterySupportKind {
    Supported,
    DriverMissing,
    Unsupported,
    NoBattery,
}

#[derive(Debug, Clone)]
pub struct BatterySupportInfo {
    pub kind: BatterySupportKind,
    pub oem_vendor: OemVendor,
    pub oem_name: String,
    pub backend_name: String,
    pub status_message: String,
}

impl Default for BatterySupportInfo {
    fn default() -> Self {
        Self {
            kind: BatterySupportKind::Unsupported,
            oem_vendor: OemVendor::Other,
            oem_name: "Unknown".to_string(),
            backend_name: "Chưa hỗ trợ".to_string(),
            status_message: "Thiết bị chưa được cấu hình kiểm soát mạch nạp.".to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// LENOVO DRIVER DISPATCHER
// ---------------------------------------------------------------------------

const LENOVO_PWRMGRV_WOW: &str = r"SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data";
const LENOVO_PWRMGRV_NATIVE: &str = r"SOFTWARE\Lenovo\PWRMGRV\ConfKeys\Data";
const LENOVO_ENERGY_MGMT: &[&str] = &[
    r"SOFTWARE\Lenovo\Lenovo Energy Management",
    r"SOFTWARE\WOW6432Node\Lenovo\Lenovo Energy Management",
    r"SOFTWARE\Lenovo\PWRMGR",
];

pub fn find_lenovo_battery_slots() -> Vec<(String, String)> {
    let mut slots = Vec::new();

    for base in [LENOVO_PWRMGRV_WOW, LENOVO_PWRMGRV_NATIVE] {
        if !reg_key_exists(HKEY_LOCAL_MACHINE, base) {
            continue;
        }
        let subkeys = reg_enum_subkeys(HKEY_LOCAL_MACHINE, base);
        for sub in &subkeys {
            let lower = sub.to_lowercase();
            // Bỏ qua các folder cấu hình không phải pack pin
            if lower == "extremebatterylife"
                || lower == "gadget"
                || lower == "issc"
                || lower == "log"
                || lower == "global"
            {
                continue;
            }

            let full_sub = format!(r"{}\{}", base, sub);

            // Nếu subkey chứa Barcode Number (vd Battery1, Battery2,...), lấy barcode đích
            if let Some(barcode) = reg_read_string(HKEY_LOCAL_MACHINE, &full_sub, "Barcode Number") {
                let trimmed = barcode.trim();
                if !trimmed.is_empty() {
                    slots.push((base.to_string(), trimmed.to_string()));
                }
            }

            let has_ctrl = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopControl").is_some();
            let has_pct = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopPercentage").is_some();

            if has_ctrl || has_pct || (!sub.starts_with("Battery") && sub.len() >= 6) {
                slots.push((base.to_string(), sub.clone()));
            }
        }
    }

    // Nếu không tìm thấy barcode riêng biệt, fallback dùng tên Battery1, Battery2
    if slots.is_empty() {
        for base in [LENOVO_PWRMGRV_WOW, LENOVO_PWRMGRV_NATIVE] {
            let subkeys = reg_enum_subkeys(HKEY_LOCAL_MACHINE, base);
            for sub in subkeys {
                if sub.starts_with("Battery") {
                    slots.push((base.to_string(), sub));
                }
            }
        }
    }

    slots.sort();
    slots.dedup();
    slots
}

fn check_lenovo_conservation_mode() -> Option<bool> {
    for path in LENOVO_ENERGY_MGMT {
        if let Some(val) = reg_read_u32(HKEY_LOCAL_MACHINE, path, "ConservationMode") {
            return Some(val == 1);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// ASUS DRIVER DISPATCHER
// ---------------------------------------------------------------------------

const ASUS_BATTERY_HEALTH_KEYS: &[&str] = &[
    r"SOFTWARE\ASUS\ASUS Battery Health Charging",
    r"SOFTWARE\WOW6432Node\ASUS\ASUS Battery Health Charging",
    r"SOFTWARE\ASUSTeK COMPUTER INC.\ASUS Battery Health Charging",
    r"SOFTWARE\WOW6432Node\ASUSTeK COMPUTER INC.\ASUS Battery Health Charging",
];

fn find_asus_charging_key() -> Option<&'static str> {
    for k in ASUS_BATTERY_HEALTH_KEYS {
        if reg_key_exists(HKEY_LOCAL_MACHINE, k) {
            return Some(*k);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// DELL DRIVER DISPATCHER
// ---------------------------------------------------------------------------

fn find_dell_cctk_path() -> Option<&'static str> {
    let paths = [
        r"C:\Program Files\Dell\Command Configure\X86_64\cctk.exe",
        r"C:\Program Files (x86)\Dell\Command Configure\X86_64\cctk.exe",
        r"C:\Program Files\Dell\Command Configure\cctk.exe",
        r"C:\Program Files (x86)\Dell\Command Configure\cctk.exe",
    ];
    for p in paths {
        if Path::new(p).exists() {
            return Some(p);
        }
    }
    None
}

// ---------------------------------------------------------------------------
// HP DRIVER DISPATCHER
// ---------------------------------------------------------------------------

fn check_hp_wmi_available() -> bool {
    reg_key_exists(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Hewlett-Packard\HP Power Manager",
    ) || reg_key_exists(HKEY_LOCAL_MACHINE, r"SOFTWARE\HP\HP Power Manager")
}

// ---------------------------------------------------------------------------
// SUPPORT PROBING & STATUS
// ---------------------------------------------------------------------------

pub fn detect_battery_limiter_support() -> BatterySupportInfo {
    let power_status = crate::power::get_power_status();
    let (vendor, oem_raw) = get_device_oem_vendor();

    if !power_status.has_battery {
        return BatterySupportInfo {
            kind: BatterySupportKind::NoBattery,
            oem_vendor: vendor,
            oem_name: oem_raw,
            backend_name: "Nguồn AC Desktop".to_string(),
            status_message: "Thiết bị không có pin (Desktop PC / Nguồn trực tiếp).".to_string(),
        };
    }

    match vendor {
        OemVendor::Lenovo => {
            let slots = find_lenovo_battery_slots();
            let conservation_exists = LENOVO_ENERGY_MGMT
                .iter()
                .any(|p| reg_key_exists(HKEY_LOCAL_MACHINE, p));

            if !slots.is_empty() {
                let slot_names: Vec<String> = slots.iter().map(|(_, s)| s.clone()).collect();
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: format!("Lenovo Vantage ({})", slot_names.join(", ")),
                    status_message: format!(
                        "Đã kết nối mạch nạp Lenovo ThinkPad/IdeaPad ({} pin).",
                        slots.len()
                    ),
                }
            } else if conservation_exists {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Lenovo Conservation Mode".to_string(),
                    status_message: "Đã kết nối chế độ bảo vệ pin Lenovo Energy Management.".to_string(),
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Lenovo Vantage / PM".to_string(),
                    status_message: "Cần cài Lenovo Vantage hoặc Lenovo Power Management Driver từ trang hỗ trợ Lenovo.".to_string(),
                }
            }
        }
        OemVendor::Asus => {
            if let Some(key) = find_asus_charging_key() {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "ASUS Battery Health".to_string(),
                    status_message: format!("Đã kết nối trình điều khiển ASUS ({key})."),
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "ASUS Battery Health".to_string(),
                    status_message: "Cần cài đặt ứng dụng ASUS Battery Health Charging hoặc MyASUS.".to_string(),
                }
            }
        }
        OemVendor::Dell => {
            if let Some(cctk) = find_dell_cctk_path() {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Dell Command | Configure".to_string(),
                    status_message: format!("Đã kết nối công cụ Dell CCTK ({cctk})."),
                }
            } else if reg_key_exists(HKEY_LOCAL_MACHINE, r"SOFTWARE\Dell\CommandPowerManager")
                || reg_key_exists(HKEY_LOCAL_MACHINE, r"SOFTWARE\Dell\Dell Power Manager")
            {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Dell Power Manager".to_string(),
                    status_message: "Đã kết nối dịch vụ Dell Power Manager.".to_string(),
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Dell Command / Power".to_string(),
                    status_message: "Cần cài Dell Command | Configure hoặc Dell Power Manager.".to_string(),
                }
            }
        }
        OemVendor::Hp => {
            if check_hp_wmi_available() {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "HP Power Manager".to_string(),
                    status_message: "Đã kết nối dịch vụ quản lý nguồn HP.".to_string(),
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "HP Battery Care".to_string(),
                    status_message: "Cần kích hoạt 'Battery Care Function' trong BIOS (F10) hoặc cài HP Power Manager.".to_string(),
                }
            }
        }
        _ => BatterySupportInfo {
            kind: BatterySupportKind::Unsupported,
            oem_vendor: vendor,
            oem_name: oem_raw.clone(),
            backend_name: "Chưa hỗ trợ BIOS/EC".to_string(),
            status_message: format!("Bo mạch {oem_raw} chưa có API can thiệp mạch nạp trực tiếp qua phần mềm."),
        },
    }
}

// ---------------------------------------------------------------------------
// THRESHOLD GETTER & SETTER
// ---------------------------------------------------------------------------

pub fn get_battery_threshold(info: &BatterySupportInfo) -> (bool, u32) {
    if info.kind != BatterySupportKind::Supported {
        return (false, 80);
    }

    match info.oem_vendor {
        OemVendor::Lenovo => {
            let slots = find_lenovo_battery_slots();
            for (base, sub) in &slots {
                let full_sub = format!(r"{}\{}", base, sub);
                let enabled = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopControl")
                    .map(|v| v == 1)
                    .unwrap_or(false);
                let pct = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopPercentage")
                    .unwrap_or(80);
                if enabled {
                    return (true, pct);
                }
            }

            if let Some(active) = check_lenovo_conservation_mode() {
                if active {
                    return (true, 80);
                }
            }

            (false, 80)
        }
        OemVendor::Asus => {
            if let Some(k) = find_asus_charging_key() {
                if let Some(mode) = reg_read_u32(HKEY_LOCAL_MACHINE, k, "ChargingMode") {
                    // Mode 2: Balanced (80%), Mode 3: Max Lifespan (60%)
                    if mode == 2 {
                        return (true, 80);
                    } else if mode == 3 {
                        return (true, 60);
                    }
                }
            }
            (false, 80)
        }
        _ => (false, 80),
    }
}

pub fn set_battery_threshold(enable: bool, stop_pct: u32) -> Result<String, String> {
    let support = detect_battery_limiter_support();

    if support.kind == BatterySupportKind::NoBattery {
        return Err("Thiết bị không có pin (Desktop PC).".to_string());
    }
    if support.kind == BatterySupportKind::DriverMissing {
        return Err(support.status_message);
    }
    if support.kind == BatterySupportKind::Unsupported {
        return Err(format!(
            "Bo mạch {} chưa hỗ trợ can thiệp mạch nạp BIOS/EC.",
            support.oem_name
        ));
    }

    let start_pct = stop_pct.saturating_sub(5).max(40);
    let ctrl_val = if enable { 1 } else { 0 };

    match support.oem_vendor {
        OemVendor::Lenovo => {
            let slots = find_lenovo_battery_slots();
            let mut script_parts = Vec::new();
            script_parts.push("$ErrorActionPreference = 'Stop'".to_string());

            for (base, sub) in &slots {
                let ps_path = if base.contains("WOW6432Node") {
                    format!(r"HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\{sub}")
                } else {
                    format!(r"HKLM:\SOFTWARE\Lenovo\PWRMGRV\ConfKeys\Data\{sub}")
                };

                script_parts.push(format!(
                    r#"if (-not (Test-Path '{ps_path}')) {{ New-Item -Path '{ps_path}' -Force | Out-Null }}; Set-ItemProperty -Path '{ps_path}' -Name ChargeStopControl -Value {ctrl_val}; Set-ItemProperty -Path '{ps_path}' -Name ChargeStopPercentage -Value {stop_pct}; Set-ItemProperty -Path '{ps_path}' -Name ChargeStartControl -Value {ctrl_val}; Set-ItemProperty -Path '{ps_path}' -Name ChargeStartPercentage -Value {start_pct}"#
                ));
            }

            for em in LENOVO_ENERGY_MGMT {
                let ps_em = format!(r"HKLM:\{em}");
                script_parts.push(format!(
                    r#"if (Test-Path '{ps_em}') {{ Set-ItemProperty -Path '{ps_em}' -Name ConservationMode -Value {ctrl_val} }}"#
                ));
            }

            if script_parts.len() <= 1 {
                return Err("Không tìm thấy đường dẫn cấu hình pin Lenovo.".to_string());
            }

            let combined_script = script_parts.join("; ");
            match crate::protection::run_powershell_elevated(&combined_script) {
                Ok(0) => Ok(format!(
                    "Đã áp dụng giới hạn sạc {}% cho {} pack pin Lenovo.",
                    stop_pct,
                    slots.len()
                )),
                Ok(1223) => Err("Người dùng từ chối cấp quyền Quản trị viên (UAC).".to_string()),
                Ok(code) => Err(format!("Lỗi khi thiết lập pin Lenovo (Mã lỗi: {code}).")),
                Err(e) => Err(format!("Lỗi thực thi lệnh Lenovo: {e}")),
            }
        }
        OemVendor::Asus => {
            let key = find_asus_charging_key().unwrap_or(ASUS_BATTERY_HEALTH_KEYS[0]);
            let ps_key = format!(r"HKLM:\{key}");
            let mode_val = if enable { 2 } else { 1 }; // 2 = 80%, 1 = 100%
            let asus_target_pct = if enable { stop_pct } else { 100 };

            let script = format!(
                r#"$ErrorActionPreference = 'Stop'; if (-not (Test-Path '{ps_key}')) {{ New-Item -Path '{ps_key}' -Force | Out-Null }}; Set-ItemProperty -Path '{ps_key}' -Name ChargingMode -Value {mode_val}; try {{ (Get-CimInstance -Namespace root\wmi -ClassName AsusAtkWmi_WMNB -ErrorAction Stop).DEVS(0x00120057, {asus_target_pct}) }} catch {{}}"#
            );
            match crate::protection::run_powershell_elevated(&script) {
                Ok(0) => Ok(format!("Đã cập nhật chế độ sạc ASUS Battery Health thành {}%.", if enable { 80 } else { 100 })),
                Ok(1223) => Err("Người dùng từ chối cấp quyền Quản trị viên (UAC).".to_string()),
                Ok(code) => Err(format!("Lỗi khi thiết lập pin ASUS (Mã lỗi: {code}).")),
                Err(e) => Err(format!("Lỗi thực thi lệnh ASUS: {e}")),
            }
        }
        OemVendor::Dell => {
            if let Some(cctk) = find_dell_cctk_path() {
                let arg = if enable {
                    format!("--PrimaryBattChargeCfg=Custom:50-{stop_pct}")
                } else {
                    "--PrimaryBattChargeCfg=Standard".to_string()
                };

                let script = format!(
                    r#"& '{cctk}' "{arg}"; exit $LASTEXITCODE"#
                );
                match crate::protection::run_powershell_elevated(&script) {
                    Ok(0) => Ok(format!("Đã cấu hình ngưỡng sạc Dell CCTK thành {stop_pct}%.")),
                    Ok(1223) => Err("Người dùng từ chối cấp quyền Quản trị viên (UAC).".to_string()),
                    Ok(code) => Err(format!("Lỗi khi thiết lập Dell CCTK (Mã lỗi: {code}).")),
                    Err(e) => Err(format!("Lỗi thực thi lệnh Dell: {e}")),
                }
            } else {
                Err("Chưa cài đặt Dell Command | Configure.".to_string())
            }
        }
        OemVendor::Hp => {
            let setting_val = if enable { "80%" } else { "100%" };
            let script = format!(
                r#"$ErrorActionPreference = 'Stop'; $svc = Get-CimInstance -Namespace root/hp/instrumentedBIOS -ClassName HP_BIOSSettingInterface -ErrorAction Stop; $svc.SetBIOSSetting('Battery Care Function', '{setting_val}')"#
            );
            match crate::protection::run_powershell_elevated(&script) {
                Ok(0) => Ok("Đã thiết lập HP Battery Care Function.".to_string()),
                Ok(1223) => Err("Người dùng từ chối cấp quyền Quản trị viên (UAC).".to_string()),
                Ok(code) => Err(format!("Lỗi khi can thiệp HP BIOS Setting (Mã lỗi: {code}).")),
                Err(e) => Err(format!("Lỗi thực thi lệnh HP: {e}")),
            }
        }
        _ => Err("Bo mạch chưa được hỗ trợ.".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_oem_detection() {
        let (vendor, raw) = get_device_oem_vendor();
        println!("Detected OEM vendor: {:?}, raw: {}", vendor, raw);
        assert!(!raw.is_empty());
    }

    #[test]
    fn test_battery_limiter_probing() {
        let support = detect_battery_limiter_support();
        println!("Support info: {:?}", support);
        assert!(!support.oem_name.is_empty());
    }
}
