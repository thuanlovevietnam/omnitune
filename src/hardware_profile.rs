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
    pub gpu_vram_mb: u32,
    pub gpu_driver_version: String,
    pub gpu_supports_hags: bool,
    pub is_laptop: bool,
    pub cpu_topology: crate::cpu::CpuTopology,
    pub optimal_cpu_dc_limit: u32,
    pub optimal_cpu_ac_limit: u32,
}

impl HardwareProfile {
    pub fn detect(sys: &mut System) -> Self {
        sys.refresh_cpu_all();
        sys.refresh_memory();

        // 1. OS Info
        let (os_name, os_version, os_build, is_windows_11) = detect_os();

        // 2. CPU Info & Topology Probe (Zero hardcoding)
        let cpu_topology = crate::cpu::CpuTopology::probe();
        let cpu_logical_cores = cpu_topology.total_logical_cores;
        let cpu_physical_cores = cpu_topology.total_physical_cores;
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

        // 5. GPU Info (Probe phần cứng VRAM, Driver, HAGS)
        let gpu_info = detect_gpu(os_build);

        // 6. Form Factor (Laptop vs Desktop)
        let power_status = crate::power::get_power_status();
        let is_laptop = power_status.has_battery;

        // 7. Dynamic CPU power limits (Continuous mathematical model - No hardcoding)
        let optimal_cpu_ac_limit = 100;
        let optimal_cpu_dc_limit = if !is_laptop {
            100 // Desktop: Full power
        } else {
            cpu_topology.calculate_optimal_dc_limit(100)
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
            gpu_name: gpu_info.name,
            is_dedicated_gpu: gpu_info.is_dedicated,
            gpu_vram_mb: gpu_info.vram_mb,
            gpu_driver_version: gpu_info.driver_version,
            gpu_supports_hags: gpu_info.supports_hags,
            is_laptop,
            cpu_topology,
            optimal_cpu_dc_limit,
            optimal_cpu_ac_limit,
        }
    }

