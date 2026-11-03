use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use sysinfo::Disks;

const CREATE_NO_WINDOW: u32 = 0x08000000;

use windows_sys::Win32::UI::Shell::IsUserAnAdmin;

pub fn is_elevated() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

pub fn base64_encode(data: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity((data.len() + 2) / 3 * 4);
    for chunk in data.chunks(3) {
        let b0 = chunk[0] as usize;
        let b1 = chunk.get(1).copied().unwrap_or(0) as usize;
        let b2 = chunk.get(2).copied().unwrap_or(0) as usize;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[(triple >> 18) & 0x3F] as char);
        out.push(TABLE[(triple >> 12) & 0x3F] as char);
        if chunk.len() > 1 {
            out.push(TABLE[(triple >> 6) & 0x3F] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(TABLE[triple & 0x3F] as char);
        } else {
            out.push('=');
        }
    }
    out
}

pub fn encode_powershell_cmd(script: &str) -> String {
    let utf16_bytes: Vec<u8> = script.encode_utf16().flat_map(|u| u.to_le_bytes()).collect();
    base64_encode(&utf16_bytes)
}

pub fn run_powershell_elevated_timeout(inner_script: &str, timeout_ms: u32) -> Result<i32, String> {
    use std::os::windows::io::AsRawHandle;
    use windows_sys::Win32::Foundation::WAIT_TIMEOUT;
    use windows_sys::Win32::System::Threading::WaitForSingleObject;

    let inner_b64 = encode_powershell_cmd(inner_script);

    let mut cmd = if is_elevated() {
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-ExecutionPolicy",
            "Bypass",
            "-EncodedCommand",
            &inner_b64,
        ]);
        c
    } else {
        let wrapper = format!(
            "try {{ \
                $p = Start-Process powershell.exe -ArgumentList '-NoProfile','-ExecutionPolicy','Bypass','-EncodedCommand','{inner_b64}' \
                     -Verb RunAs -WindowStyle Hidden -Wait -PassThru -ErrorAction Stop; \
                if ($null -ne $p.ExitCode) {{ exit $p.ExitCode }} else {{ exit 1 }} \
            }} catch {{ exit 1223 }}"
        );
        let outer_b64 = encode_powershell_cmd(&wrapper);
        let mut c = Command::new("powershell");
        c.args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-EncodedCommand",
            &outer_b64,
        ]);
        c
    };

    cmd.creation_flags(CREATE_NO_WINDOW);

    let mut child = cmd.spawn().map_err(|e| format!("Không thể khởi chạy PowerShell: {e}"))?;
    let raw_handle = child.as_raw_handle() as windows_sys::Win32::Foundation::HANDLE;
    let wait_res = unsafe { WaitForSingleObject(raw_handle, timeout_ms) };

    if wait_res == WAIT_TIMEOUT {
        let _ = child.kill();
        let _ = child.wait();
        return Err(format!(
            "Lệnh vượt quá thời gian cho phép ({:.1}s).",
            timeout_ms as f32 / 1000.0
        ));
    }

    let status = child.wait().map_err(|e| format!("Lỗi chờ tiến trình: {e}"))?;
    Ok(status.code().unwrap_or(1))
}

pub fn run_powershell_elevated(inner_script: &str) -> Result<i32, String> {
    run_powershell_elevated_timeout(inner_script, 30_000)
}

#[derive(Deserialize, Default, Debug)]
struct PhysicalDiskQueryRaw {
    #[serde(rename = "FriendlyName")]
    friendly_name: Option<String>,
    #[serde(rename = "HealthStatus")]
    health_status: Option<serde_json::Value>,
    #[serde(rename = "OperationalStatus")]
    operational_status: Option<serde_json::Value>,
}

