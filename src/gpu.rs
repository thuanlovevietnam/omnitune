use crate::hardware_profile::HardwareProfile;
use std::os::windows::process::CommandExt;
use std::process::Command;
use windows_sys::Win32::System::Registry::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};

const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GpuVendor {
    Nvidia,
    Amd,
    Intel,
    Unknown,
}

pub fn detect_vendor(gpu_name: &str) -> GpuVendor {
    let upper = gpu_name.to_uppercase();
    if upper.contains("NVIDIA") || upper.contains("GEFORCE") || upper.contains("RTX") || upper.contains("GTX") {
        GpuVendor::Nvidia
    } else if upper.contains("AMD") || upper.contains("RADEON") {
        GpuVendor::Amd
    } else if upper.contains("INTEL") || upper.contains("ARC") || upper.contains("IRIS") || upper.contains("UHD") {
        GpuVendor::Intel
    } else {
        GpuVendor::Unknown
    }
}

// 1. Hardware-Accelerated GPU Scheduling (HAGS)
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

// 2. Multimedia SystemProfile GPU Priority
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

// 3. DirectX Fullscreen Optimization & GameDVR
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

// 4. DirectX VRR & Windowed Optimization (HKCU)
pub fn set_directx_vrr_optimization(enable: bool) -> bool {
    let dx_key = r"Software\Microsoft\DirectX\UserGpuPreferences";
    let val = if enable {
        "VRROptimizeEnable=1;AutoOptIn=1;SwapEffectUpgradeEnable=1;"
    } else {
        ""
    };
    crate::reg::reg_set_string(HKEY_CURRENT_USER, dx_key, "DirectXUserGlobalSettings", val)
}