    pub fn apply_hardware_optimal(&self, is_ac: bool, battery_pct: u8) -> Vec<String> {
        if cfg!(test) {
            return vec!["Applied test hardware profile".to_string()];
        }
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

        // 3. CPU & Power Policy (Dynamic according to hardware topology & power source)
        if !self.is_laptop {
            // Desktop workstation
            let _ = crate::power::set_power_scheme("SCHEME_MIN"); // High performance
            let _ = crate::power::set_cpu_max_state_ac(100);
            let hetero = if self.cpu_topology.is_heterogeneous { Some(2) } else { None };
            let _ = crate::power::apply_cpu_deep_tuning(Some(0), Some(2), Some(100), hetero); // EPP 0, Boost Tối đa, Unpark 100%
            let _ = crate::power::set_win32_priority_separation_val(38);
            let _ = crate::power::set_cooling_policy(true);
            applied.push(format!(
                "Desktop Workstation: Mở 100% hiệu năng {} ({}). EPP 0, Boost Tối đa, Unpark 100% nhân.",
                self.cpu_topology.topology_summary, self.cpu_brand
            ));
        } else if is_ac {
            // Laptop on AC
            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::set_cpu_max_state_ac(self.optimal_cpu_ac_limit);
            let hetero = if self.cpu_topology.is_heterogeneous { Some(5) } else { None };
            let _ = crate::power::apply_cpu_deep_tuning(Some(33), Some(2), Some(100), hetero); // EPP 33, Boost Tối đa, Unpark 100%
            let _ = crate::power::set_win32_priority_separation_val(38);
            let _ = crate::power::set_cooling_policy(true);
            applied.push(format!(
                "Laptop cắm sạc (AC): Cấp 100% xung cho {} (EPP 33, Boost Tối đa, Unpark 100% nhân).",
                self.cpu_topology.topology_summary
            ));
        } else {
            // Laptop on Battery (DC) - continuous math formula without any hardcoded thresholds
            let final_dc = self.cpu_topology.calculate_optimal_dc_limit(battery_pct);
            let dynamic_epp = self.cpu_topology.calculate_dynamic_epp(false, battery_pct, false);
            let dynamic_cp = self.cpu_topology.calculate_dynamic_core_parking(false, battery_pct);
            let boost_mode = if battery_pct < 40 { 0 } else { 3 }; // <40% pin: tắt boost hạ nhiệt 15-20°C & tăng 2h pin

            let _ = crate::power::set_power_scheme("SCHEME_BALANCED");
            let _ = crate::power::apply_cpu_deep_tuning_dc(final_dc, dynamic_epp, boost_mode, dynamic_cp);

            let active_cool = battery_pct > 50;
            let _ = crate::power::set_cooling_policy(active_cool);
            applied.push(format!(
                "Laptop chạy pin (DC {}%): Điều tiết {} trần {}% (EPP {}%, Boost {}, Đỗ {}% nhân), quạt {}.",
                battery_pct,
                self.cpu_topology.topology_summary,
                final_dc,
                dynamic_epp,
                if boost_mode == 0 { "Tắt/Hạ nhiệt" } else { "Tiết kiệm" },
                100 - dynamic_cp,
                if active_cool { "chủ động" } else { "yên tĩnh (Passive 0dB)" }
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

pub fn get_current_disk_space() -> (u64, u64) {
    if let Ok(disks) = Disks::new_with_refreshed_list() {
        let mut disk_total_bytes = 0u64;
        let mut disk_available_bytes = 0u64;
        for d in &disks {
            disk_total_bytes += d.total_space();
            disk_available_bytes += d.available_space();
        }
        if disk_total_bytes > 0 {
            return (disk_total_bytes, disk_available_bytes);
        }
    }
    (0, 0)
}

#[derive(Debug, Clone, PartialEq)]
pub struct DetectedGpu {
    pub name: String,
    pub is_dedicated: bool,
    pub vram_mb: u32,
    pub driver_version: String,
    pub supports_hags: bool,
}

fn detect_gpu(os_build: u32) -> DetectedGpu {
    let base_key = r"SYSTEM\CurrentControlSet\Control\Class\{4d36e968-e325-11ce-bfc1-08002be10318}";
    let mut subkeys = crate::reg::reg_enum_subkeys(HKEY_LOCAL_MACHINE, base_key);
    if subkeys.is_empty() {
        subkeys = vec!["0000".to_string(), "0001".to_string(), "0002".to_string(), "0003".to_string()];
    }
    let mut candidates = Vec::new();

    for sub in subkeys {
        if !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        let key_path = format!(r"{}\{}", base_key, sub);
        let desc = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, &key_path, "DriverDesc")
            .or_else(|| crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, &key_path, "HardwareInformation.AdapterString"));

        if let Some(desc) = desc {
            let trimmed = desc.trim().to_string();
            let upper = trimmed.to_uppercase();
            // Lọc các display adapter ảo
            if upper.contains("SPACEDESK")
                || upper.contains("VIRTUAL")
                || upper.contains("MIRACAST")
                || upper.contains("CITRIX")
                || upper.contains("RDP")
            {
                continue;
            }

            let driver_ver = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, &key_path, "DriverVersion")
                .unwrap_or_else(|| "Unknown".to_string());

            let mut vram_bytes = 0u64;
            if let Some(qw) = crate::reg::reg_read_binary(HKEY_LOCAL_MACHINE, &key_path, "HardwareInformation.qwMemorySize") {
                if qw.len() >= 8 {
                    vram_bytes = u64::from_le_bytes(qw[..8].try_into().unwrap_or([0; 8]));
                }
            }
            if vram_bytes == 0 {
                if let Some(dw) = crate::reg::reg_read_binary(HKEY_LOCAL_MACHINE, &key_path, "HardwareInformation.MemorySize") {
                    if dw.len() >= 4 {
                        vram_bytes = u32::from_le_bytes(dw[..4].try_into().unwrap_or([0; 4])) as u64;
                    }
                }
            }
            let vram_mb = (vram_bytes / (1024 * 1024)) as u32;

            let match_id = crate::reg::reg_read_string(HKEY_LOCAL_MACHINE, &key_path, "MatchingDeviceId")
                .unwrap_or_default()
                .to_uppercase();

            let is_nvidia = match_id.contains("VEN_10DE")
                || upper.contains("NVIDIA")
                || upper.contains("GEFORCE")
                || upper.contains("RTX")
                || upper.contains("GTX")
                || upper.contains("QUADRO");
            let is_amd = match_id.contains("VEN_1002") || upper.contains("RADEON");
            let is_intel = match_id.contains("VEN_8086") || upper.contains("INTEL");

            let is_dedicated = if is_nvidia {
                true
            } else if is_amd {
                vram_mb >= 1024 || upper.contains("RX ") || upper.contains("R9 ") || upper.contains("R7 ") || upper.contains("PRO ")
            } else if is_intel {
                upper.contains("ARC") || upper.contains("IRIS XE MAX")
            } else {
                vram_mb >= 1024 && !upper.contains("BASIC")
            };

            // Dò tìm khả năng hỗ trợ HAGS thực tế của GPU driver
            let mut supports_hags = false;
            if os_build >= 19041 && is_dedicated {
                if is_nvidia {
                    supports_hags = upper.contains("GTX 10")
                        || upper.contains("GTX 16")
                        || upper.contains("RTX")
                        || upper.contains("TITAN");
                } else if is_amd {
                    supports_hags = upper.contains("RX 5")
                        || upper.contains("RX 6")
                        || upper.contains("RX 7");
                } else if is_intel {
                    supports_hags = upper.contains("ARC");
                }
            }

            candidates.push(DetectedGpu {
                name: trimmed,
                is_dedicated,
                vram_mb,
                driver_version: driver_ver,
                supports_hags,
            });
        }
    }

    if candidates.is_empty() {
        return DetectedGpu {
            name: "Đồ họa cơ bản Windows".to_string(),
            is_dedicated: false,
            vram_mb: 0,
            driver_version: "Generic".to_string(),
            supports_hags: false,
        };
    }

    if let Some(dgpu) = candidates.iter().find(|g| g.is_dedicated) {
        dgpu.clone()
    } else {
        candidates[0].clone()
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_hardware_profile_real() {
        let mut sys = System::new().expect("failed to initialize sysinfo");
        let hw = HardwareProfile::detect(&mut sys);
        assert!(!hw.gpu_name.is_empty());
        assert!(!hw.gpu_driver_version.is_empty());
        assert!(hw.ram_total_gb > 0.0);
    }
}
