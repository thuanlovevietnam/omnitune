use sysinfo::{DiskKind, Disks, System};
use windows_sys::Win32::System::Registry::HKEY_LOCAL_MACHINE;
use windows_sys::Win32::System::SystemInformation::GetTickCount;
use std::os::windows::process::CommandExt;
use std::process::Command;

#[repr(C)]
struct WinLastInputInfo {
    cb_size: u32,
    dw_time: u32,
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetLastInputInfo(plii: *mut WinLastInputInfo) -> i32;
}

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, PartialEq)]
pub struct HardwareProfile {
    pub os_name: String,
    pub os_version: String,
    pub os_build: u32,
    pub is_windows_11: bool,
    pub cpu_brand: String,
    pub cpu_logical_cores: usize,
    pub cpu_physical_cores: usize,
    pub cpu_base_mhz: u32,
    pub ram_total_bytes: u64,
    pub ram_total_gb: f32,
    pub dynamic_ram_threshold_pct: u32,
    pub is_ssd: bool,
    pub disk_name: String,
    pub disk_total_bytes: u64,
    pub disk_available_bytes: u64,
    pub disk_fs: String,
    pub gpu_name: String,
    pub is_dedicated_gpu: bool,
    pub is_laptop: bool,
    pub optimal_cpu_dc_limit: u32,
    pub optimal_cpu_ac_limit: u32,
}

impl HardwareProfile {
    pub fn detect(sys: &mut System) -> Self {
        sys.refresh_cpu_all();
        sys.refresh_memory();

        // 1. OS Info
        let (os_name, os_version, os_build, is_windows_11) = detect_os();

        // 2. CPU Info
        let cpu_logical_cores = sys.cpus().len().max(1);
        let cpu_physical_cores = sysinfo::System::physical_core_count().unwrap_or(cpu_logical_cores);
        let cpu_brand = detect_cpu_brand(sys);
        let cpu_base_mhz = detect_cpu_mhz(sys);

        // 3. RAM Info
        let ram_total_bytes = sys.total_memory();
        let ram_total_gb = (ram_total_bytes as f64 / (1024.0 * 1024.0 * 1024.0)) as f32;
        let dynamic_ram_threshold_pct = if ram_total_gb <= 8.5 {
            75
        } else if ram_total_gb <= 16.5 {
            80
        } else if ram_total_gb <= 32.5 {
            85
        } else {
            88
        };

        // 4. Disk Info
        let (is_ssd, disk_name, disk_total_bytes, disk_available_bytes, disk_fs) = detect_disk();

        // 5. GPU Info
        let (gpu_name, is_dedicated_gpu) = detect_gpu();

        // 6. Form Factor (Laptop vs Desktop)
        let power_status = crate::power::get_power_status();
        let is_laptop = power_status.has_battery;

        // 7. Dynamic CPU power limits (No hardcoding)
        let (optimal_cpu_dc_limit, optimal_cpu_ac_limit) = if !is_laptop {
            (100, 100) // Desktop: Full power
        } else if cpu_logical_cores <= 4 {
            (80, 100) // 2-4 cores: 80% on battery
        } else if cpu_logical_cores <= 8 {
            (70, 100) // 6-8 cores: 70% on battery
        } else {
            (65, 100) // >8 cores: 65% on battery
        };

        Self {
            os_name,
            os_version,
            os_build,
            is_windows_11,
            cpu_brand,
            cpu_logical_cores,
            cpu_physical_cores,
            cpu_base_mhz,
            ram_total_bytes,
            ram_total_gb,
            dynamic_ram_threshold_pct,
            is_ssd,
            disk_name,
            disk_total_bytes,
            disk_available_bytes,
            disk_fs,
            gpu_name,
            is_dedicated_gpu,
            is_laptop,
            optimal_cpu_dc_limit,
            optimal_cpu_ac_limit,
        }
    }

