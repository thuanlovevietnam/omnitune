use crate::hardware_profile::HardwareProfile;
use std::os::windows::process::CommandExt;
use std::process::Command;
use sysinfo::System;
use windows_sys::Win32::Foundation::{CloseHandle, HMODULE};
use windows_sys::Win32::System::ProcessStatus::{K32EnumProcessModules, K32GetModuleBaseNameW};
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
use windows_sys::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_INFORMATION, PROCESS_VM_READ};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppGpuPreference {
    Default = 0,
    PowerSaving = 1,
    HighPerformance = 2,
}

pub fn detect_vendor(gpu_name: &str) -> GpuVendor {
    let upper = gpu_name.to_uppercase();
    if upper.contains("NVIDIA") || upper.contains("GEFORCE") || upper.contains("RTX") || upper.contains("GTX") || upper.contains("QUADRO") {
        GpuVendor::Nvidia
    } else if upper.contains("AMD") || upper.contains("RADEON") {
        GpuVendor::Amd
    } else if upper.contains("INTEL") || upper.contains("ARC") || upper.contains("IRIS") || upper.contains("UHD") || upper.contains("HD GRAPHICS") {
        GpuVendor::Intel
    } else {
        GpuVendor::Unknown
    }
}

// 1. Tính toán dung lượng VRAM Segment tối ưu cho iGPU theo RAM thực tế (không hardcode)
pub fn calculate_optimal_igpu_vram_mb(ram_total_gb: f32) -> u32 {
    if ram_total_gb <= 4.5 {
        128
    } else if ram_total_gb <= 8.5 {
        256
    } else if ram_total_gb <= 16.5 {
        512
    } else if ram_total_gb <= 32.5 {
        1024
    } else {
        2048
    }
}

// 2. Tính toán mức độ tiết kiệm điện PCIe ASPM theo thiết bị (Desktop vs Laptop) và pin thực tế
pub fn calculate_dynamic_aspm(hw: &HardwareProfile, is_ac: bool, battery_pct: u8) -> u32 {
    if !hw.is_laptop {
        0 // Máy bàn (Desktop): Luôn cấp băng thông tối đa PCIe (ASPM 0)
    } else if is_ac {
        0 // Laptop cắm sạc: Hiệu năng tối đa PCIe
    } else if battery_pct > 50 {
        1 // Laptop chạy pin >50%: Cân bằng hiệu năng & tiết kiệm điện (ASPM 1)
    } else {
        2 // Laptop chạy pin yếu <=50%: Tiết kiệm sâu dGPU RTD3 (ASPM 2)
    }
}

// 3. Hardware-Accelerated GPU Scheduling (HAGS)
pub fn set_hags(enable: bool) -> bool {
    let key = r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers";
    let val = if enable { 2 } else { 1 };
    crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, key, "HwSchMode", val)
}

#[allow(dead_code)]
pub fn get_hags_state() -> Option<u32> {
    let key = r"SYSTEM\CurrentControlSet\Control\GraphicsDrivers";
    crate::reg::reg_read_u32(HKEY_LOCAL_MACHINE, key, "HwSchMode")
}

// 4. Multimedia SystemProfile GPU Priority
pub fn set_gpu_priority(boost: bool) -> bool {
    let tasks_games_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
    let sys_profile_key = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";

    if boost {
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "GPU Priority", 8);
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 6);
        let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "High");
        let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "SFIO Priority", "High");
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, sys_profile_key, "SystemResponsiveness", 0);
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, sys_profile_key, "NetworkThrottlingIndex", 0xFFFFFFFF);
    } else {
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "GPU Priority", 8);
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_games_key, "Priority", 2);
        let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "Scheduling Category", "Medium");
        let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_games_key, "SFIO Priority", "Normal");
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, sys_profile_key, "SystemResponsiveness", 20);
        let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, sys_profile_key, "NetworkThrottlingIndex", 10);
    }
    true
}

