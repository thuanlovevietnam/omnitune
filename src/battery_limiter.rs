use crate::reg::{
    get_device_oem_vendor, reg_enum_subkeys, reg_key_exists, reg_read_string, reg_read_u32,
    OemVendor,
};
use std::os::windows::process::CommandExt;
use std::path::Path;
use std::process::Command;
use std::sync::OnceLock;
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

const CREATE_NO_WINDOW: u32 = 0x08000000;

fn run_ps_query(script: &str) -> String {
    Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-Command",
            script,
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

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
    pub download_url: Option<String>,
}

impl Default for BatterySupportInfo {
    fn default() -> Self {
        Self {
            kind: BatterySupportKind::Unsupported,
            oem_vendor: OemVendor::Other,
            oem_name: "Unknown".to_string(),
            backend_name: "Chưa hỗ trợ".to_string(),
            status_message: "Thiết bị chưa được cấu hình kiểm soát mạch nạp.".to_string(),
            download_url: None,
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

pub fn parse_dell_cctk_output(output: &str) -> (bool, u32) {
    let lower = output.to_lowercase();
    if let Some(idx) = lower.find("custom:") {
        let rest = &output[idx + "custom:".len()..];
        if let Some(dash_idx) = rest.find('-') {
            let num_str: String = rest[dash_idx + 1..]
                .chars()
                .take_while(|c| c.is_ascii_digit())
                .collect();
            if let Ok(pct) = num_str.parse::<u32>() {
                return (true, pct);
            }
        }
        (true, 80)
    } else {
        (false, 80)
    }
}

fn get_dell_battery_threshold(cctk: &str) -> (bool, u32) {
    let script = format!(r#"& '{cctk}' --PrimaryBattChargeCfg"#);
    let out = run_ps_query(&script);
    parse_dell_cctk_output(&out)
}

// ---------------------------------------------------------------------------
// HP DRIVER DISPATCHER
// ---------------------------------------------------------------------------

fn check_hp_wmi_available() -> bool {
    static HP_WMI_CACHE: OnceLock<bool> = OnceLock::new();
    *HP_WMI_CACHE.get_or_init(|| {
        if reg_key_exists(
            HKEY_LOCAL_MACHINE,
            r"SOFTWARE\Hewlett-Packard\HP Power Manager",
        ) || reg_key_exists(HKEY_LOCAL_MACHINE, r"SOFTWARE\HP\HP Power Manager")
            || reg_key_exists(
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Services\HpBiosSettingInterface",
            )
        {
            return true;
        }

        let script = r#"try {
    $c = Get-CimClass -Namespace root/hp/instrumentedBIOS -ClassName HP_BIOSSettingInterface -ErrorAction Stop;
    if ($c) { '1' }
} catch { '' }"#;
        run_ps_query(script) == "1"
    })
}

pub fn parse_hp_bios_setting_output(output: &str) -> (bool, u32) {
    let lower = output.to_lowercase();
    if lower.is_empty()
        || lower.contains("100%")
        || lower.contains("disabled")
        || lower.contains("standard")
    {
        (false, 80)
    } else if lower.contains("50%") || lower.contains("50") {
        (true, 50)
    } else if lower.contains("80%") || lower.contains("maximize") || lower.contains("80") {
        (true, 80)
    } else {
        (false, 80)
    }
}

pub fn get_hp_battery_threshold() -> (bool, u32) {
    let script = r#"try {
    $svc = Get-CimInstance -Namespace root/hp/instrumentedBIOS -ClassName HP_BIOSSettingInterface -ErrorAction SilentlyContinue
    if ($svc) {
        $res = Invoke-CimMethod -InputObject $svc -MethodName GetBIOSSetting -Arguments @{ Name = 'Battery Care Function' } -ErrorAction SilentlyContinue
        if ($res -and $res.ReturnValue -eq 0 -and $res.Value) {
            [Console]::WriteLine($res.Value.Trim()); exit 0
        }
        $res2 = Invoke-CimMethod -InputObject $svc -MethodName GetBIOSSetting -Arguments @{ Name = 'Battery Health Manager' } -ErrorAction SilentlyContinue
        if ($res2 -and $res2.ReturnValue -eq 0 -and $res2.Value) {
            [Console]::WriteLine($res2.Value.Trim()); exit 0
        }
    }
    $enumObj = Get-CimInstance -Namespace root/hp/instrumentedBIOS -ClassName HP_BIOSEnumerated -Filter "Name = 'Battery Care Function' or Name = 'Battery Health Manager'" -ErrorAction SilentlyContinue
    if ($enumObj -and $enumObj.Value) {
        [Console]::WriteLine($enumObj.Value.Trim()); exit 0
    }
} catch {}"#;

    let out = run_ps_query(script);
    parse_hp_bios_setting_output(&out)
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
            download_url: None,
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
                    download_url: None,
                }
            } else if conservation_exists {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Lenovo Conservation Mode".to_string(),
                    status_message: "Đã kết nối chế độ bảo vệ pin Lenovo Energy Management.".to_string(),
                    download_url: None,
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Lenovo Vantage / PM".to_string(),
                    status_message: "Cần cài Lenovo Vantage hoặc Lenovo Power Management Driver từ trang hỗ trợ Lenovo.".to_string(),
                    download_url: Some("https://support.lenovo.com".to_string()),
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
                    download_url: None,
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "ASUS Battery Health".to_string(),
                    status_message: "Cần cài đặt ứng dụng ASUS Battery Health Charging hoặc MyASUS.".to_string(),
                    download_url: Some("https://www.asus.com/support".to_string()),
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
                    download_url: None,
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "Dell Command | Configure".to_string(),
                    status_message: "Cần cài đặt Dell Command | Configure (cctk.exe) để điều khiển mạch nạp: https://www.dell.com/support/kbdoc/000178000/dell-command-configure".to_string(),
                    download_url: Some("https://www.dell.com/support/kbdoc/000178000/dell-command-configure".to_string()),
                }
            }
        }
        OemVendor::Hp => {
            if check_hp_wmi_available() {
                BatterySupportInfo {
                    kind: BatterySupportKind::Supported,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "HP BIOS / WMI".to_string(),
                    status_message: "Đã kết nối giao tiếp HP BIOS/WMI (HP_BIOSSettingInterface).".to_string(),
                    download_url: None,
                }
            } else {
                BatterySupportInfo {
                    kind: BatterySupportKind::DriverMissing,
                    oem_vendor: vendor,
                    oem_name: oem_raw,
                    backend_name: "HP Battery Care".to_string(),
                    status_message: "Cần kích hoạt 'Battery Care Function' trong BIOS (F10) hoặc cài HP Power Manager.".to_string(),
                    download_url: Some("https://support.hp.com".to_string()),
                }
            }
        }
        _ => BatterySupportInfo {
            kind: BatterySupportKind::Unsupported,
            oem_vendor: vendor,
            oem_name: oem_raw.clone(),
            backend_name: "Chưa hỗ trợ BIOS/EC".to_string(),
            status_message: format!("Bo mạch {oem_raw} chưa có API can thiệp mạch nạp trực tiếp qua phần mềm."),
            download_url: None,
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
        OemVendor::Dell => {
            if let Some(cctk) = find_dell_cctk_path() {
                get_dell_battery_threshold(cctk)
            } else {
                (false, 80)
            }
        }
        OemVendor::Hp => get_hp_battery_threshold(),
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
            script_parts.push("$success = 0".to_string());

            for (base, sub) in &slots {
                let ps_path = if base.contains("WOW6432Node") {
                    format!(r"HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\{sub}")
                } else {
                    format!(r"HKLM:\SOFTWARE\Lenovo\PWRMGRV\ConfKeys\Data\{sub}")
                };

                script_parts.push(format!(
                    r#"try {{ if (-not (Test-Path '{ps_path}')) {{ New-Item -Path '{ps_path}' -Force | Out-Null }}; Set-ItemProperty -Path '{ps_path}' -Name ChargeStopControl -Value {ctrl_val} -ErrorAction Stop; Set-ItemProperty -Path '{ps_path}' -Name ChargeStopPercentage -Value {stop_pct} -ErrorAction Stop; Set-ItemProperty -Path '{ps_path}' -Name ChargeStartControl -Value {ctrl_val} -ErrorAction Stop; Set-ItemProperty -Path '{ps_path}' -Name ChargeStartPercentage -Value {start_pct} -ErrorAction Stop; $success++ }} catch {{}}"#
                ));
            }

            for em in LENOVO_ENERGY_MGMT {
                let ps_em = format!(r"HKLM:\{em}");
                script_parts.push(format!(
                    r#"try {{ if (Test-Path '{ps_em}') {{ Set-ItemProperty -Path '{ps_em}' -Name ConservationMode -Value {ctrl_val} -ErrorAction Stop; $success++ }} }} catch {{}}"#
                ));
            }

            script_parts.push("if ($success -eq 0) { exit 10 } else { exit 0 }".to_string());

            if script_parts.len() <= 3 {
                return Err("Không tìm thấy đường dẫn cấu hình pin Lenovo.".to_string());
            }

            let combined_script = script_parts.join("; ");
            match crate::protection::run_powershell_elevated(&combined_script) {
                Ok(0) => {
                    let mut success_count = 0usize;
                    for (base, sub) in &slots {
                        let full_sub = format!(r"{base}\{sub}");
                        let ctrl = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopControl");
                        let pct = reg_read_u32(HKEY_LOCAL_MACHINE, &full_sub, "ChargeStopPercentage");
                        if ctrl == Some(ctrl_val) && (!enable || pct == Some(stop_pct)) {
                            success_count += 1;
                        }
                    }

                    if success_count == 0 {
                        for em in LENOVO_ENERGY_MGMT {
                            if reg_read_u32(HKEY_LOCAL_MACHINE, em, "ConservationMode") == Some(ctrl_val) {
                                success_count = 1;
                                break;
                            }
                        }
                    }

                    if success_count > 0 {
                        Ok(format!(
                            "Đã áp dụng giới hạn sạc {}% cho {} pack pin Lenovo.",
                            stop_pct,
                            success_count
                        ))
                    } else {
                        Err("Không có pack pin nào được cập nhật thành công (success_count = 0).".to_string())
                    }
                }
                Ok(10) => Err("Không có pack pin nào được cập nhật thành công (ghi Registry thất bại).".to_string()),
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
                Err("Chưa cài đặt Dell Command | Configure (cctk.exe).".to_string())
            }
        }
        OemVendor::Hp => {
            let enable_str = if enable { "true" } else { "false" };
            let script = format!(
                r#"$target = if ('{enable_str}' -eq 'true') {{ '80%' }} else {{ '100%' }}; $altTarget = if ('{enable_str}' -eq 'true') {{ 'Maximize my battery health' }} else {{ 'Let HP manage my battery charging' }}; $ret = 1; try {{ $svc = Get-CimInstance -Namespace root/hp/instrumentedBIOS -ClassName HP_BIOSSettingInterface -ErrorAction Stop; $res = Invoke-CimMethod -InputObject $svc -MethodName SetBIOSSetting -Arguments @{{ Name = 'Battery Care Function'; Value = $target }} -ErrorAction SilentlyContinue; if ($res -and $res.ReturnValue -eq 5) {{ $res = Invoke-CimMethod -InputObject $svc -MethodName SetBIOSSetting -Arguments @{{ Name = 'Battery Care Function'; Value = $altTarget }} -ErrorAction SilentlyContinue }}; if ($res -and $res.ReturnValue -ne 0) {{ $res = Invoke-CimMethod -InputObject $svc -MethodName SetBIOSSetting -Arguments @{{ Name = 'Battery Health Manager'; Value = $target }} -ErrorAction SilentlyContinue }}; if ($res) {{ $ret = $res.ReturnValue }} }} catch {{ try {{ $wmi = Get-WmiObject -Namespace root/hp/instrumentedBIOS -Class HP_BIOSSettingInterface -ErrorAction Stop; $resWmi = $wmi.SetBIOSSetting('Battery Care Function', $target); if ($resWmi -and $resWmi.ReturnValue -eq 5) {{ $resWmi = $wmi.SetBIOSSetting('Battery Care Function', $altTarget) }}; if ($resWmi) {{ $ret = $resWmi.ReturnValue }} }} catch {{ exit 101 }} }}; exit $ret"#
            );
            match crate::protection::run_powershell_elevated(&script) {
                Ok(0) => Ok("Đã thiết lập HP Battery Care Function thành công.".to_string()),
                Ok(5) => Err("Tham số thiết lập không được phiên bản BIOS HP này hỗ trợ.".to_string()),
                Ok(6) => Err("BIOS HP yêu cầu mật khẩu quản trị (Setup Password Required).".to_string()),
                Ok(101) => Err("Không tìm thấy giao tiếp WMI HP_BIOSSettingInterface trên bo mạch HP này.".to_string()),
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

    #[test]
    fn test_parse_dell_cctk_output() {
        assert_eq!(
            parse_dell_cctk_output("PrimaryBattChargeCfg=Custom:50-80"),
            (true, 80)
        );
        assert_eq!(
            parse_dell_cctk_output("PrimaryBattChargeCfg=Custom:50-70"),
            (true, 70)
        );
        assert_eq!(
            parse_dell_cctk_output("PrimaryBattChargeCfg=Standard"),
            (false, 80)
        );
        assert_eq!(parse_dell_cctk_output(""), (false, 80));
    }

    #[test]
    fn test_parse_hp_bios_setting_output() {
        assert_eq!(parse_hp_bios_setting_output("80%"), (true, 80));
        assert_eq!(
            parse_hp_bios_setting_output("Maximize my battery health"),
            (true, 80)
        );
        assert_eq!(parse_hp_bios_setting_output("50%"), (true, 50));
        assert_eq!(parse_hp_bios_setting_output("100%"), (false, 80));
        assert_eq!(parse_hp_bios_setting_output("Disabled"), (false, 80));
        assert_eq!(parse_hp_bios_setting_output(""), (false, 80));
    }

    #[test]
    fn test_dell_cctk_missing_url() {
        let dummy = BatterySupportInfo {
            kind: BatterySupportKind::DriverMissing,
            oem_vendor: OemVendor::Dell,
            oem_name: "Dell Inc.".to_string(),
            backend_name: "Dell Command | Configure".to_string(),
            status_message: "Cần cài đặt Dell Command | Configure (cctk.exe) để điều khiển mạch nạp: https://www.dell.com/support/kbdoc/000178000/dell-command-configure".to_string(),
            download_url: Some("https://www.dell.com/support/kbdoc/000178000/dell-command-configure".to_string()),
        };
        assert_eq!(dummy.kind, BatterySupportKind::DriverMissing);
        assert!(dummy.download_url.unwrap().contains("dell-command-configure"));
    }
}
