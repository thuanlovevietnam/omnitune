use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;
use sysinfo::Disks;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Default)]
pub struct DiskInfo {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
}

#[derive(Debug, Clone, Default)]
pub struct ProtectionOverview {
    pub device_name: String,
    pub disks: Vec<DiskInfo>,
    pub startup_apps_count: usize,
    pub is_hibernate_enabled: bool,
    pub cpu_usage: f32,
}

#[derive(Deserialize, Debug, Default)]
struct ProductInfo {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Version")]
    version: Option<String>,
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

    ProtectionOverview {
        device_name,
        disks,
        startup_apps_count: startup_count,
        is_hibernate_enabled: hib,
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
