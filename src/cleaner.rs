use crate::types::CleanTier;
use std::env;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND, SHQUERYRBINFO,
};

pub struct CleanTarget {
    pub name: &'static str,
    pub path_desc: &'static str,
    pub path: Option<PathBuf>,
    pub is_recycle_bin: bool,
    pub tier: CleanTier,
}

pub fn get_default_targets() -> Vec<CleanTarget> {
    let local_app_data = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let user_profile = env::var("USERPROFILE").ok().map(PathBuf::from);
    let system_root = env::var("SystemRoot")
        .or_else(|_| env::var("windir"))
        .unwrap_or_else(|_| r"C:\Windows".to_string());
    let sys_path = PathBuf::from(&system_root);

    let edge_cache = local_app_data
        .as_ref()
        .map(|p| p.join(r"Microsoft\Edge\User Data\Default\Cache"));
    let chrome_cache = local_app_data
        .as_ref()
        .map(|p| p.join(r"Google\Chrome\User Data\Default\Cache"));
    let crash_dumps = local_app_data.as_ref().map(|p| p.join("CrashDumps"));

    // Developer Cache targets
    let npm_cache = local_app_data.as_ref().map(|p| p.join("npm-cache"));
    let go_build = local_app_data.as_ref().map(|p| p.join("go-build"));
    let pip_cache = local_app_data.as_ref().map(|p| p.join(r"pip\cache"));
    let cargo_cache = user_profile
        .as_ref()
        .map(|p| p.join(r".cargo\registry\cache"));

    vec![
        // --- TIER 1: RÁC AN TOÀN 100% ---
        CleanTarget {
            name: "User Temp (%TEMP%)",
            path_desc: "File tạm thời của các ứng dụng người dùng",
            path: Some(env::temp_dir()),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "System Temp",
            path_desc: "%SystemRoot%\\Temp (File tạm hệ điều hành)",
            path: Some(sys_path.join("Temp")),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Thùng rác (Recycle Bin)",
            path_desc: "Tất cả các file tạm đã xóa vào thùng rác hệ thống",
            path: None,
            is_recycle_bin: true,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Crash Dumps & Error Reports",
            path_desc: "File nhật ký sự cố và dump bộ nhớ ứng dụng",
            path: crash_dumps,
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Edge Browser Cache",
            path_desc: "Bộ nhớ đệm hình ảnh, script của Microsoft Edge",
            path: edge_cache,
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Chrome Browser Cache",
            path_desc: "Bộ nhớ đệm web của Google Chrome",
            path: chrome_cache,
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        // --- TIER 2: RÁC LẬP TRÌNH VIÊN (DEV CACHE) ---
        CleanTarget {
            name: "Node.js / NPM Cache",
            path_desc: "%LOCALAPPDATA%\\npm-cache (Dung lượng lớn)",
            path: npm_cache,
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Go Build & Module Cache",
            path_desc: "%LOCALAPPDATA%\\go-build (Cache build Golang)",
            path: go_build,
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Cargo Crates Download Cache",
            path_desc: "%USERPROFILE%\\.cargo\\registry\\cache",
            path: cargo_cache,
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Python Pip Cache",
            path_desc: "%LOCALAPPDATA%\\pip\\cache (Cache package pip)",
            path: pip_cache,
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        // --- TIER 3: BẢO TRÌ HỆ THỐNG SÂU ---
        CleanTarget {
            name: "Windows Update Cache",
            path_desc: "%SystemRoot%\\SoftwareDistribution\\Download (Bản vá đã cài)",
            path: Some(sys_path.join(r"SoftwareDistribution\Download")),
            is_recycle_bin: false,
            tier: CleanTier::SystemMaintenance,
        },
        CleanTarget {
            name: "Windows Prefetch",
            path_desc: "%SystemRoot%\\Prefetch (Cache khởi động ứng dụng cũ)",
            path: Some(sys_path.join("Prefetch")),
            is_recycle_bin: false,
            tier: CleanTier::SystemMaintenance,
        },
    ]
}

pub fn scan_target(target: &CleanTarget) -> (u64, usize) {
    if target.is_recycle_bin {
        unsafe {
            let mut info = SHQUERYRBINFO {
                cbSize: std::mem::size_of::<SHQUERYRBINFO>() as u32,
                i64Size: 0,
                i64NumItems: 0,
            };
            if SHQueryRecycleBinW(std::ptr::null(), &mut info) == 0 {
                return (info.i64Size.max(0) as u64, info.i64NumItems.max(0) as usize);
            }
        }
        return (0, 0);
    }

    let Some(ref path) = target.path else {
        return (0, 0);
    };

    if !path.exists() {
        return (0, 0);
    }

    let mut total_size = 0u64;
    let mut file_count = 0usize;

    for entry in WalkDir::new(path)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                total_size += meta.len();
                file_count += 1;
            }
        }
    }

    (total_size, file_count)
}