    pub fn apply_hardware_optimal(&self, is_ac: bool, battery_pct: u8) -> Vec<String> {
        let mut applied = Vec::new();

        // 1. OS-specific tweaks
        crate::power::apply_permanent_windows_system_tweaks();

        // MMCSS Network responsiveness tweak (disable network throttling index)
        crate::power::set_mmcss_gaming_responsiveness(true);
        crate::power::set_system_responsiveness(true);
        applied.push(format!(
            "OS {}: Ưu tiên phản hồi luồng giao diện (Win32PrioritySeparation=38, MMCSS NoThrottle).",
            self.os_name
        ));

        // 2. Storage Tweak (SSD vs HDD)
        if self.is_ssd {
            // SSD: Disable SysMain/SuperFetch & Disable NTFS LastAccessUpdate
            crate::reg::reg_set_u32(
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Services\SysMain",
                "Start",
                4, // Disabled
            );
            let _ = Command::new("sc")
                .args(["stop", "SysMain"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            let _ = Command::new("fsutil")
                .args(["behavior", "set", "disablelastaccess", "1"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            applied.push(format!(
                "Ổ cứng SSD [{}]: Tắt SysMain & giảm ghi rác NTFS LastAccess để tối ưu tuổi thọ và độ trễ NAND.",
                self.disk_name
            ));
        } else {
            // HDD: Enable SysMain for fast rotational prefetching
            crate::reg::reg_set_u32(
                HKEY_LOCAL_MACHINE,
                r"SYSTEM\CurrentControlSet\Services\SysMain",
                "Start",
                2, // Automatic
            );
            let _ = Command::new("sc")
                .args(["start", "SysMain"])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
            applied.push(format!(
                "Ổ cứng HDD [{}]: Kích hoạt SysMain tối ưu hóa bộ nhớ đệm đọc đĩa quay.",
                self.disk_name
            ));
        }

        // 3. CPU & Power Policy (Dynamic according to hardware & power source)
        if !self.is_laptop {
            // Desktop workstation
            let _ = crate::power::set_power_scheme("SCHEME_MIN"); // High performance
            let _ = crate::power::set_cpu_max_state_ac(100);
            let _ = crate::power::set_cooling_policy(true);
            applied.push(format!(
                "Desktop Workstation: Mở 100% hiệu năng toàn bộ {} nhân ({}). Quạt tản nhiệt chủ động.",
                self.cpu_logical_cores, self.cpu_brand
            ));
        } else if is_ac {
            // Laptop on AC
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_ac(self.optimal_cpu_ac_limit);
            let _ = crate::power::set_cooling_policy(true);
            applied.push(format!(
                "Laptop cắm sạc (AC): Cấp 100% xung nhịp tối đa cho {} nhân thực tế, làm mát chủ động.",
                self.cpu_logical_cores
            ));
        } else {
            // Laptop on Battery (DC) - dynamic limit based on battery level & core count
            let dc_dynamic = (self.optimal_cpu_dc_limit as f32
                * (0.65 + 0.35 * (battery_pct as f32 / 100.0)))
                .round() as u32;
            let final_dc = dc_dynamic.clamp(50, self.optimal_cpu_dc_limit);
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED"); // Giữ Balanced để không giảm độ sáng màn hình
            let _ = crate::power::set_cpu_max_state_dc(final_dc);
            let active_cool = battery_pct > 50;
            let _ = crate::power::set_cooling_policy(active_cool);
            applied.push(format!(
                "Laptop chạy pin (DC {}%): Điều tiết CPU {} nhân trần {}% để kéo dài thời lượng, quạt {}.",
                battery_pct,
                self.cpu_logical_cores,
                final_dc,
                if active_cool { "chủ động" } else { "yên tĩnh (Passive)" }
            ));
        }

        // 4. GPU Optimization
        let gpu_msgs = crate::gpu::apply_hardware_gpu_optimal(self, is_ac, battery_pct);
        for m in gpu_msgs {
            applied.push(m);
        }

        // 5. RAM Watchdog policy
        applied.push(format!(
            "RAM ({:.1} GB): Đặt ngưỡng tự động thu hồi động ở mức {}% (phù hợp dung lượng máy).",
            self.ram_total_gb, self.dynamic_ram_threshold_pct
        ));

        applied
    }
}

fn detect_os() -> (String, String, u32, bool) {
    let subkey = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion";
    let prod_name = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, subkey, "ProductName")
        .unwrap_or_else(|| "Windows".to_string());
    let display_ver = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, subkey, "DisplayVersion")
        .unwrap_or_else(|| "".to_string());
    let build_str = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, subkey, "CurrentBuild")
        .or_else(|| crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, subkey, "CurrentBuildNumber"))
        .unwrap_or_else(|| "0".to_string());

    let build_num: u32 = build_str.parse().unwrap_or(0);
    let is_win11 = build_num >= 22000;

    let full_name = if is_win11 && prod_name.contains("Windows 10") {
        prod_name.replace("Windows 10", "Windows 11")
    } else {
        prod_name
    };

    let formatted_os = if !display_ver.is_empty() {
        format!("{} ({}, Build {})", full_name, display_ver, build_num)
    } else {
        format!("{} (Build {})", full_name, build_num)
    };

    (formatted_os, display_ver, build_num, is_win11)
}