// 5. DirectX Fullscreen Optimization & GameDVR
pub fn apply_directx_fse_optimizations() -> Vec<String> {
    let mut actions = Vec::new();

    // GameConfigStore FSE & latency tweaks (HKCU)
    let gcs_key = r"System\GameConfigStore";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_Enabled", 0);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_FSEBehaviorMode", 2);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_HonorUserFSEBehaviorMode", 1);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_DXGIHonorFSEWindowsCompatible", 1);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_EFSEFeatureFlags", 0);
    actions.push("Bật DirectX Fullscreen Optimizations & tắt GameDVR ngầm".to_string());

    // GameDVR AppCapture (HKCU)
    let dvr_key = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, dvr_key, "AppCaptureEnabled", 0);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, dvr_key, "HistoricalCaptureEnabled", 0);

    // GameBar AutoGameMode
    let gb_key = r"Software\Microsoft\GameBar";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gb_key, "AutoGameModeEnabled", 1);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gb_key, "AllowAutoGameMode", 1);
    actions.push("Bật Windows Auto Game Mode cấp quyền ưu tiên phần cứng đồ họa".to_string());

    actions
}

// 6. DirectX VRR & Windowed Optimization (tự điều chỉnh theo OS Build máy)
pub fn set_directx_vrr_optimization(enable: bool, os_build: u32, is_win11: bool) -> bool {
    let dx_key = r"Software\Microsoft\DirectX\UserGpuPreferences";
    let val = if !enable {
        ""
    } else if is_win11 {
        "VRROptimizeEnable=1;AutoOptIn=1;SwapEffectUpgradeEnable=1;"
    } else if os_build >= 17763 {
        "VRROptimizeEnable=1;AutoOptIn=1;"
    } else {
        "VRROptimizeEnable=1;"
    };
    crate::reg::reg_set_string(HKEY_CURRENT_USER, dx_key, "DirectXUserGlobalSettings", val)
}

// 7. Windows Graphics Preference per-app (HKCU\Software\Microsoft\DirectX\UserGpuPreferences)
pub fn set_app_gpu_preference(exe_path: &str, pref: AppGpuPreference, auto_opt_in: bool) -> bool {
    let dx_key = r"Software\Microsoft\DirectX\UserGpuPreferences";
    match pref {
        AppGpuPreference::Default => {
            crate::reg::reg_delete_value(HKEY_CURRENT_USER, dx_key, exe_path)
        }
        _ => {
            let opt_in = if auto_opt_in { "AutoOptIn=1;" } else { "" };
            let val = format!("GpuPreference={};{}", pref as u32, opt_in);
            crate::reg::reg_set_string(HKEY_CURRENT_USER, dx_key, exe_path, &val)
        }
    }
}

