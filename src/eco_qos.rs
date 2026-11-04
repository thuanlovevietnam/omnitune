use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use sysinfo::{Pid, System};
use windows_sys::Win32::Foundation::CloseHandle;
use windows_sys::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, SetPriorityClass,
    SetProcessInformation, ABOVE_NORMAL_PRIORITY_CLASS, HIGH_PRIORITY_CLASS,
    PROCESS_POWER_THROTTLING_CURRENT_VERSION, PROCESS_POWER_THROTTLING_EXECUTION_SPEED,
    PROCESS_POWER_THROTTLING_STATE, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SET_INFORMATION,
    ProcessPowerThrottling,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId};

pub const ECO_CANDIDATES: &[&str] = &[
    "googledrivefs",
    "onedrive",
    "searchindexer",
    "tailscale",
    "dropbox",
];

const GENERIC_HOSTS: &[&str] = &[
    "svchost.exe",
    "cmd.exe",
    "powershell.exe",
    "pwsh.exe",
    "conhost.exe",
    "dllhost.exe",
    "rundll32.exe",
    "taskhostw.exe",
    "wsmprovhost.exe",
    "runtimebroker.exe",
    "explorer.exe",
];

fn get_process_name_win32(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok != 0 && size > 0 {
            let path_str = String::from_utf16_lossy(&buf[..size as usize]);
            std::path::Path::new(&path_str)
                .file_name()
                .and_then(|f| f.to_str())
                .map(|s| s.to_string())
        } else {
            None
        }
    }
}

fn get_process_exe_path_win32(pid: u32) -> Option<PathBuf> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if handle.is_null() {
            return None;
        }
        let mut buf = [0u16; 1024];
        let mut size = buf.len() as u32;
        let ok = QueryFullProcessImageNameW(handle, 0, buf.as_mut_ptr(), &mut size);
        CloseHandle(handle);

        if ok != 0 && size > 0 {
            Some(PathBuf::from(String::from_utf16_lossy(&buf[..size as usize])))
        } else {
            None
        }
    }
}

pub fn find_focus_family_pids(sys: &System, fg_pid: u32) -> HashSet<u32> {
    let mut family = HashSet::new();
    let my_pid = std::process::id();

    if fg_pid <= 4 || fg_pid == my_pid {
        return family;
    }
    family.insert(fg_pid);

    let fg_sys_pid = Pid::from_u32(fg_pid);
    let fg_proc = sys.process(fg_sys_pid);

    let fg_name = fg_proc
        .map(|p| p.name().to_string_lossy().to_string())
        .or_else(|| get_process_name_win32(fg_pid));
    let fg_exe = fg_proc
        .and_then(|p| p.exe().map(|e| e.to_path_buf()))
        .or_else(|| get_process_exe_path_win32(fg_pid));

    let fg_name_lower = fg_name.as_ref().map(|n| n.to_lowercase());

    let is_generic = fg_name_lower
        .as_ref()
        .map(|n| {
            GENERIC_HOSTS.iter().any(|&g| {
                n == g || n == g.trim_end_matches(".exe")
            })
        })
        .unwrap_or(false);

    // 1. Leo ngược cây tìm Ancestors cùng executable
    let mut curr_pid = fg_sys_pid;
    let mut visited_ancestors = HashSet::new();
    visited_ancestors.insert(fg_pid);

    for _ in 0..16 {
        let Some(p_proc) = sys.process(curr_pid) else { break; };
        let Some(parent_sys_pid) = p_proc.parent() else { break; };
        let p_raw = parent_sys_pid.as_u32();

        if p_raw <= 4 || p_raw == my_pid || !visited_ancestors.insert(p_raw) {
            break;
        }

        let Some(parent_proc) = sys.process(parent_sys_pid) else { break; };
        let parent_name = parent_proc.name().to_string_lossy().to_lowercase();

        if parent_name == "explorer.exe"
            || parent_name == "services.exe"
            || parent_name == "winlogon.exe"
        {
            break;
        }

        let is_same = if let Some(ref target_n) = fg_name_lower {
            parent_name == *target_n
                || parent_name.trim_end_matches(".exe") == target_n.trim_end_matches(".exe")
        } else {
            false
        };

        if is_same {
            family.insert(p_raw);
            curr_pid = parent_sys_pid;
        } else {
            break;
        }
    }

    // 2. Duyệt 1 vòng gom process cùng exe và dựng sơ đồ quan hệ cha - con
    let mut children_map: HashMap<u32, Vec<u32>> = HashMap::new();

    for (&p_pid, proc) in sys.processes() {
        let raw = p_pid.as_u32();
        if raw <= 4 || raw == my_pid {
            continue;
        }

        if let Some(parent) = proc.parent() {
            let p_raw = parent.as_u32();
            if p_raw > 4 && p_raw != my_pid {
                children_map.entry(p_raw).or_default().push(raw);
            }
        }

        if !is_generic {
            let matches_exe = match (&fg_exe, proc.exe()) {
                (Some(target_path), Some(p_path)) => target_path == p_path,
                _ => match (&fg_name_lower, proc.name().to_str()) {
                    (Some(target_n), Some(pn)) => {
                        let pn_clean = pn.to_lowercase();
                        let t_clean = target_n.to_lowercase();
                        pn_clean == t_clean
                            || pn_clean.trim_end_matches(".exe")
                                == t_clean.trim_end_matches(".exe")
                    }
                    _ => false,
                },
            };
            if matches_exe {
                family.insert(raw);
            }
        }
    }

    // 3. BFS mở rộng toàn bộ con cháu (descendants)
    let mut queue: VecDeque<u32> = family.iter().copied().collect();
    while let Some(current) = queue.pop_front() {
        if let Some(children) = children_map.get(&current) {
            for &child in children {
                if family.insert(child) {
                    queue.push_back(child);
                }
            }
        }
    }

    family.remove(&0);
    family.remove(&4);
    family.remove(&my_pid);
    family
}

