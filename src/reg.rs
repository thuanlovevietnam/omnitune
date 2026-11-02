use crate::types::StartupAppItem;
use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegCreateKeyW, RegEnumKeyExW, RegEnumValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW, HKEY, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WRITE, REG_BINARY,
    REG_DWORD, REG_EXPAND_SZ, REG_SZ,
};

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn reg_read_string(root: HKEY, subkey: &str, value_name: &str) -> Option<String> {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }

        let val_w = to_wide(value_name);
        let mut buf = [0u8; 512];
        let mut buf_len = buf.len() as u32;
        let mut val_type = 0u32;
        let res = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            buf.as_mut_ptr(),
            &mut buf_len,
        );
        RegCloseKey(hkey);

        if res == 0 && (val_type == REG_SZ || val_type == REG_EXPAND_SZ) && buf_len >= 2 {
            let u16_len = (buf_len as usize) / 2;
            let u16_slice = std::slice::from_raw_parts(buf.as_ptr() as *const u16, u16_len);
            let s = String::from_utf16_lossy(u16_slice);
            let trimmed = s.trim_end_matches('\0').trim().to_string();
            if !trimmed.is_empty() {
                return Some(trimmed);
            }
        }
        None
    }
}

pub fn reg_read_u32(root: HKEY, subkey: &str, value_name: &str) -> Option<u32> {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }

        let val_w = to_wide(value_name);
        let mut val = 0u32;
        let mut val_len = std::mem::size_of::<u32>() as u32;
        let mut val_type = 0u32;
        let res = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            &mut val as *mut u32 as *mut u8,
            &mut val_len,
        );
        RegCloseKey(hkey);

        if res == 0 && val_type == REG_DWORD {
            return Some(val);
        }
        None
    }
}

pub fn reg_set_u32(root: HKEY, subkey: &str, value_name: &str, val: u32) -> bool {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegCreateKeyW(root, subkey_w.as_ptr(), &mut hkey) != 0 {
            return false;
        }

        let val_w = to_wide(value_name);
        let ret = RegSetValueExW(
            hkey,
            val_w.as_ptr(),
            0,
            REG_DWORD,
            &val as *const u32 as *const u8,
            std::mem::size_of::<u32>() as u32,
        );
        RegCloseKey(hkey);
        ret == 0
    }
}

pub fn reg_set_string(root: HKEY, subkey: &str, value_name: &str, val: &str) -> bool {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegCreateKeyW(root, subkey_w.as_ptr(), &mut hkey) != 0 {
            return false;
        }

        let val_w = to_wide(value_name);
        let str_w = to_wide(val);
        let ret = RegSetValueExW(
            hkey,
            val_w.as_ptr(),
            0,
            REG_SZ,
            str_w.as_ptr() as *const u8,
            (str_w.len() * 2) as u32,
        );
        RegCloseKey(hkey);
        ret == 0
    }
}

pub fn reg_read_binary(root: HKEY, subkey: &str, value_name: &str) -> Option<Vec<u8>> {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return None;
        }

        let val_w = to_wide(value_name);
        let mut buf = vec![0u8; 256];
        let mut buf_len = buf.len() as u32;
        let mut val_type = 0u32;
        let res = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            buf.as_mut_ptr(),
            &mut buf_len,
        );
        RegCloseKey(hkey);

        if res == 0 && val_type == REG_BINARY {
            buf.truncate(buf_len as usize);
            return Some(buf);
        }
        None
    }
}

pub fn reg_enum_subkeys(root: HKEY, subkey: &str) -> Vec<String> {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return Vec::new();
        }

        let mut subkeys = Vec::new();
        let mut index = 0u32;
        let mut name_buf = [0u16; 256];
        loop {
            let mut name_len = name_buf.len() as u32;
            let res = RegEnumKeyExW(
                hkey,
                index,
                name_buf.as_mut_ptr(),
                &mut name_len,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            if res != 0 {
                break;
            }
            let s = String::from_utf16_lossy(&name_buf[..name_len as usize]);
            subkeys.push(s);
            index += 1;
        }
        RegCloseKey(hkey);
        subkeys
    }
}

pub fn reg_key_exists(root: HKEY, subkey: &str) -> bool {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) == 0 {
            RegCloseKey(hkey);
            true
        } else {
            false
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OemVendor {
    Lenovo,
    Asus,
    Dell,
    Hp,
    Msi,
    Acer,
    Apple,
    Microsoft,
    Samsung,
    Other,
}

impl OemVendor {
    pub fn display_name(&self) -> &'static str {
        match self {
            OemVendor::Lenovo => "Lenovo",
            OemVendor::Asus => "ASUS",
            OemVendor::Dell => "Dell",
            OemVendor::Hp => "HP",
            OemVendor::Msi => "MSI",
            OemVendor::Acer => "Acer",
            OemVendor::Apple => "Apple",
            OemVendor::Microsoft => "Microsoft",
            OemVendor::Samsung => "Samsung",
            OemVendor::Other => "Khác",
        }
    }
}

