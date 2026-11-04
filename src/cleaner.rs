use crate::types::CleanTier;
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;
use windows_sys::Win32::UI::Shell::{
    SHEmptyRecycleBinW, SHQueryRecycleBinW, SHERB_NOCONFIRMATION, SHERB_NOPROGRESSUI,
    SHERB_NOSOUND, SHQUERYRBINFO,
};

pub struct CleanTarget {
    pub name: &'static str,
    pub path_desc: &'static str,
    pub path: Option<PathBuf>,
    pub extra_paths: Vec<PathBuf>,
    pub is_recycle_bin: bool,
    pub tier: CleanTier,
}

impl CleanTarget {
    pub fn all_paths(&self) -> Vec<&PathBuf> {
        let mut paths = Vec::new();
        if let Some(ref p) = self.path {
            paths.push(p);
        }
        for p in &self.extra_paths {
            paths.push(p);
        }
        paths
    }
}

pub fn get_default_targets() -> Vec<CleanTarget> {
    let local_app_data = env::var("LOCALAPPDATA").ok().map(PathBuf::from);
    let app_data = env::var("APPDATA").ok().map(PathBuf::from);
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

    // Developer Cache targets (NPM, Yarn, PNPM, Cargo, Go, Pip, UV, NuGet, Gradle)
    let npm_local = local_app_data.as_ref().map(|p| p.join("npm-cache"));
    let npm_roaming = app_data.as_ref().map(|p| p.join("npm-cache"));
    let pnpm_cache = local_app_data.as_ref().map(|p| p.join(r"pnpm\store"));
    let yarn_cache = local_app_data.as_ref().map(|p| p.join(r"Yarn\Cache"));

    let cargo_cache = user_profile
        .as_ref()
        .map(|p| p.join(r".cargo\registry\cache"));
    let cargo_src = user_profile
        .as_ref()
        .map(|p| p.join(r".cargo\registry\src"));
    let cargo_git = user_profile
        .as_ref()
        .map(|p| p.join(r".cargo\git\checkouts"));

    let go_build = local_app_data.as_ref().map(|p| p.join("go-build"));
    let go_mod_cache = user_profile
        .as_ref()
        .map(|p| p.join(r"go\pkg\mod\cache"));

    let pip_cache = local_app_data.as_ref().map(|p| p.join(r"pip\cache"));
    let uv_cache = local_app_data.as_ref().map(|p| p.join(r"uv\cache"));

    let nuget_cache = local_app_data.as_ref().map(|p| p.join(r"NuGet\v3-cache"));
    let nuget_packages = user_profile.as_ref().map(|p| p.join(r".nuget\packages"));

    let gradle_caches = user_profile.as_ref().map(|p| p.join(r".gradle\caches"));

    vec![
        // --- TIER 1: RÁC AN TOÀN 100% ---
        CleanTarget {
            name: "User Temp (%TEMP%)",
            path_desc: "File tạm thời của các ứng dụng người dùng",
            path: Some(env::temp_dir()),
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "System Temp",
            path_desc: "%SystemRoot%\\Temp (File tạm hệ điều hành)",
            path: Some(sys_path.join("Temp")),
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Thùng rác (Recycle Bin)",
            path_desc: "Tất cả các file tạm đã xóa vào thùng rác hệ thống",
            path: None,
            extra_paths: Vec::new(),
            is_recycle_bin: true,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Crash Dumps & Error Reports",
            path_desc: "File nhật ký sự cố và dump bộ nhớ ứng dụng",
            path: crash_dumps,
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Edge Browser Cache",
            path_desc: "Bộ nhớ đệm hình ảnh, script của Microsoft Edge",
            path: edge_cache,
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        CleanTarget {
            name: "Chrome Browser Cache",
            path_desc: "Bộ nhớ đệm web của Google Chrome",
            path: chrome_cache,
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Safe,
        },
        // --- TIER 2: RÁC LẬP TRÌNH VIÊN (DEV CACHE) ---
        CleanTarget {
            name: "Node.js (NPM, Yarn, PNPM) Cache",
            path_desc: "%LOCALAPPDATA%\\npm-cache, yarn, pnpm store",
            path: npm_local,
            extra_paths: [npm_roaming, yarn_cache, pnpm_cache]
                .into_iter()
                .flatten()
                .collect(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Go Build & Module Cache",
            path_desc: "%LOCALAPPDATA%\\go-build & %USERPROFILE%\\go\\pkg\\mod",
            path: go_build,
            extra_paths: [go_mod_cache].into_iter().flatten().collect(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Cargo Crates & Sources Cache",
            path_desc: "%USERPROFILE%\\.cargo (Download cache, src & git)",
            path: cargo_cache,
            extra_paths: [cargo_src, cargo_git].into_iter().flatten().collect(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Python Pip & UV Cache",
            path_desc: "%LOCALAPPDATA%\\pip\\cache & uv\\cache",
            path: pip_cache,
            extra_paths: [uv_cache].into_iter().flatten().collect(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: ".NET / NuGet Package Cache",
            path_desc: "%USERPROFILE%\\.nuget\\packages & NuGet v3 cache",
            path: nuget_cache,
            extra_paths: [nuget_packages].into_iter().flatten().collect(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        CleanTarget {
            name: "Gradle & Android Build Cache",
            path_desc: "%USERPROFILE%\\.gradle\\caches",
            path: gradle_caches,
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        },
        // --- TIER 3: BẢO TRÌ HỆ THỐNG SÂU ---
        CleanTarget {
            name: "Windows Update Cache",
            path_desc: "%SystemRoot%\\SoftwareDistribution\\Download (Bản vá đã cài)",
            path: Some(sys_path.join(r"SoftwareDistribution\Download")),
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::SystemMaintenance,
        },
        CleanTarget {
            name: "Windows Prefetch",
            path_desc: "%SystemRoot%\\Prefetch (Cache khởi động ứng dụng cũ)",
            path: Some(sys_path.join("Prefetch")),
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::SystemMaintenance,
        },
    ]
}

fn safe_remove_file(p: &Path) -> std::io::Result<u64> {
    let meta = fs::symlink_metadata(p)?;
    let size = meta.len();
    let mut perms = meta.permissions();
    if perms.readonly() {
        perms.set_readonly(false);
        let _ = fs::set_permissions(p, perms);
    }
    fs::remove_file(p)?;
    Ok(size)
}

fn cleanup_empty_dirs(root: &Path) {
    let mut dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .max_depth(30)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.into_path())
        .collect();

    // Sort descending by depth so innermost directories are removed first
    dirs.sort_by(|a, b| b.components().count().cmp(&a.components().count()));

    for dir in dirs {
        let perms = fs::metadata(&dir).map(|m| m.permissions()).ok();
        if let Some(mut p) = perms {
            if p.readonly() {
                p.set_readonly(false);
                let _ = fs::set_permissions(&dir, p);
            }
        }
        let _ = fs::remove_dir(&dir);
    }
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

    let mut total_size = 0u64;
    let mut file_count = 0usize;

    for path in target.all_paths() {
        if !path.exists() {
            continue;
        }

        for entry in WalkDir::new(path)
            .max_depth(30)
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

    let mut freed_bytes = 0u64;
    let mut deleted_files = 0usize;

    for path in target.all_paths() {
        if !path.exists() {
            continue;
        }

        let entries: Vec<_> = WalkDir::new(path)
            .max_depth(30)
            .follow_links(false)
            .into_iter()
            .filter_map(|e| e.ok())
            .collect();

        for entry in &entries {
            if entry.file_type().is_file() || entry.file_type().is_symlink() {
                let p = entry.path();
                if let Ok(size) = safe_remove_file(p) {
                    freed_bytes += size;
                    deleted_files += 1;
                }
            }
        }

        cleanup_empty_dirs(path);
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
            extra_paths: Vec::new(),
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

    #[test]
    fn test_deep_and_readonly_dev_cache_cleaning() {
        let temp_base = env::temp_dir().join("omnitune_test_deep_cache");
        let deep_dir = temp_base.join("l1").join("l2").join("l3").join("l4").join("l5").join("l6").join("l7");
        let _ = fs::create_dir_all(&deep_dir);
        let deep_file = deep_dir.join("cached_package.bin");
        let _ = fs::write(&deep_file, b"deep nested cache content");

        // Set file as read-only (common in Windows dev caches like Go/Cargo)
        let mut perms = fs::metadata(&deep_file).unwrap().permissions();
        perms.set_readonly(true);
        let _ = fs::set_permissions(&deep_file, perms);

        let target = CleanTarget {
            name: "Deep Test",
            path_desc: "Deep Test",
            path: Some(temp_base.clone()),
            extra_paths: Vec::new(),
            is_recycle_bin: false,
            tier: CleanTier::Developer,
        };

        let (size, count) = scan_target(&target);
        assert_eq!(count, 1, "Should find file at depth > 5");
        assert_eq!(size, 25);

        let (freed, deleted) = clean_target(&target);
        assert_eq!(deleted, 1, "Should delete read-only file at depth > 5");
        assert_eq!(freed, 25);
        assert!(!deep_file.exists());

        let _ = fs::remove_dir_all(&temp_base);
    }
}
