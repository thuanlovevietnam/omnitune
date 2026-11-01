use crate::memory::trim_working_sets;
use std::sync::atomic::{AtomicBool, Ordering};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows_sys::Win32::UI::Shell::{
    Shell_NotifyIconW, NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    FindWindowW, GetCursorPos, GetMessageW, LoadIconW, PostQuitMessage, RegisterClassW,
    SetForegroundWindow, ShowWindow, TrackPopupMenu, TranslateMessage, IDI_APPLICATION,
    MF_SEPARATOR, MF_STRING, MSG, SW_RESTORE, SW_SHOW, TPM_BOTTOMALIGN, TPM_LEFTALIGN, WM_APP,
    WM_COMMAND, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_RBUTTONUP, WNDCLASSW,
    WS_OVERLAPPED,
};

pub static SHOW_WINDOW_REQUESTED: AtomicBool = AtomicBool::new(false);

const WM_TRAY_MSG: u32 = WM_APP + 101;
const ID_TRAY_OPEN: usize = 1001;
const ID_TRAY_CLEAN_RAM: usize = 1002;
const ID_TRAY_EXIT: usize = 1003;

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub fn restore_main_window() {
    SHOW_WINDOW_REQUESTED.store(true, Ordering::SeqCst);
    unsafe {
        let title = to_wide("OmniTune - Trợ Lý Tối Ưu Hệ Thống");
        let hwnd = FindWindowW(std::ptr::null(), title.as_ptr());
        if !hwnd.is_null() {
            ShowWindow(hwnd, SW_SHOW);
            ShowWindow(hwnd, SW_RESTORE);
            SetForegroundWindow(hwnd);
        }
    }
}

unsafe extern "system" fn tray_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_TRAY_MSG => {
            let event = lparam as u32;
            match event {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => {
                    restore_main_window();
                }
                WM_RBUTTONUP => unsafe {
                    let mut pt: POINT = std::mem::zeroed();
                    GetCursorPos(&mut pt);

                    let hmenu = CreatePopupMenu();
                    let text_open = to_wide("⚡ Mở OmniTune");
                    let text_ram = to_wide("🧹 Dọn RAM tức thì");
                    let text_exit = to_wide("❌ Thoát hoàn toàn");

                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, text_open.as_ptr());
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_CLEAN_RAM, text_ram.as_ptr());
                    AppendMenuW(hmenu, MF_SEPARATOR, 0, std::ptr::null());
                    AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, text_exit.as_ptr());

                    SetForegroundWindow(hwnd);
                    TrackPopupMenu(
                        hmenu,
                        TPM_BOTTOMALIGN | TPM_LEFTALIGN,
                        pt.x,
                        pt.y,
                        0,
                        hwnd,
                        std::ptr::null(),
                    );
                    DestroyMenu(hmenu);
                },
                _ => {}
            }
            0
        }
        WM_COMMAND => {
            let id = (wparam & 0xFFFF) as usize;
            match id {
                ID_TRAY_OPEN => {
                    restore_main_window();
                }
                ID_TRAY_CLEAN_RAM => {
                    std::thread::spawn(|| {
                        if let Ok(mut sys) = sysinfo::System::new() {
                            trim_working_sets(&mut sys);
                        }
                    });
                }
                ID_TRAY_EXIT => {
                    remove_tray_icon(hwnd);
                    std::process::exit(0);
                }
                _ => {}
            }
            0
        }
        WM_DESTROY => unsafe {
            remove_tray_icon(hwnd);
            PostQuitMessage(0);
            0
        },
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

pub fn remove_tray_icon(hwnd: HWND) {
    unsafe {
        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 101;
        Shell_NotifyIconW(NIM_DELETE, &nid);
    }
}

pub fn start_tray_service() {
    std::thread::spawn(|| unsafe {
        let class_name = to_wide("OmniTuneTrayWndClass");
        let hicon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION);

        let mut wc: WNDCLASSW = std::mem::zeroed();
        wc.lpfnWndProc = Some(tray_wndproc);
        wc.lpszClassName = class_name.as_ptr();
        wc.hIcon = hicon;

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            0,
            class_name.as_ptr(),
            class_name.as_ptr(),
            WS_OVERLAPPED,
            0,
            0,
            0,
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        );

        if hwnd.is_null() {
            return;
        }

        let mut nid: NOTIFYICONDATAW = std::mem::zeroed();
        nid.cbSize = std::mem::size_of::<NOTIFYICONDATAW>() as u32;
        nid.hWnd = hwnd;
        nid.uID = 101;
        nid.uFlags = NIF_ICON | NIF_MESSAGE | NIF_TIP;
        nid.uCallbackMessage = WM_TRAY_MSG;
        nid.hIcon = hicon;

        let tip = "OmniTune - Trợ Lý Tối Ưu Hệ Thống (Đang chạy ngầm)";
        for (i, c) in tip.encode_utf16().enumerate().take(127) {
            nid.szTip[i] = c;
        }

        Shell_NotifyIconW(NIM_ADD, &nid);

        let mut msg: MSG = std::mem::zeroed();
        while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    });
}
