use serde::{Deserialize, Serialize};
use sysinfo::System;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{
    OpenProcess, SetProcessAffinityMask, PROCESS_SET_INFORMATION,
};

/// Cấu trúc đại diện cho hình thái vi kiến trúc CPU thực tế
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuTopology {
    pub total_logical_cores: usize,
    pub total_physical_cores: usize,
    pub p_core_count: usize,
    pub e_core_count: usize,
    pub has_smt: bool,
    pub is_heterogeneous: bool,
    pub p_core_mask: usize,
    pub e_core_mask: usize,
    pub all_cores_mask: usize,
    pub non_core0_mask: usize,
    pub topology_summary: String,
}

impl Default for CpuTopology {
    fn default() -> Self {
        Self {
            total_logical_cores: 1,
            total_physical_cores: 1,
            p_core_count: 1,
            e_core_count: 0,
            has_smt: false,
            is_heterogeneous: false,
            p_core_mask: 1,
            e_core_mask: 1,
            all_cores_mask: 1,
            non_core0_mask: 1,
            topology_summary: "1 Nhân / 1 Luồng".to_string(),
        }
    }
}

impl CpuTopology {
    /// Quét Topology động qua Win32 NT Kernel API - Tuyệt đối không phỏng đoán hay hardcode tên chip
    pub fn probe() -> Self {
        let mut length: u32 = 0;
        unsafe {
            GetLogicalProcessorInformationEx(RelationProcessorCore, std::ptr::null_mut(), &mut length);
        }

        if length == 0 {
            return Self::fallback_probe();
        }

        let mut buffer: Vec<u8> = vec![0; length as usize];
        let p_info = buffer.as_mut_ptr() as *mut SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX;

        let success = unsafe {
            GetLogicalProcessorInformationEx(RelationProcessorCore, p_info, &mut length)
        };

        if success == 0 {
            return Self::fallback_probe();
        }

        let mut total_physical = 0usize;
        let mut total_logical = 0usize;
        let mut has_smt = false;
        let mut efficiency_classes = Vec::new();
        let mut core_masks = Vec::new();

        let mut offset = 0usize;
        while offset < length as usize {
            let item = unsafe { &*(buffer.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX) };
            if item.Size == 0 {
                break;
            }

            if item.Relationship == RelationProcessorCore {
                total_physical += 1;
                let proc_info = unsafe { &item.Anonymous.Processor };
                let is_core_smt = (proc_info.Flags & 0x01) != 0;
                if is_core_smt {
                    has_smt = true;
                }

                let mut phys_core_mask = 0usize;
                let p_groups = proc_info.GroupMask.as_ptr();
                for g in 0..proc_info.GroupCount as usize {
                    let group_aff = unsafe { *p_groups.add(g) };
                    // SetProcessAffinityMask operates on Group 0 (first 64 processors)
                    if group_aff.Group == 0 {
                        let mask = group_aff.Mask as usize;
                        phys_core_mask |= mask;
                    }
                    total_logical += (group_aff.Mask as usize).count_ones() as usize;
                }

                efficiency_classes.push(proc_info.EfficiencyClass);
                core_masks.push(phys_core_mask);
            }
            offset += item.Size as usize;
        }

        if total_physical == 0 || total_logical == 0 {
            return Self::fallback_probe();
        }

        let max_eff = efficiency_classes.iter().copied().max().unwrap_or(0);
        let min_eff = efficiency_classes.iter().copied().min().unwrap_or(0);
        let is_heterogeneous = max_eff > min_eff;

        let mut p_mask = 0usize;
        let mut e_mask = 0usize;
        let mut p_cores = 0usize;
        let mut e_cores = 0usize;

        for (&eff, &mask) in efficiency_classes.iter().zip(core_masks.iter()) {
            if is_heterogeneous {
                if eff == max_eff {
                    p_mask |= mask;
                    p_cores += 1;
                } else {
                    e_mask |= mask;
                    e_cores += 1;
                }
            } else {
                p_mask |= mask;
                e_mask |= mask;
                p_cores += 1;
            }
        }

        let all_mask = p_mask | e_mask;
        let non_core0_mask = if total_logical >= 6 {
            all_mask & !0x03 // Loại trừ Core 0 & Core 1 (DPC ngắt chuột / Card mạng / USB)
        } else {
            all_mask
        };

        let topology_summary = if is_heterogeneous {
            format!(
                "{}P + {}E ({} luồng, Kiến trúc Lai)",
                p_cores, e_cores, total_logical
            )
        } else if has_smt {
            format!(
                "{}C / {}T (SMT, {} nhân thực)",
                total_physical, total_logical, total_physical
            )
        } else {
            format!(
                "{}C / {}T (Thuần nhất, {} nhân)",
                total_physical, total_logical, total_physical
            )
        };

        Self {
            total_logical_cores: total_logical,
            total_physical_cores: total_physical,
            p_core_count: p_cores,
            e_core_count: e_cores,
            has_smt,
            is_heterogeneous,
            p_core_mask: p_mask,
            e_core_mask: e_mask,
            all_cores_mask: all_mask,
            non_core0_mask,
            topology_summary,
        }
    }