fn query_physical_disk_smart_status() -> (Option<String>, String, String) {
    let script = r#"$ProgressPreference = 'SilentlyContinue'; try { $d = @(Get-PhysicalDisk); if ($d.Count -gt 0) { $bad = $d | Where-Object { $_.HealthStatus -ne 'Healthy' -or $_.OperationalStatus -ne 'OK' } | Select-Object -First 1; if ($bad) { $bad | Select-Object FriendlyName, HealthStatus, OperationalStatus | ConvertTo-Json -Compress } else { $d[0] | Select-Object FriendlyName, HealthStatus, OperationalStatus | ConvertTo-Json -Compress } } } catch {}"#;
    let enc = encode_powershell_cmd(script);

    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-EncodedCommand", &enc])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let json_text = output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    if json_text.is_empty() {
        return (None, "Healthy".to_string(), "OK".to_string());
    }

    let parsed: PhysicalDiskQueryRaw = match serde_json::from_str(&json_text) {
        Ok(val) => val,
        Err(_) => return (None, "Healthy".to_string(), "OK".to_string()),
    };

    let health = match parsed.health_status {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        Some(serde_json::Value::Number(n)) => match n.as_i64() {
            Some(0) => "Healthy".to_string(),
            Some(1) => "Warning".to_string(),
            Some(2) => "Unhealthy".to_string(),
            _ => "Healthy".to_string(),
        },
        _ => "Healthy".to_string(),
    };

    let status = match parsed.operational_status {
        Some(serde_json::Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        Some(serde_json::Value::Array(arr)) => {
            let parts: Vec<String> = arr
                .into_iter()
                .filter_map(|v| match v {
                    serde_json::Value::String(s) if !s.trim().is_empty() => Some(s.trim().to_string()),
                    serde_json::Value::Number(n) => match n.as_i64() {
                        Some(2) => Some("OK".to_string()),
                        Some(3) => Some("Degraded".to_string()),
                        Some(11) => Some("Predictive Failure".to_string()),
                        Some(other) => Some(other.to_string()),
                        None => None,
                    },
                    _ => None,
                })
                .collect();
            if parts.is_empty() {
                "OK".to_string()
            } else {
                parts.join(", ")
            }
        }
        Some(serde_json::Value::Number(n)) => match n.as_i64() {
            Some(2) => "OK".to_string(),
            Some(3) => "Degraded".to_string(),
            Some(11) => "Predictive Failure".to_string(),
            Some(other) => other.to_string(),
            None => "OK".to_string(),
        },
        _ => "OK".to_string(),
    };

    let friendly = parsed
        .friendly_name
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty());

    (friendly, health, status)
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
}

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ProtectionOverview {
    pub device_name: String,
    pub disks: Vec<DiskInfo>,
    pub physical_disk_name: String,
    pub physical_disk_health: String,
    pub physical_disk_status: String,
    pub startup_apps_count: usize,
    pub is_hibernate_enabled: bool,
    pub battery_threshold_enabled: bool,
    pub battery_threshold_pct: u32,
    pub battery_support: crate::battery_limiter::BatterySupportInfo,
    pub cpu_usage: f32,
}

pub fn get_protection_overview(cpu_usage: f32, startup_count: usize) -> ProtectionOverview {
    let mut disks = Vec::new();

    if let Ok(disks_info) = Disks::new_with_refreshed_list() {
        for d in &disks_info {
            disks.push(DiskInfo {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                total_space: d.total_space(),
                available_space: d.available_space(),
            });
        }
    }

    let hib = crate::reg::check_hibernate_enabled_native();
    let device_name = crate::reg::get_device_product_name_native();
    let mut p_name = crate::reg::get_physical_disk_name_native();
    let (smart_name, p_health, p_status) = query_physical_disk_smart_status();
    if p_name.is_empty() || p_name == "NVMe SSD" || p_name == "Ổ cứng SSD" || p_name == "Ổ cứng HDD" {
        if let Some(name) = smart_name {
            p_name = name;
        }
    }
    let battery_support = crate::battery_limiter::detect_battery_limiter_support();
    let (thresh_enabled, thresh_pct) = crate::battery_limiter::get_battery_threshold(&battery_support);

    ProtectionOverview {
        device_name,
        disks,
        physical_disk_name: p_name,
        physical_disk_health: p_health,
        physical_disk_status: p_status,
        startup_apps_count: startup_count,
        is_hibernate_enabled: hib,
        battery_threshold_enabled: thresh_enabled,
        battery_threshold_pct: thresh_pct,
        battery_support,
        cpu_usage,
    }
}

pub fn set_battery_threshold(enable: bool, stop_pct: u32) -> bool {
    crate::battery_limiter::set_battery_threshold(enable, stop_pct).is_ok()
}

pub fn set_battery_threshold_result(enable: bool, stop_pct: u32) -> Result<String, String> {
    crate::battery_limiter::set_battery_threshold(enable, stop_pct)
}

pub fn enable_hibernate_as_admin() -> bool {
    matches!(
        run_powershell_elevated("powercfg /hibernate on; exit $LASTEXITCODE"),
        Ok(0)
    )
}

pub fn disable_hibernate() -> bool {
    matches!(
        run_powershell_elevated("powercfg /hibernate off; exit $LASTEXITCODE"),
        Ok(0)
    )
}

