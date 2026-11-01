use crate::autonomous_engine::OperatingMode;
use crate::scenarios::Scenario;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use windows_sys::Win32::System::Registry::{
    RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW, RegSetValueExW, HKEY,
    HKEY_CURRENT_USER, KEY_READ, KEY_WRITE, REG_SZ,
};

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn default_true() -> bool {
    true
}

fn default_brightness() -> u32 {
    80
}

fn default_ram_threshold() -> u32 {
    80
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub timer_1ms_enabled: bool,
    pub keep_awake_mode: u8,
    pub cpu_limit_dc: u32,
    pub cpu_limit_ac: u32,
    pub cooling_active: bool,
    pub gamedvr_enabled: bool,
    pub auto_pilot_enabled: bool,
    pub battery_threshold_80: bool,
    pub minimize_to_tray: bool,
    pub run_on_startup: bool,
    #[serde(default)]
    pub operating_mode: OperatingMode,
    #[serde(default)]
    pub saved_scenario: Scenario,
    #[serde(default = "default_true")]
    pub eco_qos_enabled: bool,
    #[serde(default = "default_true")]
    pub auto_ram_enabled: bool,
    #[serde(default = "default_brightness")]
    pub brightness_level: u32,
    #[serde(default = "default_ram_threshold")]
    pub ram_trim_threshold_pct: u32,
    #[serde(default = "default_true")]
    pub sysmain_disabled: bool,
    #[serde(default = "default_true")]
    pub disable_ntfs_last_access: bool,
    #[serde(default = "default_true")]
    pub mmcss_optimized: bool,
    #[serde(default = "default_true")]
    pub tcp_autotuning: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            timer_1ms_enabled: false,
            keep_awake_mode: 0,
            cpu_limit_dc: 80,
            cpu_limit_ac: 100,
            cooling_active: false,
            gamedvr_enabled: false,
            auto_pilot_enabled: true,
            battery_threshold_80: false,
            minimize_to_tray: false,
            run_on_startup: false,
            operating_mode: OperatingMode::WorkloadAdaptive,
            saved_scenario: Scenario::Default,
            eco_qos_enabled: true,
            auto_ram_enabled: true,
            brightness_level: 80,
            ram_trim_threshold_pct: 80,
            sysmain_disabled: true,
            disable_ntfs_last_access: true,
            mmcss_optimized: true,
            tcp_autotuning: true,
        }
    }
}

pub fn get_config_path() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        let dir = PathBuf::from(appdata).join("OmniTune");
        let _ = std::fs::create_dir_all(&dir);
        return dir.join("config.json");
    }
    PathBuf::from("config.json")
}

pub fn load_config() -> AppConfig {
    let path = get_config_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(cfg) = serde_json::from_str::<AppConfig>(&data) {
            return cfg;
        }
    }
    let default_cfg = AppConfig::default();
    save_config(&default_cfg);
    default_cfg
}

pub fn save_config(config: &AppConfig) {
    let path = get_config_path();
    if let Ok(json) = serde_json::to_string_pretty(config) {
        let _ = std::fs::write(path, json);
    }
}

pub fn is_autostart_enabled() -> bool {
    let subkey = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let val_name = "OmniTune";

    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ, &mut hkey) != 0 {
            return false;
        }

        let val_w = to_wide(val_name);
        let mut val_type: u32 = 0;
        let mut buf_size: u32 = 0;
        let ret = RegQueryValueExW(
            hkey,
            val_w.as_ptr(),
            std::ptr::null_mut(),
            &mut val_type,
            std::ptr::null_mut(),
            &mut buf_size,
        );
        RegCloseKey(hkey);
        ret == 0 && buf_size > 0
    }
}

pub fn set_autostart_registry(enable: bool) -> bool {
    let subkey = r"Software\Microsoft\Windows\CurrentVersion\Run";
    let val_name = "OmniTune";

    unsafe {
        let subkey_w = to_wide(subkey);
        let mut hkey: HKEY = std::mem::zeroed();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_w.as_ptr(), 0, KEY_READ | KEY_WRITE, &mut hkey) != 0 {
            return false;
        }

        let val_w = to_wide(val_name);
        let success = if enable {
            if let Ok(exe_path) = std::env::current_exe() {
                let cmd = format!("\"{}\" --minimized", exe_path.to_string_lossy());
                let cmd_w = to_wide(&cmd);
                RegSetValueExW(
                    hkey,
                    val_w.as_ptr(),
                    0,
                    REG_SZ,
                    cmd_w.as_ptr() as *const u8,
                    (cmd_w.len() * 2) as u32,
                ) == 0
            } else {
                false
            }
        } else {
            RegDeleteValueW(hkey, val_w.as_ptr());
            true
        };

        RegCloseKey(hkey);
        success
    }
}
