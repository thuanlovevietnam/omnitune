use crate::types::ProcessMemInfo;
use sysinfo::System;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::Security::{
    AdjustTokenPrivileges, LookupPrivilegeValueW, LUID_AND_ATTRIBUTES, TOKEN_ADJUST_PRIVILEGES,
    TOKEN_PRIVILEGES, TOKEN_QUERY, SE_PRIVILEGE_ENABLED,
};
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcess, OpenProcess, OpenProcessToken, TerminateProcess, PROCESS_QUERY_INFORMATION,
    PROCESS_SET_QUOTA, PROCESS_TERMINATE,
};

#[link(name = "ntdll")]
unsafe extern "system" {
    fn NtSetSystemInformation(
        system_information_class: u32,
        system_information: *const std::ffi::c_void,
        system_information_length: u32,
    ) -> i32;
}

const SYSTEM_MEMORY_LIST_INFORMATION: u32 = 80;
const MEMORY_PURGE_STANDBY_LIST: u32 = 4;

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MemStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub load_percent: u32,
    pub pagefile_total_bytes: u64,
    pub pagefile_used_bytes: u64,
}

pub fn get_memory_stats() -> MemStats {
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            let total = status.ullTotalPhys;
            let free = status.ullAvailPhys;
            let used = total.saturating_sub(free);
            let pf_total = status.ullTotalPageFile;
            let pf_free = status.ullAvailPageFile;
            let pf_used = pf_total.saturating_sub(pf_free);
            return MemStats {
                total_bytes: total,
                used_bytes: used,
                free_bytes: free,
                load_percent: status.dwMemoryLoad,
                pagefile_total_bytes: pf_total,
                pagefile_used_bytes: pf_used,
            };
        }
    }

    MemStats::default()
}

pub fn get_top_processes(sys: &mut System, limit: usize) -> Vec<ProcessMemInfo> {
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut list: Vec<ProcessMemInfo> = sys
        .processes()
        .iter()
        .map(|(pid, proc)| ProcessMemInfo {
            pid: pid.as_u32(),
            name: proc.name().to_string_lossy().to_string(),
            memory_bytes: proc.memory(),
            cpu_usage: proc.cpu_usage(),
        })
        .collect();

    list.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    list.truncate(limit);
    list
}

#[allow(dead_code)]
pub fn get_top_cpu_processes(sys: &mut System, limit: usize) -> Vec<ProcessMemInfo> {
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let mut list: Vec<ProcessMemInfo> = sys
        .processes()
        .iter()
        .map(|(pid, proc)| ProcessMemInfo {
            pid: pid.as_u32(),
            name: proc.name().to_string_lossy().to_string(),
            memory_bytes: proc.memory(),
            cpu_usage: proc.cpu_usage(),
        })
        .collect();

    list.sort_by(|a, b| b.cpu_usage.partial_cmp(&a.cpu_usage).unwrap_or(std::cmp::Ordering::Equal));
    list.truncate(limit);
    list
}

/// Kích hoạt đặc quyền SeProfileSingleProcessPrivilege cần thiết để can thiệp Windows Memory Manager.
fn enable_profile_privilege() -> bool {
    unsafe {
        let mut token = std::ptr::null_mut();
        if OpenProcessToken(
            GetCurrentProcess(),
            TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY,
            &mut token,
        ) == 0
        {
            return false;
        }

        let priv_name: Vec<u16> = "SeProfileSingleProcessPrivilege\0"
            .encode_utf16()
            .collect();
        let mut luid = std::mem::zeroed();
        if LookupPrivilegeValueW(std::ptr::null(), priv_name.as_ptr(), &mut luid) == 0 {
            CloseHandle(token);
            return false;
        }

        let tp = TOKEN_PRIVILEGES {
            PrivilegeCount: 1,
            Privileges: [LUID_AND_ATTRIBUTES {
                Luid: luid,
                Attributes: SE_PRIVILEGE_ENABLED,
            }],
        };

        let res = AdjustTokenPrivileges(
            token,
            0,
            &tp,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );
        CloseHandle(token);
        res != 0
    }
}

/// Dọn sạch Standby List về danh sách bộ nhớ trống (Free Memory List) qua NtSetSystemInformation.
pub fn purge_standby_list() -> bool {
    enable_profile_privilege();
    let command: u32 = MEMORY_PURGE_STANDBY_LIST;
    unsafe {
        let status = NtSetSystemInformation(
            SYSTEM_MEMORY_LIST_INFORMATION,
            &command as *const u32 as *const std::ffi::c_void,
            std::mem::size_of::<u32>() as u32,
        );
        status >= 0
    }
}

/// Thu hồi bộ nhớ làm việc (Working Set) của mọi tiến trình qua K32EmptyWorkingSet.
///
/// Cơ chế kỹ thuật: K32EmptyWorkingSet loại bỏ các trang nhớ không sử dụng ra khỏi Working Set
/// và chuyển sang Standby List / Modified List; không xóa Standby List. Để xóa Standby List,
/// hãy dùng kèm `purge_standby_list()`.
pub fn trim_working_sets(sys: &mut System) -> (u64, usize) {
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);

    let my_pid = std::process::id();
    let mut trimmed_count = 0usize;
    let mut total_freed_bytes = 0u64;

    for (&pid, _) in sys.processes() {
        let raw_pid = pid.as_u32();
        if raw_pid <= 4 || raw_pid == my_pid {
            // Skip System Idle, System, and OmniTune itself
            continue;
        }

        unsafe {
            let handle = OpenProcess(
                PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA,
                0,
                raw_pid,
            );
            if !handle.is_null() {
                let mut pmc_before: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                pmc_before.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;

                let has_before = K32GetProcessMemoryInfo(
                    handle,
                    &mut pmc_before,
                    pmc_before.cb,
                ) != 0;

                if K32EmptyWorkingSet(handle) != 0 {
                    trimmed_count += 1;

                    if has_before {
                        let mut pmc_after: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
                        pmc_after.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
                        if K32GetProcessMemoryInfo(handle, &mut pmc_after, pmc_after.cb) != 0 {
                            let diff = pmc_before
                                .WorkingSetSize
                                .saturating_sub(pmc_after.WorkingSetSize)
                                as u64;
                            total_freed_bytes += diff;
                        }
                    }
                }
                CloseHandle(handle);
            }
        }
    }

    (total_freed_bytes, trimmed_count)
}

pub fn trim_single_process(pid: u32) -> u64 {
    if pid <= 4 {
        return 0;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_SET_QUOTA, 0, pid);
        if handle.is_null() {
            return 0;
        }

        let mut pmc_before: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
        pmc_before.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
        let has_before = K32GetProcessMemoryInfo(handle, &mut pmc_before, pmc_before.cb) != 0;

        let mut freed = 0u64;
        if K32EmptyWorkingSet(handle) != 0 && has_before {
            let mut pmc_after: PROCESS_MEMORY_COUNTERS = std::mem::zeroed();
            pmc_after.cb = std::mem::size_of::<PROCESS_MEMORY_COUNTERS>() as u32;
            if K32GetProcessMemoryInfo(handle, &mut pmc_after, pmc_after.cb) != 0 {
                freed = pmc_before
                    .WorkingSetSize
                    .saturating_sub(pmc_after.WorkingSetSize) as u64;
            }
        }

        CloseHandle(handle);
        freed
    }
}

pub fn terminate_process(pid: u32) -> bool {
    if pid <= 4 {
        return false;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_TERMINATE, 0, pid);
        if handle.is_null() {
            return false;
        }

        let res = TerminateProcess(handle, 1) != 0;
        CloseHandle(handle);
        res
    }
}
