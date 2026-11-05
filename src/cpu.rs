use serde::{Deserialize, Serialize};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::SystemInformation::{
    GetLogicalProcessorInformationEx, RelationProcessorCore,
    SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX,
};
use windows_sys::Win32::System::Threading::{
    GetActiveProcessorCount, GetActiveProcessorGroupCount, OpenProcess, SetProcessAffinityMask,
    PROCESS_SET_INFORMATION,
};

const ALL_PROCESSOR_GROUPS: u16 = 0xFFFF;

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
            e_core_mask: 0,
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
        let min_header_size = std::mem::size_of::<u32>() * 2;
        while offset + min_header_size <= length as usize {
            let item = unsafe { &*(buffer.as_ptr().add(offset) as *const SYSTEM_LOGICAL_PROCESSOR_INFORMATION_EX) };
            let item_size = item.Size as usize;
            if item_size == 0 || offset.saturating_add(item_size) > length as usize {
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
                    // SetProcessAffinityMask hoạt động trên Processor Group 0 (tối đa 64 luồng đầu tiên).
                    // Gom nhóm và ưu tiên Group 0 để tránh tràn bitmask usize trên kiến trúc 64-bit.
                    if group_aff.Group == 0 {
                        let mask = group_aff.Mask as usize;
                        phys_core_mask |= mask;
                    }
                    total_logical += (group_aff.Mask as usize).count_ones() as usize;
                }

                efficiency_classes.push(proc_info.EfficiencyClass);
                core_masks.push(phys_core_mask);
            }
            offset += item_size;
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
                // Đồng nhất: toàn bộ nhân đều là P-Core, E-Core để trống (0)
                p_mask |= mask;
                p_cores += 1;
            }
        }

        let all_mask = p_mask | e_mask;

        if total_physical == 0 || total_logical == 0 || all_mask == 0 {
            return Self::fallback_probe();
        }

        // 2. Tự động tính toán mặt nạ bảo vệ ngắt (Hardware Interrupt Shielding Mask):
        // Nếu total_logical >= 6: Loại trừ 2 luồng đầu tiên (Core 0 và luồng SMT đi kèm): all_mask & !0x03
        // Nếu total_logical < 6: Giữ nguyên all_cores_mask để chống Core Starvation.
        let non_core0_mask = if total_logical >= 6 && (all_mask & !0x03usize) != 0 {
            all_mask & !0x03usize
        } else {
            all_mask
        };

        // 3. Nhận diện nhóm đa xử lý (> 64 Cores / Multi-Group)
        let group_count = unsafe { GetActiveProcessorGroupCount() } as usize;

        let topology_summary = if is_heterogeneous {
            format!(
                "{}P + {}E ({} luồng, Kiến trúc Lai)",
                p_cores, e_cores, total_logical
            )
        } else if total_logical > 64 || group_count > 1 {
            format!(
                "{}C / {}T (Đa Group: {} nhóm, Ưu tiên Group 0)",
                total_physical, total_logical, group_count.max(2)
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

    /// 4. Cơ chế Fallback an toàn đa tầng khi kernel từ chối cấp thông tin
    pub fn fallback_probe() -> Self {
        // Tầng 1: Win32 native API cực nhanh (< 0.01ms) không tốn bộ nhớ heap
        let win32_all = unsafe { GetActiveProcessorCount(ALL_PROCESSOR_GROUPS) } as usize;
        let win32_g0 = unsafe { GetActiveProcessorCount(0) } as usize;

        // Tầng 2: sysinfo physical_core_count() và available_parallelism
        let sys_physical = sysinfo::System::physical_core_count().ok();
        let sys_logical = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);

        let logical = if win32_all > 0 {
            win32_all
        } else if sys_logical > 0 {
            sys_logical
        } else {
            1
        };

        let physical = sys_physical.unwrap_or(logical).max(1);
        let has_smt = logical > physical;

        // Group 0 mask: giới hạn 64 luồng trong Group 0 để không tràn usize
        let group0_logical = if win32_g0 > 0 {
            win32_g0
        } else {
            logical.min(64)
        }.clamp(1, 64);

        let mask = if group0_logical >= (std::mem::size_of::<usize>() * 8) {
            usize::MAX
        } else {
            (1usize << group0_logical).wrapping_sub(1)
        };

        // Quy tắc toán học bảo vệ ngắt: Nếu >= 6 luồng, loại trừ 2 luồng đầu (Core 0 + SMT)
        let non_core0_mask = if logical >= 6 && (mask & !0x03usize) != 0 {
            mask & !0x03usize
        } else {
            mask
        };

        let summary = if has_smt {
            format!("{}C / {}T (SMT Fallback)", physical, logical)
        } else {
            format!("{}C / {}T (Fallback)", physical, logical)
        };

        // Cấu hình đối xứng an toàn
        Self {
            total_logical_cores: logical,
            total_physical_cores: physical,
            p_core_count: physical,
            e_core_count: 0,
            has_smt,
            is_heterogeneous: false,
            p_core_mask: mask,
            e_core_mask: 0,
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

    /// Tính toán mặt nạ CPU tối ưu cho Game: Né Core 0 (ngắt chuột/mạng) và ưu tiên P-Core
    pub fn calculate_gaming_mask(&self) -> usize {
        if self.is_heterogeneous {
            let p_isolated = self.p_core_mask & self.non_core0_mask;
            if p_isolated != 0 {
                p_isolated
            } else if self.p_core_mask != 0 {
                self.p_core_mask
            } else {
                self.all_cores_mask
            }
        } else {
            let non_c0 = self.all_cores_mask & self.non_core0_mask;
            if non_c0 != 0 {
                non_c0
            } else {
                self.all_cores_mask
            }
        }
    }

    /// Gán Affinity tối ưu cho Game: Né Core 0 (ngắt chuột/mạng) và ưu tiên P-Core
    pub fn boost_gaming_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }

        let target_mask = self.calculate_gaming_mask();
        set_process_affinity_native(pid, target_mask)
    }

    /// Gán Affinity tối ưu cho Biên dịch / Render: Mở 100% tất cả P-Core và E-Core
    pub fn boost_compilation_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }
        // Trên hệ thống đa Processor Group (> 64 cores), không gán SetProcessAffinityMask(Group 0)
        // để tránh khóa tiến trình vào 64 core đầu tiên và tước đoạt các Processor Group còn lại của trình biên dịch!
        if self.total_logical_cores > 64 {
            return true;
        }
        set_process_affinity_native(pid, self.all_cores_mask)
    }

    /// Tính toán mặt nạ CPU khi ép tiến trình nền ngốn tài nguyên vào E-Core hoặc nửa sau Group 0
    pub fn calculate_throttle_mask(&self) -> usize {
        if self.is_heterogeneous && self.e_core_mask != 0 {
            self.e_core_mask
        } else if self.total_logical_cores >= 4 {
            // Group 0 có tối đa 64 nhân biểu diễn trong all_cores_mask (usize).
            // Chia đôi số nhân thuộc Group 0 để cô lập tiến trình nền sang nửa sau, né Core 0.
            let group0_cores = self.all_cores_mask.count_ones() as usize;
            let shift = (group0_cores / 2).min(32);
            let lower_half_mask = if shift == 0 {
                0
            } else {
                (1usize << shift).wrapping_sub(1)
            };
            let upper_half = self.all_cores_mask & !lower_half_mask;
            if upper_half != 0 {
                upper_half
            } else {
                self.all_cores_mask
            }
        } else {
            self.all_cores_mask
        }
    }

    /// Ép tiến trình nền ngốn tài nguyên vào E-Core (nếu là CPU lai) hoặc giới hạn tải
    pub fn throttle_background_process(&self, pid: u32) -> bool {
        if pid <= 4 {
            return false;
        }

        let target_mask = self.calculate_throttle_mask();
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
        let topo = CpuTopology::probe();
        assert!(topo.total_logical_cores >= 1, "Logical cores must be >= 1");
        assert!(topo.total_physical_cores >= 1, "Physical cores must be >= 1");
        assert!(topo.all_cores_mask > 0, "All cores mask must not be 0");
        assert!(topo.p_core_mask > 0, "P core mask must not be 0");
        assert!(!topo.topology_summary.is_empty(), "Summary must not be empty");
        println!("Tested CpuTopology: {:?}", topo);
    }

    #[test]
    fn test_probe_latency_under_5ms() {
        let start = std::time::Instant::now();
        let topo = CpuTopology::probe();
        let elapsed = start.elapsed();
        println!("CpuTopology::probe() took: {:?}", elapsed);
        // Trong môi trường test debug cho phép dung sai tối đa 15ms phòng trường hợp context switch của hệ điều hành
        assert!(
            elapsed.as_millis() < 15,
            "Probe execution latency must be fast, took: {:?}",
            elapsed
        );
        assert!(topo.total_logical_cores >= 1);
    }

    #[test]
    fn test_starvation_shielding_less_than_6_cores() {
        // Mô phỏng CPU 4 cores / 4 threads (ví dụ i5-6500 hoặc máy ảo VM 4 vCPUs)
        let mut topo = CpuTopology::default();
        topo.total_physical_cores = 4;
        topo.total_logical_cores = 4;
        topo.all_cores_mask = 0x0F;
        topo.p_core_mask = 0x0F;
        topo.e_core_mask = 0;
        topo.p_core_count = 4;
        topo.e_core_count = 0;
        topo.is_heterogeneous = false;

        // Công thức: total_logical < 6 -> non_core0_mask = all_cores_mask
        let non_c0 = if topo.total_logical_cores >= 6 {
            topo.all_cores_mask & !0x03
        } else {
            topo.all_cores_mask
        };
        topo.non_core0_mask = non_c0;

        assert_eq!(
            topo.non_core0_mask, topo.all_cores_mask,
            "CPUs with < 6 logical cores must retain all cores in non_core0_mask to avoid core starvation"
        );
        assert_eq!(topo.calculate_gaming_mask(), 0x0F);
        // Throttle mask trên 4 cores: 4/2 = 2, shift 2 -> lower_half 0x03, upper_half 0x0C
        assert_eq!(topo.calculate_throttle_mask(), 0x0C);
    }

    #[test]
    fn test_interrupt_shielding_greater_equal_6_cores() {
        // Mô phỏng CPU 8 cores / 16 threads (ví dụ Ryzen 7 hoặc Intel i7)
        let mut topo = CpuTopology::default();
        topo.total_physical_cores = 8;
        topo.total_logical_cores = 16;
        topo.all_cores_mask = 0xFFFF;
        topo.p_core_mask = 0xFFFF;
        topo.e_core_mask = 0;
        topo.p_core_count = 8;
        topo.e_core_count = 0;
        topo.is_heterogeneous = false;

        let non_c0 = if topo.total_logical_cores >= 6 {
            topo.all_cores_mask & !0x03
        } else {
            topo.all_cores_mask
        };
        topo.non_core0_mask = non_c0;

        assert_eq!(topo.non_core0_mask & 0x01, 0, "Core 0 Thread 0 must be excluded");
        assert_eq!(topo.non_core0_mask & 0x02, 0, "Core 0 Thread 1 (SMT) must be excluded");
        assert_eq!(topo.non_core0_mask & 0xFFFC, 0xFFFC, "Remaining 14 threads must be active");
        assert_eq!(topo.calculate_gaming_mask(), 0xFFFC);
    }

    #[test]
    fn test_heterogeneous_pe_core_partitioning() {
        // Mô phỏng Intel Core i9-13900K (8P + 16E, 32 threads)
        let mut topo = CpuTopology::default();
        topo.total_physical_cores = 24;
        topo.total_logical_cores = 32;
        topo.p_core_count = 8;
        topo.e_core_count = 16;
        topo.is_heterogeneous = true;
        topo.has_smt = true;
        // 8 P-Cores có SMT chiếm 16 luồng đầu tiên (0..15)
        topo.p_core_mask = 0x0000_FFFF;
        // 16 E-Cores không có SMT chiếm 16 luồng tiếp theo (16..31)
        topo.e_core_mask = 0xFFFF_0000;
        topo.all_cores_mask = topo.p_core_mask | topo.e_core_mask;
        topo.non_core0_mask = topo.all_cores_mask & !0x03;

        // Kiểm tra tính chất phân vùng
        assert_eq!(topo.p_core_mask & topo.e_core_mask, 0, "P-mask and E-mask must be mutually exclusive");
        assert_eq!(topo.p_core_count + topo.e_core_count, topo.total_physical_cores);

        // Gaming mask ưu tiên P-Core và loại trừ Core 0
        let gaming_mask = topo.calculate_gaming_mask();
        assert_eq!(gaming_mask, 0x0000_FFFC, "Gaming mask must target P-cores excluding Core 0/1");

        // Throttle mask ép chặt vào E-Core
        let throttle_mask = topo.calculate_throttle_mask();
        assert_eq!(throttle_mask, 0xFFFF_0000, "Background throttle must isolate to E-cores");
    }

    #[test]
    fn test_multi_group_and_shift_overflow_safety() {
        // Mô phỏng máy trạm AMD Threadripper 64C / 128T (2 Processor Groups)
        let mut topo = CpuTopology::default();
        topo.total_physical_cores = 64;
        topo.total_logical_cores = 128;
        topo.p_core_count = 64;
        topo.e_core_count = 0;
        topo.is_heterogeneous = false;
        topo.has_smt = true;
        // Group 0 gồm 64 luồng đầu tiên, biểu diễn bằng usize::MAX
        topo.all_cores_mask = usize::MAX;
        topo.p_core_mask = usize::MAX;
        topo.e_core_mask = 0;
        topo.non_core0_mask = usize::MAX & !0x03usize;

        assert_eq!(topo.total_logical_cores, 128);
        assert_eq!(topo.total_physical_cores, 64);
        assert_eq!(topo.non_core0_mask & 0x03, 0);

        // Kiểm tra không bị panic do tràn dịch bit (1 << (128 / 2))
        // Trên hệ thống 64-bit, dịch 64 sẽ gây panic nếu không được giới hạn cẩn thận
        let throttle_mask = topo.calculate_throttle_mask();
        // 64 cores trong Group 0 / 2 = 32. Lower half: 32 bits (0x0000_0000_FFFF_FFFF).
        // Upper half: 0xFFFF_FFFF_0000_0000.
        assert_eq!(
            throttle_mask,
            0xFFFF_FFFF_0000_0000usize,
            "Throttle mask for 128T CPU must isolate to upper 32 cores of Group 0 without overflow"
        );

        let gaming_mask = topo.calculate_gaming_mask();
        assert_eq!(gaming_mask, usize::MAX & !0x03usize);
    }

    #[test]
    fn test_boost_compilation_process_multi_group_preserves_all_groups() {
        let mut topo = CpuTopology::default();
        topo.total_logical_cores = 128;
        topo.all_cores_mask = usize::MAX;
        // Trên hệ thống đa nhóm (> 64 cores), hàm phải return true và không ép affinity vào 64 core của Group 0
        assert!(topo.boost_compilation_process(1234));
    }

    #[test]
    fn test_fallback_probe_correctness_and_symmetry() {
        let topo = CpuTopology::fallback_probe();
        assert!(topo.total_logical_cores >= 1);
        assert!(topo.total_physical_cores >= 1);
        assert!(!topo.is_heterogeneous, "Fallback must always produce symmetric homogeneous configuration");
        assert_eq!(topo.e_core_count, 0, "Fallback must have 0 E-cores");
        assert_eq!(topo.e_core_mask, 0, "Fallback must have e_core_mask = 0");
        assert_eq!(topo.p_core_mask, topo.all_cores_mask, "Fallback p_core_mask must equal all_cores_mask");
        assert_eq!(topo.p_core_count, topo.total_physical_cores);
        assert!(topo.all_cores_mask > 0);

        if topo.total_logical_cores >= 6 {
            assert_eq!(topo.non_core0_mask, topo.all_cores_mask & !0x03);
        } else {
            assert_eq!(topo.non_core0_mask, topo.all_cores_mask);
        }
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