// Dò tìm động tiến trình 3D/DirectX/Vulkan đang chạy trên máy (không hardcode)
pub(crate) fn is_process_using_3d_graphics(pid: u32, exe_path: &str) -> bool {
    let my_pid = std::process::id();
    if pid <= 4 || pid == my_pid {
        return false;
    }

    let lower_path = exe_path.to_lowercase();

    // 0. Loại trừ sớm browser, shell, launcher, media, dev/office và system apps
    // Tránh nhận diện nhầm & giảm tải CPU kiểm tra module Win32
    let excluded_keywords = [
        // Shell & Hệ điều hành
        "explorer.exe", "dwm.exe", "windowsterminal.exe", "textinputhost.exe",
        "applicationframehost.exe", "systemsettings.exe", "shellexperiencehost.exe",
        "searchapp.exe", "searchhost.exe", "startmenuexperiencehost.exe", "taskmgr.exe",
        "powershell.exe", "cmd.exe", "conhost.exe", "powertoys.exe", "translucenttb.exe",

        // Trình duyệt web
        "chrome.exe", "brave.exe", "edge.exe", "msedge.exe", "firefox.exe", "opera.exe",

        // Game Launchers & Client Store (không phải bản thân trò chơi)
        "steam.exe", "steamwebhelper.exe", "steamservice.exe",
        "epicgameslauncher.exe", "riotclientservices.exe", "riotclientux.exe",
        "battle.net.exe", "agent.exe", "galaxyclient.exe", "galaxyclientservice.exe",
        "upc.exe", "ubisoftconnect.exe", "eadesktop.exe", "origin.exe",

        // IDE, Editor & Terminal
        "code.exe", "cursor.exe", "windsurf.exe", "devenv.exe", "idea64.exe",
        "clion64.exe", "pycharm64.exe", "webstorm64.exe", "rider64.exe", "sublime_text.exe",

        // Meeting, Chat & Văn phòng
        "discord.exe", "slack.exe", "teams.exe", "zoom.exe", "webex.exe",
        "winword.exe", "excel.exe", "powerpnt.exe", "outlook.exe", "onenote.exe",

        // Media Player & Audio (thuộc kịch bản Media riêng)
        "vlc.exe", "potplayer", "mpc-hc", "mpc-be", "mpv.exe", "kmplayer",
        "foobar2000", "musicbee.exe", "aimp.exe", "spotify.exe", "netflix",

        // Live Wallpaper & Background Render tools
        "wallpaper32.exe", "wallpaper64.exe", "lively.exe", "obs64.exe", "obs32.exe", "sharex.exe",
    ];
    if excluded_keywords.iter().any(|&k| lower_path.contains(k)) {
        return false;
    }
    if lower_path.contains(r"\windows\system32\") || lower_path.contains(r"\windows\systemapps\") {
        return false;
    }

    // 1. Thư mục cài đặt thư viện game của các nền tảng phổ biến trên máy
    // Lưu ý: Không đưa "windowsapps" vào đây vì thư mục này chứa tất cả ứng dụng Microsoft Store (WhatsApp, Calculator...)
    // Các game Xbox Game Pass cài trong windowsapps sẽ được phát hiện chính xác qua module 3D ở bước 2.
    let game_dirs = [
        "steamapps", "riot games", "epic games", "ubisoft", "gog galaxy",
        "ea games", "origin games", "battle.net", "xboxgames",
    ];
    if game_dirs.iter().any(|&d| lower_path.contains(d)) {
        return true;
    }

    // 2. Dò tìm trực tiếp các module 3D runtime (DirectX, Vulkan, OpenGL, NVAPI, AMDXC) nạp trong tiến trình
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return false;
        }

        let mut modules: [HMODULE; 128] = [std::ptr::null_mut(); 128];
        let mut cb_needed: u32 = 0;
        let mut is_3d = false;

        if K32EnumProcessModules(
            handle,
            modules.as_mut_ptr(),
            (modules.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut cb_needed,
        ) != 0 {
            let count = (cb_needed as usize / std::mem::size_of::<HMODULE>()).min(modules.len());
            for &hmod in &modules[..count] {
                if hmod.is_null() {
                    continue;
                }
                let mut buf = [0u16; 64];
                let len = K32GetModuleBaseNameW(handle, hmod, buf.as_mut_ptr(), buf.len() as u32);
                if len > 0 {
                    let mod_name = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
                    if mod_name == "d3d11.dll"
                        || mod_name == "d3d12.dll"
                        || mod_name == "dxgi.dll"
                        || mod_name == "vulkan-1.dll"
                        || mod_name == "opengl32.dll"
                        || mod_name.starts_with("nvapi")
                        || mod_name.starts_with("amdxc")
                    {
                        is_3d = true;
                        break;
                    }
                }
            }
        }

        CloseHandle(handle);
        is_3d
    }
}

