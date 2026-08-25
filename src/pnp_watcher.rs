use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;

type BOOL = i32;
type DWORD = u32;
type HANDLE = *mut c_void;
type HDEVNOTIFY = *mut c_void;
type HWND = *mut c_void;
type LPARAM = isize;
type LRESULT = isize;
type UINT = u32;
type WPARAM = usize;

const DBT_DEVTYP_DEVICEINTERFACE: DWORD = 0x00000005;
const DBT_DEVICEARRIVAL: WPARAM = 0x8000;
const DBT_DEVICEREMOVECOMPLETE: WPARAM = 0x8004;
const DEVICE_NOTIFY_ALL_INTERFACE_CLASSES: DWORD = 0x00000004;
const WM_DEVICECHANGE: UINT = 0x0219;
const HWND_MESSAGE: HWND = -3isize as HWND;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default)]
struct GUID {
    data1: u32,
    data2: u16,
    data3: u16,
    data4: [u8; 8],
}

#[repr(C)]
struct DEV_BROADCAST_DEVICEINTERFACE_W {
    dbcc_size: DWORD,
    dbcc_devicetype: DWORD,
    dbcc_reserved: DWORD,
    dbcc_classguid: GUID,
    dbcc_name: [u16; 1],
}

#[repr(C)]
struct WNDCLASSEXW {
    cb_size: UINT,
    style: UINT,
    lpfn_wnd_proc: Option<unsafe extern "system" fn(HWND, UINT, WPARAM, LPARAM) -> LRESULT>,
    cb_cls_extra: i32,
    cb_wnd_extra: i32,
    h_instance: HANDLE,
    h_icon: HANDLE,
    h_cursor: HANDLE,
    h_br_background: HANDLE,
    lpsz_menu_name: *const u16,
    lpsz_class_name: *const u16,
    h_icon_sm: HANDLE,
}

#[repr(C)]
struct MSG {
    hwnd: HWND,
    message: UINT,
    w_param: WPARAM,
    l_param: LPARAM,
    time: DWORD,
    pt_x: i32,
    pt_y: i32,
}

#[link(name = "user32")]
extern "system" {
    fn RegisterClassExW(lpwcx: *const WNDCLASSEXW) -> u16;
    fn CreateWindowExW(dw_ex_style: DWORD, lp_class_name: *const u16, lp_window_name: *const u16, dw_style: DWORD, x: i32, y: i32, n_width: i32, n_height: i32, h_wnd_parent: HWND, h_menu: HANDLE, h_instance: HANDLE, lp_param: *mut c_void) -> HWND;
    fn DefWindowProcW(h_wnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT;
    fn GetMessageW(lp_msg: *mut MSG, h_wnd: HWND, w_msg_filter_min: UINT, w_msg_filter_max: UINT) -> BOOL;
    fn TranslateMessage(lp_msg: *const MSG) -> BOOL;
    fn DispatchMessageW(lp_msg: *const MSG) -> LRESULT;
    fn RegisterDeviceNotificationW(h_recipient: HANDLE, notification_filter: *const c_void, flags: DWORD) -> HDEVNOTIFY;
    fn UnregisterDeviceNotification(handle: HDEVNOTIFY) -> BOOL;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: UINT, w_param: WPARAM, l_param: LPARAM) -> LRESULT {
    DefWindowProcW(hwnd, msg, w_param, l_param)
}

pub struct PnpWatcher {
    is_running: Arc<AtomicBool>,
}

impl PnpWatcher {
    pub fn new() -> Self {
        Self {
            is_running: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn start_monitoring<F>(&self, mut on_device_event: F)
    where
        F: FnMut() + Send + 'static,
    {
        let is_running = self.is_running.clone();

        thread::spawn(move || {
            let class_name = to_wide("FantomX_PnpWatcher");
            let wc = WNDCLASSEXW {
                cb_size: mem::size_of::<WNDCLASSEXW>() as UINT,
                style: 0,
                lpfn_wnd_proc: Some(wnd_proc),
                cb_cls_extra: 0,
                cb_wnd_extra: 0,
                h_instance: ptr::null_mut(),
                h_icon: ptr::null_mut(),
                h_cursor: ptr::null_mut(),
                h_br_background: ptr::null_mut(),
                lpsz_menu_name: ptr::null(),
                lpsz_class_name: class_name.as_ptr(),
                h_icon_sm: ptr::null_mut(),
            };

            unsafe { RegisterClassExW(&wc); }

            let hwnd = unsafe {
                CreateWindowExW(
                    0,
                    class_name.as_ptr(),
                    ptr::null(),
                    0,
                    0, 0, 0, 0,
                    HWND_MESSAGE,
                    ptr::null_mut(),
                    ptr::null_mut(),
                    ptr::null_mut(),
                )
            };

            let mut filter: DEV_BROADCAST_DEVICEINTERFACE_W = unsafe { mem::zeroed() };
            filter.dbcc_size = mem::size_of::<DEV_BROADCAST_DEVICEINTERFACE_W>() as DWORD;
            filter.dbcc_devicetype = DBT_DEVTYP_DEVICEINTERFACE;

            let h_notify = unsafe {
                RegisterDeviceNotificationW(
                    hwnd as HANDLE,
                    &filter as *const _ as *const c_void,
                    DEVICE_NOTIFY_ALL_INTERFACE_CLASSES,
                )
            };

            // Trigger initial discovery check
            on_device_event();

            let mut msg: MSG = unsafe { mem::zeroed() };
            while is_running.load(Ordering::SeqCst) {
                let res = unsafe { GetMessageW(&mut msg, hwnd, 0, 0) };
                if res <= 0 {
                    break;
                }
                if msg.message == WM_DEVICECHANGE {
                    if msg.w_param == DBT_DEVICEARRIVAL || msg.w_param == DBT_DEVICEREMOVECOMPLETE {
                        on_device_event();
                    }
                }
                unsafe {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                }
            }

            if !h_notify.is_null() {
                unsafe { UnregisterDeviceNotification(h_notify); }
            }
        });
    }
}
