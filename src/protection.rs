use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use sysinfo::Disks;

const CREATE_NO_WINDOW: u32 = 0x08000000;

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

    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
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
    let script = "Start-Process powercfg -ArgumentList '/hibernate on' -Verb RunAs -WindowStyle Hidden";
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

pub fn create_restore_point() -> Result<String, String> {
    let script = "Checkpoint-Computer -Description 'OmniTune-PreTuning' -RestorePointType 'MODIFY_SETTINGS'";
    let full_cmd = format!(
        "Start-Process powershell -ArgumentList \"-NoProfile -ExecutionPolicy Bypass -Command {}\" -Verb RunAs -WindowStyle Hidden",
        script
    );

    let res = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &full_cmd])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    match res {
        Ok(s) if s.success() => Ok("Đã gửi yêu cầu tạo System Restore Point thành công.".to_string()),
        _ => Err("Không thể tạo điểm khôi phục (Cần quyền Admin).".to_string()),
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
