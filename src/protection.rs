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
    let p_name = crate::reg::get_physical_disk_name_native();
    let (thresh_enabled, thresh_pct) = crate::reg::get_battery_threshold_native();

    ProtectionOverview {
        device_name,
        disks,
        physical_disk_name: p_name,
        physical_disk_health: "Healthy".to_string(),
        physical_disk_status: "OK".to_string(),
        startup_apps_count: startup_count,
        is_hibernate_enabled: hib,
        battery_threshold_enabled: thresh_enabled,
        battery_threshold_pct: thresh_pct,
        cpu_usage,
    }
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

pub fn enable_hibernate_as_admin() -> bool {
    let script = "Start-Process powercfg -ArgumentList '/hibernate on' -Verb RunAs -WindowStyle Hidden";
    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