pub fn get_device_oem_vendor() -> (OemVendor, String) {
    let subkey = r"HARDWARE\DESCRIPTION\System\BIOS";
    let sys_mfg = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "SystemManufacturer")
        .unwrap_or_default();
    let board_mfg = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "BaseBoardManufacturer")
        .unwrap_or_default();

    let combined = format!("{} {}", sys_mfg, board_mfg).to_lowercase();
    let raw_name = if !sys_mfg.is_empty() {
        sys_mfg
    } else if !board_mfg.is_empty() {
        board_mfg
    } else {
        "Unknown".to_string()
    };

    let vendor = if combined.contains("lenovo") || combined.contains("ibm") {
        OemVendor::Lenovo
    } else if combined.contains("asus") || combined.contains("asustek") {
        OemVendor::Asus
    } else if combined.contains("dell") || combined.contains("alienware") {
        OemVendor::Dell
    } else if combined.contains("hp") || combined.contains("hewlett-packard") {
        OemVendor::Hp
    } else if combined.contains("msi") || combined.contains("micro-star") {
        OemVendor::Msi
    } else if combined.contains("acer") {
        OemVendor::Acer
    } else if combined.contains("apple") {
        OemVendor::Apple
    } else if combined.contains("microsoft") {
        OemVendor::Microsoft
    } else if combined.contains("samsung") {
        OemVendor::Samsung
    } else {
        OemVendor::Other
    };

    (vendor, raw_name)
}

pub fn get_device_product_name_native() -> String {
    let subkey = r"HARDWARE\DESCRIPTION\System\BIOS";
    let prod_name = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "SystemProductName");
    let version = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "SystemVersion");
    let board = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "BaseBoardProduct");

    match (version, prod_name) {
        (Some(v), Some(p)) if !v.is_empty() && !p.is_empty() => format!("{} ({})", v, p),
        (Some(v), _) if !v.is_empty() => v,
        (_, Some(p)) if !p.is_empty() => p,
        _ => board.unwrap_or_else(|| "Windows PC".to_string()),
    }
}

pub fn check_hibernate_enabled_native() -> bool {
    reg_read_u32(
        HKEY_LOCAL_MACHINE,
        r"SYSTEM\CurrentControlSet\Control\Power",
        "HibernateEnabled",
    )
    .map(|v| v == 1)
    .unwrap_or(false)
}

#[allow(dead_code)]
pub fn get_battery_threshold_native() -> (bool, u32) {
    for base in [
        r"SOFTWARE\WOW6432Node\Lenovo\PWRMGRV\ConfKeys\Data",
        r"SOFTWARE\Lenovo\PWRMGRV\ConfKeys\Data",
    ] {
        let subkeys = reg_enum_subkeys(HKEY_LOCAL_MACHINE, base);
        for sub in subkeys {
            let p = format!(r"{}\{}", base, sub);
            if let Some(enabled) = reg_read_u32(HKEY_LOCAL_MACHINE, &p, "ChargeStopControl") {
                let pct = reg_read_u32(HKEY_LOCAL_MACHINE, &p, "ChargeStopPercentage").unwrap_or(80);
                return (enabled == 1, pct);
            }
        }
    }
    (false, 80)
}

pub fn get_gamedvr_native() -> bool {
    let app_capture = reg_read_u32(
        HKEY_CURRENT_USER,
        r"Software\Microsoft\Windows\CurrentVersion\GameDVR",
        "AppCaptureEnabled",
    );
    if let Some(v) = app_capture {
        return v == 1;
    }

    reg_read_u32(
        HKEY_CURRENT_USER,
        r"System\GameConfigStore",
        "GameDVR_Enabled",
    )
    .map(|v| v == 1)
    .unwrap_or(false)
}

pub fn get_physical_disk_name_native() -> String {
    // Check Enum for disk
    let subkey = r"SYSTEM\CurrentControlSet\Services\disk\Enum";
    if let Some(entry) = reg_read_string(HKEY_LOCAL_MACHINE, subkey, "0") {
        if let Some(pos) = entry.find("Prod_") {
            let part = &entry[pos + 5..];
            let name = part.split('\\').next().unwrap_or(part);
            let cleaned = name.replace('_', " ").trim().to_string();
            if !cleaned.is_empty() {
                return cleaned;
            }
        }
    }
    "NVMe SSD".to_string()
}

