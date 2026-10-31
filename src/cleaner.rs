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
            path_desc: "C:\\Windows\\Temp (File tạm hệ điều hành)",
            path: Some(PathBuf::from(r"C:\Windows\Temp")),
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
            path_desc: "C:\\Windows\\SoftwareDistribution\\Download (Bản vá đã cài)",
            path: Some(PathBuf::from(r"C:\Windows\SoftwareDistribution\Download")),
            is_recycle_bin: false,
            tier: CleanTier::SystemMaintenance,
        },
        CleanTarget {
            name: "Windows Prefetch",
            path_desc: "C:\\Windows\\Prefetch (Cache khởi động ứng dụng cũ)",
            path: Some(PathBuf::from(r"C:\Windows\Prefetch")),
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
        .max_depth(4)
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