// Dò tìm động tiến trình media player đang phát (không hardcode)
fn is_process_media_player(pid: u32, exe_path: &str) -> bool {
    let lower_path = exe_path.to_lowercase();
    let media_keywords = [
        "vlc.exe", "mpc-hc", "mpc-be", "potplayer", "kmplayer", "mpv.exe", "foobar2000",
        "wmplayer.exe", "musicbee.exe", "aimp.exe",
    ];
    if media_keywords.iter().any(|&m| lower_path.contains(m)) {
        return true;
    }

    // Dò tìm module giải mã audio/video (lavfilters, avcodec, libvlc)
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, 0, pid);
        if handle.is_null() {
            return false;
        }

        let mut modules: [HMODULE; 128] = [std::ptr::null_mut(); 128];
        let mut cb_needed: u32 = 0;
        let mut is_media = false;

        if K32EnumProcessModules(
            handle,
            modules.as_mut_ptr(),
            (modules.len() * std::mem::size_of::<HMODULE>()) as u32,
            &mut cb_needed,
        ) != 0 {
            let count = (cb_needed as usize / std::mem::size_of::<HMODULE>()).min(modules.len());
            for &hmod in &modules[..count] {
                if hmod.is_null() {
                    continue;
                }
                let mut buf = [0u16; 64];
                let len = K32GetModuleBaseNameW(handle, hmod, buf.as_mut_ptr(), buf.len() as u32);
                if len > 0 {
                    let mod_name = String::from_utf16_lossy(&buf[..len as usize]).to_lowercase();
                    if mod_name.contains("lavvideo")
                        || mod_name.contains("lavsplitter")
                        || mod_name.contains("avcodec")
                        || mod_name.contains("libvlc")
                    {
                        is_media = true;
                        break;
                    }
                }
            }
        }

        CloseHandle(handle);
        is_media
    }
}

// Quét toàn bộ tiến trình đang chạy và áp dụng cấu hình GPU phù hợp theo đặc tính máy
pub fn boost_running_graphics_apps(sys: &System, pref: AppGpuPreference) -> usize {
    let mut count = 0;
    for (&pid, proc) in sys.processes() {
        let raw_pid = pid.as_u32();
        if raw_pid <= 4 {
            continue;
        }
        if let Some(exe_path) = proc.exe() {
            let path_str = exe_path.to_string_lossy();
            if path_str.is_empty() {
                continue;
            }
            let is_target = match pref {
                AppGpuPreference::HighPerformance => is_process_using_3d_graphics(raw_pid, &path_str),
                AppGpuPreference::PowerSaving => is_process_media_player(raw_pid, &path_str),
                AppGpuPreference::Default => false,
            };

            if is_target && set_app_gpu_preference(&path_str, pref, true) {
                count += 1;
            }
        }
    }
    count
}

// 8. Intel iGPU Dedicated Segment Size (theo dung lượng RAM thực tế)
pub fn set_intel_igpu_vram(size_mb: u32) -> bool {
    let key = r"SOFTWARE\Intel\GMM";
    crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, key, "DedicatedSegmentSize", size_mb)
}

// 9. PCIe Link State Power Management (ASPM)
pub fn set_pcie_aspm(level: u32, is_ac: bool) -> bool {
    if cfg!(test) {
        return true;
    }
    let pcie_sub = "501a4d13-42af-4429-9fd1-a8218c268e20";
    let pcie_setting = "ee12f906-d277-404b-b6da-e5fa1a576df5";
    let val_str = level.to_string();

    let target_flag = if is_ac { "/setacvalueindex" } else { "/setdcvalueindex" };

    let _ = Command::new("powercfg")
        .args([target_flag, "SCHEME_CURRENT", pcie_sub, pcie_setting, &val_str])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    let _ = Command::new("powercfg")
        .args(["/setactive", "SCHEME_CURRENT"])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    true
}