fn is_startup_approved(name: &str, is_folder: bool) -> bool {
    let subkey = if is_folder {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"
    } else {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
    };

    if let Some(bytes) = reg_read_binary(HKEY_CURRENT_USER, subkey, name) {
        if let Some(&first) = bytes.first() {
            // Byte 0x01 or 0x03 means disabled in Windows StartupApproved
            return first != 1 && first != 3;
        }
    }
    true
}

pub fn get_startup_apps_native() -> Vec<StartupAppItem> {
    let mut apps = Vec::new();

    // 1. HKCU Run
    enumerate_run_key(HKEY_CURRENT_USER, r"Software\Microsoft\Windows\CurrentVersion\Run", "HKCU:Run", &mut apps);

    // 2. HKLM Run
    enumerate_run_key(HKEY_LOCAL_MACHINE, r"Software\Microsoft\Windows\CurrentVersion\Run", "HKLM:Run", &mut apps);

    // 3. User Startup Folder
    if let Ok(appdata) = std::env::var("APPDATA") {
        let startup_path = PathBuf::from(appdata).join(r"Microsoft\Windows\Start Menu\Programs\Startup");
        if let Ok(entries) = std::fs::read_dir(startup_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = path.file_stem().and_then(|s| s.to_str()).unwrap_or_default();
                    if !file_name.is_empty() && file_name != "desktop.ini" {
                        let is_enabled = is_startup_approved(file_name, true);
                        apps.push(StartupAppItem {
                            name: file_name.to_string(),
                            command: path.to_string_lossy().to_string(),
                            location: "Startup Folder".to_string(),
                            user: "Current User".to_string(),
                            is_enabled,
                        });
                    }
                }
            }
        }
    }

    apps
}

fn enumerate_run_key(root: HKEY, subkey: &str, loc_label: &str, apps: &mut Vec<StartupAppItem>) {
    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(root, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return;
        }

        let mut index = 0u32;
        loop {
            let mut name_buf = [0u16; 256];
            let mut name_len = name_buf.len() as u32;
            let mut val_type = 0u32;
            let mut data_buf = [0u8; 1024];
            let mut data_len = data_buf.len() as u32;

            let res = RegEnumValueW(
                hkey,
                index,
                name_buf.as_mut_ptr(),
                &mut name_len,
                std::ptr::null(),
                &mut val_type,
                data_buf.as_mut_ptr(),
                &mut data_len,
            );

            if res != 0 {
                break;
            }

            let name = OsString::from_wide(&name_buf[..name_len as usize])
                .to_string_lossy()
                .to_string();

            let cmd = if val_type == REG_SZ || val_type == REG_EXPAND_SZ {
                let u16_len = (data_len as usize) / 2;
                let u16_slice = std::slice::from_raw_parts(data_buf.as_ptr() as *const u16, u16_len);
                String::from_utf16_lossy(u16_slice).trim_end_matches('\0').trim().to_string()
            } else {
                String::new()
            };

            let is_enabled = is_startup_approved(&name, false);

            apps.push(StartupAppItem {
                name,
                command: cmd,
                location: loc_label.to_string(),
                user: if root == HKEY_CURRENT_USER { "Current User".to_string() } else { "All Users".to_string() },
                is_enabled,
            });

            index += 1;
        }

        RegCloseKey(hkey);
    }
}

pub fn toggle_startup_item_native(name: &str, location: &str, enable: bool) -> bool {
    let is_folder = location.contains("Startup");
    let subkey = if is_folder {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\StartupFolder"
    } else {
        r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run"
    };

    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ | KEY_WRITE, &mut hkey) != 0 {
            return false;
        }

        let val_w = to_wide(name);
        let byte_val = if enable { 0x02u8 } else { 0x03u8 };

        // If exists, update first byte; otherwise create 12-byte buffer
        let mut existing_buf = [0u8; 12];
        let mut buf_len = existing_buf.len() as u32;
        let mut val_type = 0u32;
        let res = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            existing_buf.as_mut_ptr(),
            &mut buf_len,
        );

        if res == 0 && buf_len > 0 {
            existing_buf[0] = byte_val;
        } else {
            existing_buf = [byte_val, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
            buf_len = 12;
        }

        let write_res = RegSetValueExW(
            hkey,
            val_w.as_ptr(),
            0,
            REG_BINARY,
            existing_buf.as_ptr(),
            buf_len,
        );

        RegCloseKey(hkey);
        write_res == 0
    }
}
