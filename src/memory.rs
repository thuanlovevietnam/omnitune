use crate::types::ProcessMemInfo;
use sysinfo::System;
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::ProcessStatus::{
    K32EmptyWorkingSet, K32GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
};
use windows_sys::Win32::System::SystemInformation::{GlobalMemoryStatusEx, MEMORYSTATUSEX};
use windows_sys::Win32::System::Threading::{
    OpenProcess, TerminateProcess, PROCESS_QUERY_INFORMATION, PROCESS_SET_QUOTA,
    PROCESS_TERMINATE,
};

#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct MemStats {
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub free_bytes: u64,
    pub load_percent: u32,
}

pub fn get_memory_stats() -> MemStats {
    unsafe {
        let mut status: MEMORYSTATUSEX = std::mem::zeroed();
        status.dwLength = std::mem::size_of::<MEMORYSTATUSEX>() as u32;
        if GlobalMemoryStatusEx(&mut status) != 0 {
            let total = status.ullTotalPhys;
            let free = status.ullAvailPhys;
            let used = total.saturating_sub(free);
            return MemStats {
                total_bytes: total,
                used_bytes: used,
                free_bytes: free,
                load_percent: status.dwMemoryLoad,
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
        })
        .collect();

    list.sort_by(|a, b| b.memory_bytes.cmp(&a.memory_bytes));
    list.truncate(limit);
    list
}

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
