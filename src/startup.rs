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
    #[serde(rename = "IsEnabled")]
    is_enabled: Option<bool>,
}

pub fn get_startup_apps() -> Vec<StartupAppItem> {
    let script = r#"& {
$apps = Get-CimInstance Win32_StartupCommand | Select-Object Name, Command, Location, User
$runKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
$folderKey = "HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"

$apps | ForEach-Object {
    $nm = $_.Name
    $loc = $_.Location
    $enabled = $true

    if ($loc -like "*Startup*") {
        $p = Get-ItemProperty -Path $folderKey -Name $nm -ErrorAction SilentlyContinue
        if ($p) {
            $val = $p.$nm
            if ($val -and ($val[0] -eq 1 -or $val[0] -eq 3)) { $enabled = $false }
        }
    } else {
        $p = Get-ItemProperty -Path $runKey -Name $nm -ErrorAction SilentlyContinue
        if ($p) {
            $val = $p.$nm
            if ($val -and ($val[0] -eq 1 -or $val[0] -eq 3)) { $enabled = $false }
        }
    }

    [PSCustomObject]@{
        Name = $_.Name
        Command = $_.Command
        Location = $_.Location
        User = $_.User
        IsEnabled = $enabled
    }
} | ConvertTo-Json
}"#;

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
            is_enabled: i.is_enabled.unwrap_or(true),
        })
        .collect()
}

pub fn toggle_startup_item(name: &str, location: &str, enable: bool) -> bool {
    let is_folder = location.contains("Startup");
    let target_key = if is_folder {
        r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"
    } else {
        r"HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
    };

    let byte_val = if enable { 2 } else { 3 };

    let script = format!(
        r#"& {{
$key = '{key}'
$p = Get-ItemProperty -Path $key -Name '{nm}' -ErrorAction SilentlyContinue
if ($p) {{
    $val = $p.'{nm}'
    $val[0] = {b}
    Set-ItemProperty -Path $key -Name '{nm}' -Value $val -ErrorAction SilentlyContinue
}} else {{
    $val = [byte[]](0x0{b}, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0)
    Set-ItemProperty -Path $key -Name '{nm}' -Value $val -ErrorAction SilentlyContinue
}}
}}"#,
        key = target_key,
        nm = name,
        b = byte_val
    );

    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
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