fn detect_cpu_brand(sys: &System) -> String {
    // Try registry first for complete branded name
    let subkey = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
    if let Some(brand) = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, subkey, "ProcessorNameString") {
        let trimmed = brand.trim().to_string();
        if !trimmed.is_empty() {
            return trimmed;
        }
    }
    // Fall back to sysinfo
    if let Some(cpu) = sys.cpus().first() {
        let b = cpu.brand().trim();
        if !b.is_empty() {
            return b.to_string();
        }
    }
    "Generic Processor".to_string()
}

fn detect_cpu_mhz(sys: &System) -> u32 {
    let subkey = r"HARDWARE\DESCRIPTION\System\CentralProcessor\0";
    if let Some(mhz) = crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, subkey, "~MHz") {
        if mhz > 0 {
            return mhz;
        }
    }
    if let Some(cpu) = sys.cpus().first() {
        return cpu.frequency() as u32;
    }
    2000
}

fn detect_disk() -> (bool, String, u64, u64, String) {
    let mut disk_name = crate::reg::get_physical_disk_name_native();
    let mut is_ssd = false;
    let mut disk_total_bytes = 0u64;
    let mut disk_available_bytes = 0u64;
    let mut disk_fs = "NTFS".to_string();

    // Check disk name keywords
    let upper = disk_name.to_uppercase();
    if upper.contains("SSD") || upper.contains("NVME") || upper.contains("OPTANE") || upper.contains("FLASH") {
        is_ssd = true;
    }

    // Inspect via sysinfo::Disks
    if let Ok(disks) = Disks::new_with_refreshed_list() {
        for d in &disks {
            if d.kind() == DiskKind::SSD {
                is_ssd = true;
            }
            let name = d.name().to_string_lossy().to_string();
            if !name.is_empty() && disk_name.is_empty() {
                disk_name = name;
            }
            disk_total_bytes += d.total_space();
            disk_available_bytes += d.available_space();
            let fs = d.file_system().to_string_lossy().to_string();
            if !fs.is_empty() {
                disk_fs = fs;
            }
        }
    }

    if disk_name.is_empty() {
        disk_name = if is_ssd { "Ổ cứng SSD".to_string() } else { "Ổ cứng HDD".to_string() };
    }

    (is_ssd, disk_name, disk_total_bytes, disk_available_bytes, disk_fs)
}

fn detect_gpu() -> (String, bool) {
    let base_key = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut subkeys = crate::reg::reg_enum_subkeys(HKEY_LOCAL_MACHINE, base_key);
    if subkeys.is_empty() {
        subkeys = vec!["0000".to_string(), "0001".to_string(), "0002".to_string(), "0003".to_string()];
    }
    let mut found_gpus = Vec::new();

    for sub in subkeys {
        if !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let key_path = format!(r"{}\{}", base_key, sub);
        if let Some(desc) = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, &key_path, "DriverDesc") {
            let trimmed = desc.trim().to_string();
            let upper = trimmed.to_uppercase();
            // Lọc các display adapter ảo
            if upper.contains("SPACEDESK") || upper.contains("VIRTUAL") || upper.contains("MIRACAST") || upper.contains("CITRIX") {
                continue;
            }
            if !trimmed.is_empty() && !found_gpus.contains(&trimmed) {
                found_gpus.push(trimmed);
            }
        }
    }

    if found_gpus.is_empty() {
        return ("Đồ họa cơ bản Windows".to_string(), false);
    }

    // Find if dedicated GPU exists
    let dedicated_keywords = [
        "NVIDIA", "GEFORCE", "RTX", "GTX", "QUADRO", "TITAN", "TESLA",
        "RADEON RX", "RADEON PRO", "RADEON VII", "ARC A", "ARC PRO",
    ];
    let mut dedicated = false;
    let mut primary_gpu = found_gpus[0].clone();

    for g in &found_gpus {
        let g_upper = g.to_uppercase();
        if dedicated_keywords.iter().any(|&k| g_upper.contains(k)) {
            dedicated = true;
            primary_gpu = g.clone();
            break;
        }
    }

    if found_gpus.len() > 1 && !dedicated {
        primary_gpu = found_gpus.join(" + ");
    }

    (primary_gpu, dedicated)
}

pub fn get_user_idle_ms() -> u32 {
    unsafe {
        let mut lii = WinLastInputInfo {
            cb_size: std::mem::size_of::<WinLastInputInfo>() as u32,
            dw_time: 0,
        };
        if GetLastInputInfo(&mut lii) != 0 {
            let tick = GetTickCount();
            return tick.saturating_sub(lii.dw_time);
        }
    }
    0
}
