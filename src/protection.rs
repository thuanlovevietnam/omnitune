use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use sysinfo::Disks;

const CREATE_NO_WINDOW: u32 = 0x08000000;

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
    pub cpu_usage: f32,
}

#[derive(Deserialize, Debug, Default)]
struct ProductInfo {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct PhysicalDiskInfo {
    #[serde(rename = "FriendlyName")]
    friendly_name: Option<String>,
    #[serde(rename = "HealthStatus")]
    health_status: Option<String>,
    #[serde(rename = "OperationalStatus")]
    operational_status: Option<String>,
}

#[derive(Deserialize, Debug, Default)]
struct BatteryThresholdJson {
    #[serde(rename = "Enabled")]
    enabled: Option<bool>,
    #[serde(rename = "Percentage")]
    percentage: Option<u32>,
}

pub fn get_protection_overview(cpu_usage: f32) -> ProtectionOverview {
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

    let startup_count = get_startup_apps_count();
    let hib = check_hibernate_enabled();
    let device_name = get_device_product_name();
    let (p_name, p_health, p_status) = get_physical_disk_details();
    let (thresh_enabled, thresh_pct) = get_battery_threshold_status();

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
        cpu_usage,
    }
}

fn get_device_product_name() -> String {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "Get-CimInstance Win32_ComputerSystemProduct | Select-Object Name, Version | ConvertTo-Json",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(prod) = serde_json::from_str::<ProductInfo>(&text) {
            let ver = prod.version.unwrap_or_default();
            let nm = prod.name.unwrap_or_default();
            if !ver.is_empty() && !nm.is_empty() {
                return format!("{} ({})", ver, nm);
            } else if !ver.is_empty() {
                return ver;
            } else if !nm.is_empty() {
                return nm;
            }
        }
    }
    "Windows Laptop".to_string()
}

fn get_physical_disk_details() -> (String, String, String) {
    let script = "Get-PhysicalDisk | Select-Object FriendlyName, HealthStatus, OperationalStatus | ConvertTo-Json";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(disk) = serde_json::from_str::<PhysicalDiskInfo>(&text) {
            return (
                disk.friendly_name.unwrap_or_else(|| "NVMe SSD".to_string()),
                disk.health_status.unwrap_or_else(|| "Healthy".to_string()),
                disk.operational_status.unwrap_or_else(|| "OK".to_string()),
            );
        }
    }

    ("NVMe SSD".to_string(), "Healthy".to_string(), "OK".to_string())
}

pub fn get_battery_threshold_status() -> (bool, u32) {
    let script = r#"& {
$path = 'HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\1ZM8D35GNEF'
$p = Get-ItemProperty -Path $path -ErrorAction SilentlyContinue
if ($p) {
    [PSCustomObject]@{
        Enabled = ($p.ChargeStopControl -eq 1)
        Percentage = $p.ChargeStopPercentage
    } | ConvertTo-Json
}
}"#;

    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        if let Ok(th) = serde_json::from_str::<BatteryThresholdJson>(&text) {
            return (
                th.enabled.unwrap_or(false),
                th.percentage.unwrap_or(80),
            );
        }
    }

    (false, 80)
}

pub fn set_battery_threshold(enable: bool, stop_pct: u32) -> bool {
    let start_pct = stop_pct.saturating_sub(5).max(40);
    let control_val = if enable { 1 } else { 0 };

    let script = format!(
        r#"$cmd = 'Set-ItemProperty -Path HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\1ZM8D35GNEF -Name ChargeStopControl -Value {ctrl}; Set-ItemProperty -Path HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\1ZM8D35GNEF -Name ChargeStopPercentage -Value {stop}; Set-ItemProperty -Path HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\1ZM8D35GNEF -Name ChargeStartControl -Value {ctrl}; Set-ItemProperty -Path HKLM:\SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data\1ZM8D35GNEF -Name ChargeStartPercentage -Value {start}'; Start-Process powershell -ArgumentList "-NoProfile -Command $cmd" -Verb RunAs -WindowStyle Hidden"#,
        ctrl = control_val,
        stop = stop_pct,
        start = start_pct
    );

    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn get_startup_apps_count() -> usize {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-CimInstance Win32_StartupCommand | Measure-Object).Count",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        text.parse::<usize>().unwrap_or(0)
    } else {
        0
    }
}

fn check_hibernate_enabled() -> bool {
    let output = Command::new("powershell")
        .args([
            "-NoProfile",
            "-WindowStyle",
            "Hidden",
            "-Command",
            "(Get-ItemProperty -Path 'HKLM:\\SYSTEM\\CurrentControlSet\\Control\\Power' -Name HibernateEnabled -ErrorAction SilentlyContinue).HibernateEnabled",
        ])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
        text == "1"
    } else {
        false
    }
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