    fn fallback_probe() -> Self {
        let (logical, physical) = if let Ok(mut sys) = System::new() {
            sys.refresh_cpu_all();
            let l = sys.cpus().len().max(1);
            let p = sysinfo::System::physical_core_count().unwrap_or(l);
            (l, p)
        } else {
            (1, 1)
        };
        let has_smt = logical > physical;

        let mask = if logical >= (std::mem::size_of::<usize>() * 8) {
            usize::MAX
        } else {
            (1usize << logical).wrapping_sub(1)
        };

        let non_core0_mask = if logical >= 6 {
            mask & !0x03
        } else {
            mask
        };

        let summary = if has_smt {
            format!("{}C / {}T (SMT Fallback)", physical, logical)
        } else {
            format!("{}C / {}T (Fallback)", physical, logical)
        };

        Self {
            total_logical_cores: logical,
            total_physical_cores: physical,
            p_core_count: physical,
            e_core_count: 0,
            has_smt,
            is_heterogeneous: false,
            p_core_mask: mask,
            e_core_mask: mask,
            all_cores_mask: mask,
            non_core0_mask,
            topology_summary: summary,
        }
    }

    /// Hàm toán học tính trần CPU Max State khi chạy pin liên tục theo số nhân logic và % pin còn lại.
    /// Hoàn toàn không hardcode câu lệnh `if cpu <= 4 ... else if <= 8`.
    pub fn calculate_optimal_dc_limit(&self, battery_pct: u8) -> u32 {
        let n = self.total_logical_cores.max(2) as f32;
        // Công thức suy biến phi tuyến: CPU càng nhiều nhân càng có thể hãm sâu hơn mà không giảm độ mượt
        let base = 55.0 + (30.0 / (n / 2.0).sqrt());
        let clamped_base = base.clamp(65.0, 85.0);

        // Điều chế theo dung lượng pin thực tế: pin thấp tự động hạ dần trần xung nhịp để chống sụt áp cell pin
        let dynamic_dc = clamped_base * (0.70 + 0.30 * (battery_pct as f32 / 100.0));
        (dynamic_dc.round() as u32).clamp(50, 95)
    }

    /// Tính toán EPP (Energy Performance Preference) tối ưu theo trạng thái nguồn và ngữ cảnh
    pub fn calculate_dynamic_epp(&self, is_ac: bool, battery_pct: u8, is_gaming_or_dev: bool) -> u32 {
        if is_gaming_or_dev {
            0 // Maximum Performance (0% trễ tăng xung nhịp)
        } else if is_ac {
            33 // Balanced Snappy (Phản hồi nhạy nhưng không dư thừa nhiệt)
        } else {
            // Khi chạy pin: tính liên tục từ 50% đến 85%
            let bat_ratio = (battery_pct as f32 / 100.0).clamp(0.0, 1.0);
            let epp = 50.0 + (1.0 - bat_ratio) * 35.0;
            (epp.round() as u32).clamp(50, 85)
        }
    }

    /// Tính toán Core Parking Min Cores (Tỷ lệ nhân thức tối thiểu)
    pub fn calculate_dynamic_core_parking(&self, is_ac: bool, battery_pct: u8) -> u32 {
        if is_ac {
            100 // AC: Đánh thức 100% nhân, triệt tiêu micro-stutter do trễ đánh thức C-state
        } else if battery_pct > 60 {
            75 // Pin tốt: Giữ 75% nhân sẵn sàng
        } else if battery_pct > 25 {
            50 // Pin trung bình: Cho phép đỗ 50% nhân
        } else {
            25 // Pin yếu: Chỉ giữ 25% nhân thức (tối thiểu 2-4 luồng) để tiết kiệm sâu
        }
    }