// 10. Vendor Graphics Power Plan (Chỉ thực thi khi nhà sản xuất đó có mặt trên máy)
pub fn set_vendor_graphics_power(vendor: GpuVendor, high_perf: bool, is_ac: bool) {
    if cfg!(test) {
        return;
    }
    let flag = if is_ac { "/setacvalueindex" } else { "/setdcvalueindex" };

    match vendor {
        GpuVendor::Intel => {
            let intel_sub = "44f3beca-a7c0-460e-9df2-bb8b99e0cba6";
            let intel_setting = "3619c3f2-afb2-4afc-b0e9-e7fef372de36";
            let val = if high_perf { "2" } else if is_ac { "1" } else { "0" };
            let _ = Command::new("powercfg")
                .args([flag, "SCHEME_CURRENT", intel_sub, intel_setting, val])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        GpuVendor::Amd => {
            let amd_sub = "5fb4938d-1ee8-40fa-bac4-988124f5c7b1";
            let amd_setting = "a2f04e26-3536-419b-a041-30925082c8d1";
            let val = if high_perf { "3" } else if is_ac { "2" } else { "0" };
            let _ = Command::new("powercfg")
                .args([flag, "SCHEME_CURRENT", amd_sub, amd_setting, val])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        _ => {}
    }
}

// 11. Orchestrator cho Hardware Profile Optimal (Tuân theo thông tin phần cứng máy)
pub fn apply_hardware_gpu_optimal(hw: &HardwareProfile, is_ac: bool, battery_pct: u8) -> Vec<String> {
    let mut msgs = Vec::new();
    if cfg!(test) {
        return msgs;
    }
    let vendor = detect_vendor(&hw.gpu_name);

    // DirectX FSE & Game Mode (cấu hình theo OS Build máy)
    let _ = apply_directx_fse_optimizations();
    let _ = set_directx_vrr_optimization(true, hw.os_build, hw.is_windows_11);

    if hw.is_dedicated_gpu {
        // dGPU: Kích hoạt HAGS nếu phần cứng và driver thực tế hỗ trợ
        if hw.gpu_supports_hags {
            if set_hags(true) {
                msgs.push(format!(
                    "GPU rời [{}] ({}MB VRAM, Driver {}): Bật HAGS giảm độ trễ render.",
                    hw.gpu_name, hw.gpu_vram_mb, hw.gpu_driver_version
                ));
            } else {
                msgs.push(format!(
                    "GPU rời [{}] ({}MB VRAM): Ưu tiên phân bổ đồ họa hiệu năng cao.",
                    hw.gpu_name, hw.gpu_vram_mb
                ));
            }
        } else {
            msgs.push(format!(
                "GPU rời [{}] ({}MB VRAM, Driver {}): Tối ưu hóa GPU Priority & PCIe ASPM theo thông số máy.",
                hw.gpu_name, hw.gpu_vram_mb, hw.gpu_driver_version
            ));
        }

        // Tối ưu GPU Priority trong Multimedia Scheduler
        set_gpu_priority(true);

        // PCIe ASPM: Dò tìm theo thiết bị (Desktop vs Laptop) và dung lượng pin thực tế
        let aspm = calculate_dynamic_aspm(hw, is_ac, battery_pct);
        set_pcie_aspm(aspm, is_ac);
        if vendor != GpuVendor::Unknown {
            set_vendor_graphics_power(vendor, is_ac, is_ac);
        }
    } else {
        // iGPU: Cân bằng tài nguyên, cấu hình Intel GMM theo dung lượng RAM thực tế của máy
        if vendor == GpuVendor::Intel {
            let target_vram_mb = calculate_optimal_igpu_vram_mb(hw.ram_total_gb);
            set_intel_igpu_vram(target_vram_mb);
            msgs.push(format!(
                "GPU tích hợp [{}] (Driver {}): Cấu hình {}MB VRAM Segment (Intel GMM, tương ứng RAM {:.1}GB), tối ưu DirectX Flip Model & PCIe ASPM.",
                hw.gpu_name, hw.gpu_driver_version, target_vram_mb, hw.ram_total_gb
            ));
        } else {
            msgs.push(format!(
                "GPU tích hợp [{}] (Driver {}): Kích hoạt DirectX Flip Model & tối ưu PCIe ASPM.",
                hw.gpu_name, hw.gpu_driver_version
            ));
        }
        set_gpu_priority(false);
        let aspm = calculate_dynamic_aspm(hw, is_ac, battery_pct);
        set_pcie_aspm(aspm, is_ac);
        if vendor != GpuVendor::Unknown {
            set_vendor_graphics_power(vendor, is_ac, is_ac);
        }
    }

    msgs
}

// 12. Orchestrator cho Scenario Gaming & Graphics
pub fn apply_scenario_gaming_gpu(hw: &HardwareProfile, _is_pro: bool, sys: &System) -> Vec<String> {
    let mut msgs = Vec::new();
    if cfg!(test) {
        return msgs;
    }
    let vendor = detect_vendor(&hw.gpu_name);

    // Bật DirectX FSE & Game Mode
    msgs.extend(apply_directx_fse_optimizations());
    let _ = set_directx_vrr_optimization(true, hw.os_build, hw.is_windows_11);

    // Ép GPU Priority = 8 tối đa
    set_gpu_priority(true);
    msgs.push("Thiết lập GPU Priority = 8 (Multimedia SystemProfile) cho tác vụ đồ họa".to_string());

    // Ép PCIe ASPM = 0 (tắt tiết kiệm điện bus PCIe, chống drop FPS)
    set_pcie_aspm(0, true);
    set_pcie_aspm(0, false);

    // Kích hoạt Vendor GPU Maximum Performance
    if vendor != GpuVendor::Unknown {
        set_vendor_graphics_power(vendor, true, true);
        set_vendor_graphics_power(vendor, true, false);
    }

    if hw.gpu_supports_hags {
        let _ = set_hags(true);
    }

    // Tự động dò tìm các tiến trình 3D/DirectX/Vulkan đang chạy trên máy và gán High Performance GPU
    let game_count = boost_running_graphics_apps(sys, AppGpuPreference::HighPerformance);
    if game_count > 0 {
        msgs.push(format!("Đã nhận diện và gán High Performance GPU (DirectX Preferences) cho {} tiến trình 3D/đồ họa.", game_count));
    }

    msgs
}

// 13. Orchestrator cho Scenario MediaChill (Hardware Video Acceleration)
pub fn apply_scenario_media_gpu(hw: &HardwareProfile, is_ac: bool, sys: &System) -> Vec<String> {
    let mut msgs = Vec::new();
    if cfg!(test) {
        return msgs;
    }
    let vendor = detect_vendor(&hw.gpu_name);

    // MMCSS Playback Task GPU Priority
    let tasks_playback = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Playback";
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_playback, "GPU Priority", 8);
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_playback, "Priority", 6);
    let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_playback, "Scheduling Category", "High");
    let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_playback, "Background Only", "False");
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_playback, "Clock Rate", 10000);

    // Hardware Video Acceleration (Media Foundation DXVA)
    let mf_key = r"Software\Microsoft\Windows Media Foundation\Platform";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, mf_key, "EnableHardwareVideoAcceleration", 1);

    // Powercfg Video Playback Quality Bias (SUB_VIDEO)
    let video_sub = "9590e415-9c70-4e74-b9f0-4b382a659080";
    let video_bias = "1077503d-7565-4e2e-8260-ac967da929e6";
    let flag = if is_ac { "/setacvalueindex" } else { "/setdcvalueindex" };
    let bias_val = if is_ac { "1" } else { "0" };
    let _ = Command::new("powercfg")
        .args([flag, "SCHEME_CURRENT", video_sub, video_bias, bias_val])
        .creation_flags(CREATE_NO_WINDOW)
        .status();

    // Nếu có dGPU và chạy pin: hạ PCIe ASPM = 2 để dGPU ngủ sâu, nhường giải mã cho iGPU tiết kiệm điện và giữ mát
    if hw.is_dedicated_gpu && !is_ac {
        set_pcie_aspm(2, false);
        if vendor != GpuVendor::Unknown {
            set_vendor_graphics_power(vendor, false, false);
        }
    }

    // Dò tìm động các tiến trình phát media trên máy và gán cấu hình iGPU/Power Saving
    let media_count = boost_running_graphics_apps(sys, AppGpuPreference::PowerSaving);

    msgs.push("Bật Hardware Video Acceleration (Media Foundation DXVA) & ưu tiên GPU Playback".to_string());
    if media_count > 0 {
        msgs.push(format!("Đã nhận diện và gán iGPU/Power-Saving cho {} trình phát media để giữ quạt êm và tiết kiệm pin.", media_count));
    }

    msgs
}

