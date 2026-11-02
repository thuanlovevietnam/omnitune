use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

const LICENSE_SALT: &str = "OMNITUNE_PRO_AUTONOMOUS_2026_THUAN_VIETNAM";
const TRIAL_DURATION_SECS: u64 = 7 * 24 * 3600; // 7 days (168 hours)

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrialStatus {
    NotStarted,
    Active { remaining_secs: u64 },
    Expired,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LicenseData {
    pub licensee: String,
    pub key: String,
    #[serde(default)]
    pub hwid: Option<String>,
    pub is_valid: bool,
    #[serde(default)]
    pub trial_start_timestamp: Option<u64>,
}

impl Default for LicenseData {
    fn default() -> Self {
        Self {
            licensee: String::new(),
            key: String::new(),
            hwid: None,
            is_valid: false,
            trial_start_timestamp: None,
        }
    }
}

pub fn get_hardware_id() -> String {
    let raw_id = crate::reg::reg_read_string(
        windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE,
        r"SOFTWARE\Microsoft\Cryptography",
        "MachineGuid",
    )
    .unwrap_or_else(|| {
        let comp = env::var("COMPUTERNAME").unwrap_or_else(|_| "DEFAULT_PC".into());
        let user = env::var("USERNAME").unwrap_or_else(|_| "USER".into());
        format!("{}-{}", comp, user)
    });

    let mut hasher = Sha256::new();
    hasher.update(raw_id.as_bytes());
    let hash: String = hasher
        .finalize()
        .into_iter()
        .map(|b| format!("{:02X}", b))
        .collect();

    format!("{}-{}", &hash[0..4], &hash[4..8])
}

fn get_license_file_path() -> PathBuf {
    let app_data = env::var("APPDATA").unwrap_or_else(|_| "C:\\Users\\Default\\AppData\\Roaming".to_string());
    let dir = PathBuf::from(app_data).join("OmniTune");
    let _ = fs::create_dir_all(&dir);
    dir.join("license.json")
}

pub fn generate_key_for(email: &str) -> String {
    let normalized = email.trim().to_lowercase();
    let input = format!("{}:{}", LICENSE_SALT, normalized);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash: String = hasher
        .finalize()
        .into_iter()
        .map(|b| format!("{:02X}", b))
        .collect();

    // Format: OTPRO-XXXX-XXXX-XXXX
    let part1 = &hash[0..4];
    let part2 = &hash[4..8];
    let part3 = &hash[8..12];
    format!("OTPRO-{}-{}-{}", part1, part2, part3)
}

pub fn generate_key_for_hwid(email: &str, hwid: &str) -> String {
    let normalized_email = email.trim().to_lowercase();
    let normalized_hwid = hwid.trim().to_uppercase();
    let input = format!("{}:{}:{}", LICENSE_SALT, normalized_email, normalized_hwid);
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let hash: String = hasher
        .finalize()
        .into_iter()
        .map(|b| format!("{:02X}", b))
        .collect();

    // Format: OTHW-XXXX-XXXX-XXXX
    let part1 = &hash[0..4];
    let part2 = &hash[4..8];
    let part3 = &hash[8..12];
    format!("OTHW-{}-{}-{}", part1, part2, part3)
}

pub fn verify_license(email: &str, key: &str) -> bool {
    let key_trimmed = key.trim().to_uppercase();
    if key_trimmed.is_empty() || email.trim().is_empty() {
        return false;
    }

    let hwid = get_hardware_id();
    let expected_email = generate_key_for(email);
    let expected_hwid = generate_key_for_hwid(email, &hwid);

    key_trimmed == expected_email || key_trimmed == expected_hwid
}

pub fn get_trial_status(lic: &LicenseData) -> TrialStatus {
    if let Some(start) = lic.trial_start_timestamp {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        if now >= start && (now - start) < TRIAL_DURATION_SECS {
            TrialStatus::Active {
                remaining_secs: TRIAL_DURATION_SECS - (now - start),
            }
        } else {
            TrialStatus::Expired
        }
    } else {
        TrialStatus::NotStarted
    }
}

pub fn start_trial() -> LicenseData {
    let mut lic = load_license();
    if lic.trial_start_timestamp.is_none() {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        lic.trial_start_timestamp = Some(now);
        let path = get_license_file_path();
        if let Ok(json) = serde_json::to_string_pretty(&lic) {
            let _ = fs::write(path, json);
        }
    }
    lic
}

pub fn is_pro_active(lic: &LicenseData) -> bool {
    if lic.is_valid {
        return true;
    }
    matches!(get_trial_status(lic), TrialStatus::Active { .. })
}

pub fn format_remaining_trial(remaining_secs: u64) -> String {
    let days = remaining_secs / 86400;
    let hours = (remaining_secs % 86400) / 3600;
    let mins = (remaining_secs % 3600) / 60;
    if days > 0 {
        format!("{} ngày {} giờ", days, hours)
    } else if hours > 0 {
        format!("{} giờ {} phút", hours, mins)
    } else {
        format!("{} phút", mins.max(1))
    }
}

pub fn load_license() -> LicenseData {
    let path = get_license_file_path();
    if let Ok(data) = fs::read_to_string(&path) {
        if let Ok(mut lic) = serde_json::from_str::<LicenseData>(&data) {
            lic.is_valid = verify_license(&lic.licensee, &lic.key);
            lic.hwid = Some(get_hardware_id());
            return lic;
        }
    }
    LicenseData::default()
}

pub fn save_license(email: &str, key: &str) -> bool {
    if !verify_license(email, key) {
        return false;
    }

    let mut current = load_license();
    current.licensee = email.trim().to_string();
    current.key = key.trim().to_uppercase();
    current.hwid = Some(get_hardware_id());
    current.is_valid = true;

    let path = get_license_file_path();
    if let Ok(json) = serde_json::to_string_pretty(&current) {
        return fs::write(path, json).is_ok();
    }
    false
}

pub fn deactivate_license() {
    let mut current = load_license();
    current.licensee.clear();
    current.key.clear();
    current.is_valid = false;
    let path = get_license_file_path();
    if let Ok(json) = serde_json::to_string_pretty(&current) {
        let _ = fs::write(path, json);
    }
}