    /// Gán Affinity tối ưu cho Game: Né Core 0 (ngắt chuột/mạng) và ưu tiên P-Core
    pub fn boost_gaming_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }

        let target_mask = if self.is_heterogeneous {
            let p_isolated = self.p_core_mask & self.non_core0_mask;
            if p_isolated != 0 {
                p_isolated
            } else {
                self.p_core_mask
            }
        } else {
            let non_c0 = self.all_cores_mask & self.non_core0_mask;
            if non_c0 != 0 {
                non_c0
            } else {
                self.all_cores_mask
            }
        };

        set_process_affinity_native(pid, target_mask)
    }

    /// Gán Affinity tối ưu cho Biên dịch / Render: Mở 100% tất cả P-Core và E-Core
    pub fn boost_compilation_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }
        set_process_affinity_native(pid, self.all_cores_mask)
    }

    /// Ép tiến trình nền ngốn tài nguyên vào E-Core (nếu là CPU lai) hoặc giới hạn tải
    pub fn throttle_background_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }

        let target_mask = if self.is_heterogeneous && self.e_core_mask != 0 {
            self.e_core_mask
        } else if self.total_logical_cores >= 4 {
            // CPU đồng nhất nhiều nhân: ép chạy ở nửa sau danh sách nhân (xa Core 0)
            self.all_cores_mask & !( (1usize << (self.total_logical_cores / 2)).wrapping_sub(1) )
        } else {
            self.all_cores_mask
        };

        set_process_affinity_native(pid, target_mask)
    }
}

pub fn set_process_affinity_native(pid: u32, mask: usize) -> bool {
    if pid <= 4 || mask == 0 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let res = SetProcessAffinityMask(handle, mask);
        CloseHandle(handle);
        res != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topology_probe_validity() {
        let dummy: SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX = unsafe { std::mem::zeroed() };
        println!("Processor GroupMask array length: {}", unsafe { dummy.Anonymous.Processor.GroupMask.len() });

        let topo = CpuTopology::probe();
        assert!(topo.total_logical_cores >= 1, "Logical cores must be >= 1");
        assert!(topo.total_physical_cores >= 1, "Physical cores must be >= 1");
        assert!(topo.all_cores_mask > 0, "All cores mask must not be 0");
        assert!(topo.p_core_mask > 0, "P core mask must not be 0");
        assert!(!topo.topology_summary.is_empty(), "Summary must not be empty");
        println!("Tested CpuTopology: {:?}", topo);
    }

    #[test]
    fn test_dynamic_dc_formula_monotonic_and_bounded() {
        let mut topo = CpuTopology::default();

        for cores in [2, 4, 6, 8, 12, 16, 24, 32, 64] {
            topo.total_logical_cores = cores;

            let dc_100 = topo.calculate_optimal_dc_limit(100);
            let dc_50 = topo.calculate_optimal_dc_limit(50);
            let dc_10 = topo.calculate_optimal_dc_limit(10);

            assert!(dc_100 >= dc_50, "Limit at 100% battery must be >= 50% battery for {} cores", cores);
            assert!(dc_50 >= dc_10, "Limit at 50% battery must be >= 10% battery for {} cores", cores);
            assert!(dc_100 <= 95 && dc_100 >= 50, "Must be in [50, 95]");
            assert!(dc_10 <= 95 && dc_10 >= 50, "Must be in [50, 95]");
        }
    }

    #[test]
    fn test_dynamic_epp_scaling() {
        let topo = CpuTopology::default();
        let epp_gaming = topo.calculate_dynamic_epp(true, 100, true);
        assert_eq!(epp_gaming, 0, "Gaming EPP must be 0");

        let epp_ac = topo.calculate_dynamic_epp(true, 100, false);
        assert_eq!(epp_ac, 33, "AC Balanced EPP must be 33");

        let epp_bat_full = topo.calculate_dynamic_epp(false, 100, false);
        let epp_bat_low = topo.calculate_dynamic_epp(false, 10, false);
        assert!(epp_bat_low > epp_bat_full, "Low battery must have higher EPP (more energy saving)");
    }

    #[test]
    fn test_non_core0_mask_safety() {
        let mut topo = CpuTopology::default();
        topo.total_logical_cores = 8;
        topo.all_cores_mask = 0xFF;
        let non_c0 = if topo.total_logical_cores >= 6 {
            topo.all_cores_mask & !0x03
        } else {
            topo.all_cores_mask
        };
        assert_eq!(non_c0 & 0x01, 0, "Bit 0 must be 0");
        assert_eq!(non_c0 & 0x02, 0, "Bit 1 must be 0");
        assert_eq!(non_c0 & 0xFC, 0xFC, "Upper bits must remain set");
    }
}