// 14. Orchestrator cho Scenario OfficeMeeting (Camera & Screen Share Hardware Acceleration)
pub fn apply_scenario_office_gpu(_hw: &HardwareProfile, _is_ac: bool) -> Vec<String> {
    let mut msgs = Vec::new();
    let tasks_capture = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Capture";
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_capture, "GPU Priority", 8);
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_capture, "Priority", 6);
    let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_capture, "Scheduling Category", "High");

    msgs.push("Ưu tiên GPU Capture (Camera & Screen Share) trong Multimedia Scheduler".to_string());
    msgs
}

// 15. Orchestrator cho Scenario RemoteDesktop (Hardware GPU Encoding cho RDP)
pub fn apply_scenario_remote_gpu() -> Vec<String> {
    let mut msgs = Vec::new();
    let rdp_key = r"SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services";
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, rdp_key, "fEnableHardwareMode", 1);
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, rdp_key, "bRemoteFxAdvancedHardwareGraphics", 1);

    msgs.push("Kích hoạt GPU Hardware Encoding cho Remote Desktop (Terminal Services)".to_string());
    msgs
}

// 16. Tiết kiệm điện tối đa cho GPU khi chạy pin / nhàn rỗi (RTD3)
pub fn apply_gpu_power_save(hw: &HardwareProfile) -> Vec<String> {
    let mut msgs = Vec::new();
    if cfg!(test) {
        return msgs;
    }
    let vendor = detect_vendor(&hw.gpu_name);

    // Hạ GPU Priority về bình thường
    set_gpu_priority(false);

    // Ép PCIe ASPM = 2 (Max Power Savings -> dGPU chuyển về trạng thái ngủ sâu L1/RTD3)
    set_pcie_aspm(2, false);
    set_pcie_aspm(2, true);

    // Hạ đồ họa tích hợp/rời xuống mức tiết kiệm pin nhất
    if vendor != GpuVendor::Unknown {
        set_vendor_graphics_power(vendor, false, false);
    }

    msgs.push(format!(
        "GPU [{}]: Hạ xung nhịp và đưa bus PCIe vào chế độ ngủ sâu tiết kiệm pin (RTD3).",
        hw.gpu_name
    ));
    msgs
}

