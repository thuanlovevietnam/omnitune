use sha2::{Digest, Sha256};
use std::env;
use std::fs;
use std::path::PathBuf;

const LICENSE_SALT: &str = "OMNITUNE_PRO_AUTONOMOUS_2026_THUAN_VIETNAM";
const MASTER_KEY: &str = "OTPRO-LIFETIME-VIP-2026";

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct LicenseData {
    pub licensee: String,
    pub key: String,
    pub is_valid: bool,
}

impl Default for LicenseData {
    fn default() -> Self {
        Self {
            licensee: String::new(),
            key: String::new(),
            is_valid: false,
        }
    }
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

pub fn verify_license(email: &str, key: &str) -> bool {
    let key_trimmed = key.trim().to_uppercase();
    if key_trimmed == MASTER_KEY {
        return true;
    }

    let expected = generate_key_for(email);
    key_trimmed == expected
}

pub fn load_license() -> LicenseData {
    let path = get_license_file_path();
    if let Ok(data) = fs::read_to_string(path) {
        if let Ok(lic) = serde_json::from_str::<LicenseData>(&data) {
            if verify_license(&lic.licensee, &lic.key) {
                return LicenseData {
                    licensee: lic.licensee,
                    key: lic.key,
                    is_valid: true,
                };
            }
        }
    }
    LicenseData::default()
}

pub fn save_license(email: &str, key: &str) -> bool {
    if !verify_license(email, key) {
        return false;
    }

    let data = LicenseData {
        licensee: email.trim().to_string(),
        key: key.trim().to_uppercase(),
        is_valid: true,
    };

    let path = get_license_file_path();
    if let Ok(json) = serde_json::to_string_pretty(&data) {
        return fs::write(path, json).is_ok();
    }
    false
}

pub fn deactivate_license() {
    let path = get_license_file_path();
    let _ = fs::remove_file(path);
}