pub fn create_restore_point() -> Result<String, String> {
    let script = r#"
$ErrorActionPreference = 'Stop'
$vss = Get-Service -Name vss -ErrorAction SilentlyContinue
if (-not $vss) { exit 101 }
if ($vss.StartType -eq 'Disabled') { exit 102 }

$pol = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Policies\Microsoft\Windows NT\SystemRestore' -ErrorAction SilentlyContinue
if ($pol -and ($pol.DisableSR -eq 1 -or $pol.DisableConfig -eq 1)) { exit 103 }

$cfg = Get-ItemProperty -Path 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore\cfg' -ErrorAction SilentlyContinue
if (-not $cfg -or [int]$cfg.DiskPercent -le 0) { exit 104 }

$srKey = 'HKLM:\SOFTWARE\Microsoft\Windows NT\CurrentVersion\SystemRestore'
if (Test-Path $srKey) {
    Set-ItemProperty -Path $srKey -Name 'SystemRestorePointCreationFrequency' -Value 0 -Type DWord -Force -ErrorAction SilentlyContinue
}

try {
    Checkpoint-Computer -Description 'OmniTune-PreTuning' -RestorePointType 'MODIFY_SETTINGS' -ErrorAction Stop
    exit 0
} catch {
    exit 105
}
"#;

    match run_powershell_elevated_timeout(script, 90_000) {
        Ok(0) => Ok("Đã tạo điểm khôi phục hệ thống (System Restore Point) thành công.".to_string()),
        Ok(1223) => Err("Người dùng từ chối cấp quyền Quản trị viên (UAC).".to_string()),
        Ok(101) => Err("Dịch vụ Volume Shadow Copy (VSS) không tồn tại trên hệ thống.".to_string()),
        Ok(102) => Err("Dịch vụ Volume Shadow Copy (VSS) đang bị Disabled. Cần bật lại trong Services.".to_string()),
        Ok(103) => Err("System Restore đã bị vô hiệu hóa bởi Group Policy.".to_string()),
        Ok(104) => Err("Bảo vệ hệ thống (System Protection) đang tắt trên ổ C:. Hãy bật trong Windows trước khi tạo.".to_string()),
        Ok(105) => Err("Lỗi khi tạo điểm khôi phục từ Windows (Kiểm tra dung lượng trống ổ C: hoặc quyền hệ thống).".to_string()),
        Ok(code) => Err(format!("Lỗi khi tạo Restore Point (Mã lỗi: {code}). Kiểm tra dung lượng trống ổ C:.")),
        Err(e) => Err(format!("Lỗi thực thi Restore Point: {e}")),
    }
}

pub fn restore_windows_defaults() -> Vec<String> {
    let mut results = Vec::new();

    // 1. Restore Windows Power Schemes to default
    let res = Command::new("powercfg")
        .args(["/restoredefaultschemes"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();
    if res.map(|s| s.success()).unwrap_or(false) {
        results.push("Đã khôi phục Power Schemes mặc định.".to_string());
    }

    // 2. Restore GPU, DirectX and GameDVR
    results.extend(crate::gpu::restore_gpu_defaults());

    // 3. Restore Timer Resolution to default 15.6ms
    unsafe {
        let _ = windows_sys::Win32::Media::timeEndPeriod(1);
    }
    results.push("Đã khôi phục Timer Resolution về mặc định.".to_string());

    // 4. Restore Telemetry DiagTrack
    crate::startup::toggle_telemetry(false);
    results.push("Đã khôi phục Telemetry DiagTrack về ban đầu.".to_string());

    // 5. Restore Deep Debloat
    let _ = apply_deep_debloat(false);
    results.push("Đã khôi phục cài đặt Copilot, Bing Search, Edge về mặc định.".to_string());

    results
}

pub fn apply_deep_debloat(enable: bool) -> Vec<String> {
    use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    let mut logs = Vec::new();

    let val = if enable { 1 } else { 0 };

    // 1. Windows Copilot
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
        val,
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Windows\WindowsCopilot",
        "TurnOffWindowsCopilot",
        val,
    );
    logs.push(if enable { "Tắt Windows Copilot nền" } else { "Bật lại Copilot" }.to_string());

    // 2. Bing Search Start Menu
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Policies\Microsoft\Windows\Explorer",
        "DisableSearchBoxSuggestions",
        val,
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Search",
        "BingSearchEnabled",
        if enable { 0 } else { 1 },
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\Search",
        "CortanaConsent",
        if enable { 0 } else { 1 },
    );
    logs.push(if enable { "Tắt Bing Search Start Menu" } else { "Bật lại Bing Search Start Menu" }.to_string());

    // 3. Edge Background Mode & Startup Boost
    let edge_val = if enable { 0 } else { 1 };
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Edge",
        "StartupBoostEnabled",
        edge_val,
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Edge",
        "BackgroundModeEnabled",
        edge_val,
    );
    logs.push(if enable { "Chặn Edge chạy ngầm chiếm RAM" } else { "Bật lại Edge chạy ngầm" }.to_string());

    logs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_base64_encode() {
        assert_eq!(base64_encode(b""), "");
        assert_eq!(base64_encode(b"f"), "Zg==");
        assert_eq!(base64_encode(b"fo"), "Zm8=");
        assert_eq!(base64_encode(b"foo"), "Zm9v");
        assert_eq!(base64_encode(b"foob"), "Zm9vYg==");
        assert_eq!(base64_encode(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64_encode(b"foobar"), "Zm9vYmFy");
    }

    #[test]
    fn test_encode_powershell_cmd() {
        // "exit 0" in UTF-16LE is: [101, 0, 120, 0, 105, 0, 116, 0, 32, 0, 48, 0]
        let encoded = encode_powershell_cmd("exit 0");
        assert_eq!(encoded, "ZQB4AGkAdAAgADAA");

        // Test unicode & curly braces
        let complex = r#"if ($true) { Write-Output 'Tiếng Việt' }"#;
        let enc_complex = encode_powershell_cmd(complex);
        assert!(!enc_complex.is_empty());
    }

    #[test]
    fn test_is_elevated_callable() {
        // Should execute Win32 API without panic/crash
        let _ = is_elevated();
    }
}