// 17. Khôi phục mặc định Windows
pub fn restore_gpu_defaults() -> Vec<String> {
    let mut msgs = Vec::new();

    // GameConfigStore & GameDVR về mặc định
    let gcs_key = r"System\GameConfigStore";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_Enabled", 1);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_FSEBehaviorMode", 0);

    let dvr_key = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, dvr_key, "AppCaptureEnabled", 1);

    // DirectX VRR & Global Settings xóa sạch
    let dx_key = r"Software\Microsoft\DirectX\UserGpuPreferences";
    crate::reg::reg_delete_value(HKEY_CURRENT_USER, dx_key, "DirectXUserGlobalSettings");

    // Multimedia SystemProfile về mặc định
    set_gpu_priority(false);

    // MMCSS Tasks Playback & Capture về mặc định
    let tasks_playback = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Playback";
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_playback, "GPU Priority", 8);
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_playback, "Priority", 2);
    let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_playback, "Scheduling Category", "Medium");

    let tasks_capture = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Capture";
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_capture, "GPU Priority", 8);
    let _ = crate::reg::reg_set_u32(HKEY_LOCAL_MACHINE, tasks_capture, "Priority", 2);
    let _ = crate::reg::reg_set_string(HKEY_LOCAL_MACHINE, tasks_capture, "Scheduling Category", "Medium");

    // Media Foundation Hardware Video Acceleration về mặc định
    let mf_key = r"Software\Microsoft\Windows Media Foundation\Platform";
    crate::reg::reg_delete_value(HKEY_CURRENT_USER, mf_key, "EnableHardwareVideoAcceleration");

    // Terminal Services RDP GPU về mặc định
    let rdp_key = r"SOFTWARE\Policies\Microsoft\Windows NT\Terminal Services";
    crate::reg::reg_delete_value(HKEY_LOCAL_MACHINE, rdp_key, "fEnableHardwareMode");
    crate::reg::reg_delete_value(HKEY_LOCAL_MACHINE, rdp_key, "bRemoteFxAdvancedHardwareGraphics");

    // Intel GMM về mặc định
    let intel_gmm = r"SOFTWARE\Intel\GMM";
    crate::reg::reg_delete_value(HKEY_LOCAL_MACHINE, intel_gmm, "DedicatedSegmentSize");

    // PCIe ASPM về mặc định (Balanced = 1)
    set_pcie_aspm(1, true);
    set_pcie_aspm(2, false);

    msgs.push("Đã khôi phục thiết lập GPU, DirectX và GameDVR về mặc định Windows.".to_string());
    msgs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_detect_vendor() {
        assert_eq!(detect_vendor("NVIDIA GeForce RTX 4080"), GpuVendor::Nvidia);
        assert_eq!(detect_vendor("AMD Radeon RX 7900 XTX"), GpuVendor::Amd);
        assert_eq!(detect_vendor("Intel(R) Iris(R) Xe Graphics"), GpuVendor::Intel);
        assert_eq!(detect_vendor("Microsoft Basic Display Adapter"), GpuVendor::Unknown);
    }

    #[test]
    fn test_dynamic_vram_calculation() {
        assert_eq!(calculate_optimal_igpu_vram_mb(4.0), 128);
        assert_eq!(calculate_optimal_igpu_vram_mb(8.0), 256);
        assert_eq!(calculate_optimal_igpu_vram_mb(16.0), 512);
        assert_eq!(calculate_optimal_igpu_vram_mb(32.0), 1024);
        assert_eq!(calculate_optimal_igpu_vram_mb(64.0), 2048);
    }

    #[test]
    fn test_app_gpu_preference_formatting() {
        let test_exe = r"C:\Windows\System32\notepad.exe";
        assert!(set_app_gpu_preference(test_exe, AppGpuPreference::HighPerformance, true));
        let read_back = crate::reg::reg_read_string(
            HKEY_CURRENT_USER,
            r"Software\Microsoft\DirectX\UserGpuPreferences",
            test_exe,
        );
        assert_eq!(read_back.as_deref(), Some("GpuPreference=2;AutoOptIn=1;"));
        assert!(set_app_gpu_preference(test_exe, AppGpuPreference::Default, false));
    }

    #[test]
    fn test_is_process_using_3d_graphics_exclusions() {
        let my_pid = std::process::id();
        assert!(!is_process_using_3d_graphics(my_pid, "omnitune.exe"));
        assert!(!is_process_using_3d_graphics(0, ""));
        assert!(!is_process_using_3d_graphics(4, "System"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Program Files\Google\Chrome\Application\chrome.exe"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Users\Admin\AppData\Local\Programs\Microsoft VS Code\Code.exe"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Program Files (x86)\Steam\steam.exe"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Program Files\WindowsApps\5319275A.WhatsAppDesktop_cv1g1gvanyjgm\WhatsApp.exe"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Windows\System32\dwm.exe"));
        assert!(!is_process_using_3d_graphics(1234, r"C:\Program Files\Spotify\Spotify.exe"));
    }

    #[test]
    fn test_is_process_using_3d_graphics_game_folder() {
        assert!(is_process_using_3d_graphics(99999, r"D:\SteamLibrary\steamapps\common\Cyberpunk 2077\bin\x64\Cyberpunk2077.exe"));
        assert!(is_process_using_3d_graphics(99999, r"C:\Riot Games\VALORANT\live\ShooterGame\Binaries\Win64\VALORANT-Win64-Shipping.exe"));
        assert!(is_process_using_3d_graphics(99999, r"E:\Epic Games\GTA5\GTA5.exe"));
        assert!(is_process_using_3d_graphics(99999, r"C:\XboxGames\Halo Infinite\Content\HaloInfinite.exe"));
    }
}