// 5. PCIe Link State Power Management (ASPM)
// 0 = Off (Max performance, 0 latency wake)
// 1 = Moderate Power Savings
// 2 = Maximum Power Savings (dGPU RTD3 deep sleep)
pub fn set_pcie_aspm(level: u32, is_ac: bool) -> bool {
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

// 6. Vendor Graphics Power Plan (Intel / AMD switchable)
pub fn set_vendor_graphics_power(vendor: GpuVendor, high_perf: bool, is_ac: bool) {
    let flag = if is_ac { "/setacvalueindex" } else { "/setdcvalueindex" };

    match vendor {
        GpuVendor::Intel => {
            // Intel Graphics Power Plan: 2 = Max Perf, 1 = Balanced, 0 = Max Battery
            let intel_sub = "44f3beca-a7c0-460e-9df2-bb8b99e0cba6";
            let intel_setting = "3619c3f2-afb2-4afc-b0e9-e7fef372de36";
            let val = if high_perf { "2" } else if is_ac { "1" } else { "0" };
            let _ = Command::new("powercfg")
                .args([flag, "SCHEME_CURRENT", intel_sub, intel_setting, val])
                .creation_flags(CREATE_NO_WINDOW)
                .status();
        }
        GpuVendor::Amd => {
            // Switchable Dynamic Graphics: 3 = Max Perf, 2 = Optimize Perf, 1 = Optimize Power, 0 = Max Power
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

// 7. Orchestrator cho Hardware Profile Optimal
pub fn apply_hardware_gpu_optimal(hw: &HardwareProfile, is_ac: bool, _battery_pct: u8) -> Vec<String> {
    let mut msgs = Vec::new();
    let vendor = detect_vendor(&hw.gpu_name);

    // A. DirectX FSE & Game Mode (luôn an toàn, chạy trong user space)
    let _ = apply_directx_fse_optimizations();
    let _ = set_directx_vrr_optimization(true);

    if hw.is_dedicated_gpu {
        // dGPU: Kích hoạt HAGS nếu W10 (>=19041) hoặc Win11
        if hw.os_build >= 19041 {
            if set_hags(true) {
                msgs.push(format!(
                    "GPU rời [{}]: Bật HAGS (Hardware Scheduling) giảm độ trễ render.",
                    hw.gpu_name
                ));
            } else {
                msgs.push(format!(
                    "GPU rời [{}]: Ưu tiên phân bổ đồ họa hiệu năng cao.",
                    hw.gpu_name
                ));
            }
        } else {
            msgs.push(format!(
                "GPU rời [{}]: Ưu tiên phân bổ đồ họa hiệu năng cao.",
                hw.gpu_name
            ));
        }

        // Tối ưu GPU Priority trong Multimedia Scheduler
        set_gpu_priority(true);

        // PCIe ASPM: 0 (Off/Max perf) khi cắm sạc, 1 (Balanced) khi chạy pin
        let aspm = if is_ac { 0 } else { 1 };
        set_pcie_aspm(aspm, is_ac);
        set_vendor_graphics_power(vendor, is_ac, is_ac);
    } else {
        // iGPU: Cân bằng tài nguyên, không ép HAGS để tránh nghẽn bus RAM chia sẻ
        msgs.push(format!(
            "GPU tích hợp [{}]: Tối ưu hóa DirectX Flip Model & VRAM chia sẻ động.",
            hw.gpu_name
        ));
        set_gpu_priority(false);
        let aspm = if is_ac { 1 } else { 2 };
        set_pcie_aspm(aspm, is_ac);
        set_vendor_graphics_power(vendor, is_ac, is_ac);
    }

    msgs
}

// 8. Orchestrator cho Scenario Gaming & Graphics
pub fn apply_scenario_gaming_gpu(hw: &HardwareProfile, _is_pro: bool) -> Vec<String> {
    let mut msgs = Vec::new();
    let vendor = detect_vendor(&hw.gpu_name);

    // Bật DirectX FSE & Game Mode
    msgs.extend(apply_directx_fse_optimizations());
    let _ = set_directx_vrr_optimization(true);

    // Ép GPU Priority = 8 tối đa
    set_gpu_priority(true);
    msgs.push("Thiết lập GPU Priority = 8 (Multimedia SystemProfile) cho tác vụ đồ họa".to_string());

    // Ép PCIe ASPM = 0 (tắt tiết kiệm điện bus PCIe, chống drop FPS)
    set_pcie_aspm(0, true);
    set_pcie_aspm(0, false);

    // Kích hoạt Vendor GPU Maximum Performance
    set_vendor_graphics_power(vendor, true, true);
    set_vendor_graphics_power(vendor, true, false);

    if hw.is_dedicated_gpu && hw.os_build >= 19041 {
        let _ = set_hags(true);
    }

    msgs
}

// 9. Tiết kiệm điện tối đa cho GPU khi chạy pin / nhàn rỗi (RTD3)
pub fn apply_gpu_power_save(hw: &HardwareProfile) -> Vec<String> {
    let mut msgs = Vec::new();
    let vendor = detect_vendor(&hw.gpu_name);

    // Hạ GPU Priority về bình thường
    set_gpu_priority(false);

    // Ép PCIe ASPM = 2 (Max Power Savings -> dGPU chuyển về trạng thái ngủ sâu L1/RTD3)
    set_pcie_aspm(2, false);
    set_pcie_aspm(2, true);

    // Hạ đồ họa tích hợp/rời xuống mức tiết kiệm pin nhất
    set_vendor_graphics_power(vendor, false, false);

    msgs.push(format!(
        "GPU [{}]: Hạ xung nhịp và đưa bus PCIe vào chế độ ngủ sâu tiết kiệm pin (RTD3).",
        hw.gpu_name
    ));
    msgs
}

// 10. Khôi phục mặc định Windows
pub fn restore_gpu_defaults() -> Vec<String> {
    let mut msgs = Vec::new();

    // GameConfigStore & GameDVR về mặc định
    let gcs_key = r"System\GameConfigStore";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_Enabled", 1);
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, gcs_key, "GameDVR_FSEBehaviorMode", 0);

    let dvr_key = r"Software\Microsoft\Windows\CurrentVersion\GameDVR";
    let _ = crate::reg::reg_set_u32(HKEY_CURRENT_USER, dvr_key, "AppCaptureEnabled", 1);

    // DirectX VRR về rỗng
    let _ = set_directx_vrr_optimization(false);

    // Multimedia SystemProfile về mặc định
    set_gpu_priority(false);

    // PCIe ASPM về mặc định (Balanced = 1)
    set_pcie_aspm(1, true);
    set_pcie_aspm(2, false);

    msgs.push("Đã khôi phục thiết lập GPU, DirectX và GameDVR về mặc định Windows.".to_string());
    msgs
}