pub fn get_foreground_process(sys: &System) -> Option<(u32, String)> {
    unsafe {
        let hwnd = GetForegroundWindow();
        if hwnd.is_null() {
            return None;
        }
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if pid <= 4 || pid == std::process::id() {
            return None;
        }

        let name = sys
            .process(sysinfo::Pid::from_u32(pid))
            .map(|p| p.name().to_string_lossy().to_string())
            .or_else(|| get_process_name_win32(pid))
            .unwrap_or_else(|| format!("PID {}", pid));

        Some((pid, name))
    }
}

pub fn set_process_eco_mode(pid: u32, enable_eco: bool) -> bool {
    if pid <= 4 {
        return false;
    }

    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }

        let mut throttling_state: PROCESS_POWER_THROTTLING_STATE = std::mem::zeroed();
        throttling_state.Version = PROCESS_POWER_THROTTLING_CURRENT_VERSION;
        throttling_state.ControlMask = PROCESS_POWER_THROTTLING_EXECUTION_SPEED;
        throttling_state.StateMask = if enable_eco {
            PROCESS_POWER_THROTTLING_EXECUTION_SPEED
        } else {
            0
        };

        let res = SetProcessInformation(
            handle,
            ProcessPowerThrottling,
            &throttling_state as *const _ as *const _,
            std::mem::size_of::<PROCESS_POWER_THROTTLING_STATE>() as u32,
        );

        CloseHandle(handle);
        res != 0
    }
}

pub fn is_core_system_process(name: &str) -> bool {
    let lower = name.to_lowercase();
    let clean = lower.trim_end_matches(".exe");
    matches!(
        clean,
        "dwm"
            | "explorer"
            | "services"
            | "lsass"
            | "csrss"
            | "smss"
            | "winlogon"
            | "fontdrvhost"
            | "audiodg"
            | "spoolsv"
            | "taskhostw"
            | "system"
            | "registry"
            | "idle"
            | "omnitune"
    )
}

#[allow(dead_code)]
pub fn apply_foreground_boost_and_eco(
    sys: &System,
    fg_pid: Option<u32>,
) -> (usize, usize) {
    apply_foreground_boost_and_eco_with_topology(sys, fg_pid, None)
}

