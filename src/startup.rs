use crate::types::StartupAppItem;
use std::os::windows::process::CommandExt;
use std::process::Command;

const CREATE_NO_WINDOW: u32 = 0x08000000;

pub fn get_startup_apps() -> Vec<StartupAppItem> {
    crate::reg::get_startup_apps_native()
}

pub fn toggle_startup_item(name: &str, location: &str, enable: bool) -> bool {
    crate::reg::toggle_startup_item_native(name, location, enable)
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
    use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;

    // Permanent Policy in Windows Registry
    let val = if disable { 0 } else { 3 };
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Policies\Microsoft\Windows\DataCollection",
        "AllowTelemetry",
        val,
    );
    let _ = crate::reg::reg_set_u32(
        HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Windows\CurrentVersion\Policies\DataCollection",
        "AllowTelemetry",
        val,
    );

    let script = if disable {
        "Start-Process sc.exe -ArgumentList 'stop DiagTrack' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'config DiagTrack start=disabled' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'stop dmwappushservice' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'config dmwappushservice start=disabled' -Verb RunAs -WindowStyle Hidden"
    } else {
        "Start-Process sc.exe -ArgumentList 'config DiagTrack start=auto' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'start DiagTrack' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'config dmwappushservice start=delayed-auto' -Verb RunAs -WindowStyle Hidden; Start-Process sc.exe -ArgumentList 'start dmwappushservice' -Verb RunAs -WindowStyle Hidden"
    };

    Command::new("powershell")
        .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", script])
        .creation_flags(CREATE_NO_WINDOW)
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}
