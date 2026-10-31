use crate::types::StartupAppItem;
use serde::Deserialize;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Deserialize, Debug, Default)]
struct StartupJsonItem {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Command")]
    command: Option<String>,
    #[serde(rename = "Location")]
    location: Option<String>,
    #[serde(rename = "User")]
    user: Option<String>,
}

pub fn get_startup_apps() -> Vec<StartupAppItem> {
    let script = "Get-CimInstance Win32_StartupCommand | Select-Object Name, Command, Location, User | ConvertTo-Json";
    let output = Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    let json_text = output
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_string())
        .unwrap_or_default();

    let trimmed = json_text.trim();
    if trimmed.is_empty() {
        return Vec::new();
    }

    let items: Vec<StartupJsonItem> = if trimmed.starts_with('[') {
        serde_json::from_str(trimmed).unwrap_or_default()
    } else if trimmed.starts_with('{') {
        if let Ok(single) = serde_json::from_str::<StartupJsonItem>(trimmed) {
            vec![single]
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    items
        .into_iter()
        .map(|i| StartupAppItem {
            name: i.name.unwrap_or_default(),
            command: i.command.unwrap_or_default(),
            location: i.location.unwrap_or_default(),
            user: i.user.unwrap_or_default(),
        })
        .collect()
}

pub fn is_telemetry_active() -> bool {
    let output = Command::new("sc.exe")
        .args(["query", "DiagTrack"])
        .creation_flags(CREATE_NO_WINDOW)
        .output();

    if let Ok(out) = output {
        let text = String::from_utf8_lossy(&out.stdout);
        text.contains("RUNNING")
    } else {
        false
    }
}

pub fn toggle_telemetry(disable: bool) -> bool {
    let script = if disable {
        "Start-Process sc.exe -ArgumentList 'stop DiagTrack' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'config DiagTrack start=disabled' -Verb RunAs -WindowStyle Hidden"
    } else {
        "Start-Process sc.exe -ArgumentList 'config DiagTrack start=auto' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'start DiagTrack' -Verb RunAs -WindowStyle Hidden"
    };

    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