pub fn apply_foreground_boost_and_eco_with_topology(
    sys: &System,
    fg_pid: Option<u32>,
    topo: Option<&crate::cpu::CpuTopology>,
) -> (usize, usize) {
    let mut boosted = 0;
    let mut eco_throttled = 0;
    let my_pid = std::process::id();

    let focus_family = if let Some(pid) = fg_pid {
        find_focus_family_pids(sys, pid)
    } else {
        HashSet::new()
    };

    // 1. Boost active foreground process and its family (un-throttle and prioritize)
    for &pid in &focus_family {
        if pid <= 4 || pid == my_pid {
            continue;
        }
        unsafe {
            let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
            if !handle.is_null() {
                let _ = SetPriorityClass(handle, ABOVE_NORMAL_PRIORITY_CLASS);
                CloseHandle(handle);
            }
        }
        if set_process_eco_mode(pid, false) {
            boosted += 1;
        }
        if let Some(t) = topo {
            t.boost_compilation_process(pid); // Khôi phục affinity toàn bộ nhân
        }
    }

    // 2. Behavioral EcoQoS: Target background candidate names OR background processes using > 1.5% CPU
    for (&pid, proc) in sys.processes() {
        let raw_pid = pid.as_u32();
        if raw_pid <= 4 || raw_pid == my_pid || focus_family.contains(&raw_pid) {
            continue;
        }

        let name = proc.name().to_string_lossy().to_lowercase();
        if is_core_system_process(&name) {
            continue;
        }

        let is_candidate = ECO_CANDIDATES.iter().any(|&c| name.contains(c));
        let is_high_cpu_bg = proc.cpu_usage() > 1.5;

        if (is_candidate || is_high_cpu_bg) && set_process_eco_mode(raw_pid, true) {
            eco_throttled += 1;
            if let Some(t) = topo {
                t.throttle_background_process(raw_pid); // Ép chạy trên E-core hoặc nửa sau nhân
            }
        }
    }

    (boosted, eco_throttled)
}

pub fn apply_pro_foreground_boost_with_topology(
    pid: u32,
    topo: &crate::cpu::CpuTopology,
    is_gaming: bool,
) -> bool {
    if pid <= 4 {
        return false;
    }
    unsafe {
        let handle = OpenProcess(PROCESS_SET_INFORMATION, 0, pid);
        if handle.is_null() {
            return false;
        }
        let _ = SetPriorityClass(handle, HIGH_PRIORITY_CLASS);
        CloseHandle(handle);
    }

    if is_gaming {
        topo.boost_gaming_process(pid)
    } else {
        topo.boost_compilation_process(pid)
    }
}

#[allow(dead_code)]
pub fn apply_pro_foreground_boost(pid: u32, _total_cores: usize) -> bool {
    let topo = crate::cpu::CpuTopology::probe();
    apply_pro_foreground_boost_with_topology(pid, &topo, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eco_candidates_no_browsers() {
        let forbidden = ["chrome", "msedge", "brave", "firefox", "opera", "coccoc"];
        for b in forbidden {
            assert!(
                !ECO_CANDIDATES.contains(&b),
                "Candidate list must not contain browser: {}",
                b
            );
        }
    }

    #[test]
    fn test_find_focus_family_system_pids() {
        let sys = System::new().expect("failed to initialize sysinfo");
        let family_0 = find_focus_family_pids(&sys, 0);
        let family_4 = find_focus_family_pids(&sys, 4);
        assert!(family_0.is_empty());
        assert!(family_4.is_empty());
    }

    #[test]
    fn test_find_focus_family_excludes_self() {
        let mut sys = System::new().expect("failed to initialize sysinfo");
        sys.refresh_processes(sysinfo::ProcessesToUpdate::All, true);
        let my_pid = std::process::id();
        let family = find_focus_family_pids(&sys, my_pid);
        assert!(!family.contains(&0));
        assert!(!family.contains(&4));
        assert!(!family.contains(&my_pid));
    }
}