pub fn clean_target(target: &CleanTarget) -> (u64, usize) {
    if target.is_recycle_bin {
        let (before_size, before_count) = scan_target(target);
        unsafe {
            SHEmptyRecycleBinW(
                std::ptr::null_mut(),
                std::ptr::null(),
                SHERB_NOCONFIRMATION | SHERB_NOPROGRESSUI | SHERB_NOSOUND,
            );
        }
        return (before_size, before_count);
    }

    let Some(ref path) = target.path else {
        return (0, 0);
    };

    if !path.exists() {
        return (0, 0);
    }

    let mut freed_bytes = 0u64;
    let mut deleted_files = 0usize;

    let entries: Vec<_> = WalkDir::new(path)
        .max_depth(5)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .collect();

    for entry in entries {
        let p = entry.path();
        if entry.file_type().is_file() {
            if let Ok(meta) = entry.metadata() {
                let size = meta.len();
                if fs::remove_file(p).is_ok() {
                    freed_bytes += size;
                    deleted_files += 1;
                }
            }
        }
    }

    (freed_bytes, deleted_files)
}

pub fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_clean_tiers_coverage() {
        let targets = get_default_targets();
        assert!(targets.iter().any(|t| t.tier == CleanTier::Safe));
        assert!(targets.iter().any(|t| t.tier == CleanTier::Developer));
        assert!(targets.iter().any(|t| t.tier == CleanTier::SystemMaintenance));

        let sys_maint_targets: Vec<_> = targets
            .into_iter()
            .filter(|t| t.tier == CleanTier::SystemMaintenance)
            .collect();
        assert_eq!(sys_maint_targets.len(), 2);
        assert!(sys_maint_targets.iter().any(|t| t.name == "Windows Update Cache"));
        assert!(sys_maint_targets.iter().any(|t| t.name == "Windows Prefetch"));

        for target in sys_maint_targets {
            assert!(target.path.is_some());
            let path = target.path.unwrap();
            assert!(path.to_string_lossy().contains("SoftwareDistribution") || path.to_string_lossy().contains("Prefetch"));
        }
    }

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(500), "500 B");
        assert_eq!(format_bytes(1024), "1.0 KB");
        assert_eq!(format_bytes(1024 * 1024 * 5), "5.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn test_scan_and_clean_temp_target() {
        let temp_dir = env::temp_dir().join("omnitune_test_clean_dummy");
        let _ = fs::create_dir_all(&temp_dir);
        let dummy_file = temp_dir.join("test.tmp");
        let _ = fs::write(&dummy_file, b"hello test cleaner");

        let target = CleanTarget {
            name: "Test Dummy",
            path_desc: "Test",
            path: Some(temp_dir.clone()),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        };

        let (size, count) = scan_target(&target);
        assert!(count >= 1);
        assert!(size >= 18);

        let (freed, cleaned) = clean_target(&target);
        assert!(cleaned >= 1);
        assert!(freed >= 18);

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
