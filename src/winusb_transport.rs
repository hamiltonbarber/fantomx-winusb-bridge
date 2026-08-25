use std::ffi::c_void;
use std::mem;
use std::ptr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use crate::usb_midi_codec::{MidiMessage, UsbMidiEncoder};

type BOOL = i32;
type DWORD = u32;
type HANDLE = *mut c_void;
type HDEVINFO = *mut c_void;
type HKEY = *mut c_void;
type LSTATUS = i32;
type UCHAR = u8;
type ULONG = u32;

const INVALID_HANDLE_VALUE: HANDLE = -1isize as HANDLE;
const HKEY_LOCAL_MACHINE: HKEY = 0x80000002usize as HKEY;
const KEY_READ: DWORD = 0x20019;
const ERROR_SUCCESS: LSTATUS = 0;
const TRUE: BOOL = 1;
const FALSE: BOOL = 0;

const GENERIC_READ: DWORD = 0x80000000;
const GENERIC_WRITE: DWORD = 0x40000000;
const FILE_SHARE_READ: DWORD = 0x00000001;
const FILE_SHARE_WRITE: DWORD = 0x00000002;
const OPEN_EXISTING: DWORD = 3;
const FILE_ATTRIBUTE_NORMAL: DWORD = 0x00000080;
const FILE_FLAG_OVERLAPPED: DWORD = 0x40000000;

const DIGCF_PRESENT: DWORD = 0x00000002;
const DIGCF_DEVICEINTERFACE: DWORD = 0x00000010;

const ALLOW_PARTIAL_READS: DWORD = 0x05;
const AUTO_SUSPEND: DWORD = 0x81;

#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, Eq)]
pub struct GUID {
    pub data1: u32,
    pub data2: u16,
    pub data3: u16,
    pub data4: [u8; 8],
}

#[repr(C)]
struct SP_DEVICE_INTERFACE_DATA {
    cb_size: DWORD,
    interface_class_guid: GUID,
    flags: DWORD,
    reserved: usize,
}

#[repr(C)]
struct OVERLAPPED {
    internal: usize,
    internal_high: usize,
    offset: DWORD,
    offset_high: DWORD,
    h_event: HANDLE,
}

#[link(name = "advapi32")]
extern "system" {
    fn RegOpenKeyExW(hkey: HKEY, lp_sub_key: *const u16, ul_options: DWORD, sam_desired: DWORD, phk_result: *mut HKEY) -> LSTATUS;
    fn RegEnumKeyExW(hkey: HKEY, dw_index: DWORD, lp_name: *mut u16, lpcch_name: *mut DWORD, lp_reserved: *mut DWORD, lp_class: *mut u16, lpcch_class: *mut DWORD, lpft_last_write_time: *mut c_void) -> LSTATUS;
    fn RegQueryValueExW(hkey: HKEY, lp_value_name: *const u16, lp_reserved: *mut DWORD, lp_type: *mut DWORD, lp_data: *mut u8, lpcb_data: *mut DWORD) -> LSTATUS;
    fn RegCloseKey(hkey: HKEY) -> LSTATUS;
}

#[link(name = "setupapi")]
extern "system" {
    fn SetupDiGetClassDevsW(class_guid: *const GUID, enumerator: *const u16, hwnd_parent: HANDLE, flags: DWORD) -> HDEVINFO;
    fn SetupDiEnumDeviceInterfaces(device_info_set: HDEVINFO, device_info_data: *mut c_void, interface_class_guid: *const GUID, member_index: DWORD, device_interface_data: *mut SP_DEVICE_INTERFACE_DATA) -> BOOL;
    fn SetupDiGetDeviceInterfaceDetailW(device_info_set: HDEVINFO, device_info_data: *mut SP_DEVICE_INTERFACE_DATA, device_interface_detail_data: *mut u8, device_interface_detail_data_size: DWORD, required_size: *mut DWORD, device_info_data: *mut c_void) -> BOOL;
    fn SetupDiDestroyDeviceInfoList(device_info_set: HDEVINFO) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateFileW(lp_file_name: *const u16, dw_desired_access: DWORD, dw_share_mode: DWORD, lp_security_attributes: *mut c_void, dw_creation_disposition: DWORD, dw_flags_and_attributes: DWORD, h_template_file: HANDLE) -> HANDLE;
    fn CreateEventW(lp_event_attributes: *mut c_void, b_manual_reset: BOOL, b_initial_state: BOOL, lp_name: *const u16) -> HANDLE;
    fn CancelIoEx(h_file: HANDLE, lp_overlapped: *const OVERLAPPED) -> BOOL;
    fn CloseHandle(h_object: HANDLE) -> BOOL;
    fn GetLastError() -> DWORD;
}

#[link(name = "winusb")]
extern "system" {
    fn WinUsb_Initialize(device_handle: HANDLE, interface_handle: *mut HANDLE) -> BOOL;
    fn WinUsb_Free(interface_handle: HANDLE) -> BOOL;
    fn WinUsb_ReadPipe(interface_handle: HANDLE, pipe_id: UCHAR, buffer: *mut u8, buffer_length: ULONG, length_transferred: *mut ULONG, overlapped: *mut OVERLAPPED) -> BOOL;
    fn WinUsb_WritePipe(interface_handle: HANDLE, pipe_id: UCHAR, buffer: *const u8, buffer_length: ULONG, length_transferred: *mut ULONG, overlapped: *mut OVERLAPPED) -> BOOL;
    fn WinUsb_GetOverlappedResult(interface_handle: HANDLE, overlapped: *mut OVERLAPPED, number_of_bytes_transferred: *mut ULONG, b_wait: BOOL) -> BOOL;
    fn WinUsb_SetPowerPolicy(interface_handle: HANDLE, policy_type: ULONG, value_length: ULONG, value: *const c_void) -> BOOL;
    fn WinUsb_SetPipePolicy(interface_handle: HANDLE, pipe_id: UCHAR, policy_type: ULONG, value_length: ULONG, value: *const c_void) -> BOOL;
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn parse_guid_str(s: &str) -> Option<GUID> {
    let clean = s.trim().trim_matches('{').trim_matches('}');
    let parts: Vec<&str> = clean.split('-').collect();
    if parts.len() != 5 {
        return None;
    }
    let data1 = u32::from_str_radix(parts[0], 16).ok()?;
    let data2 = u16::from_str_radix(parts[1], 16).ok()?;
    let data3 = u16::from_str_radix(parts[2], 16).ok()?;
    let p3 = u16::from_str_radix(parts[3], 16).ok()?;
    let p4 = u64::from_str_radix(parts[4], 16).ok()?;

    let mut data4 = [0u8; 8];
    data4[0] = ((p3 >> 8) & 0xFF) as u8;
    data4[1] = (p3 & 0xFF) as u8;
    for i in 0..6 {
        data4[2 + i] = ((p4 >> (8 * (5 - i))) & 0xFF) as u8;
    }

    Some(GUID { data1, data2, data3, data4 })
}

pub fn discover_fantomx_guids() -> Vec<GUID> {
    let mut guids = Vec::new();
    let root_path = to_wide("SYSTEM\\CurrentControlSet\\Enum\\USB\\VID_0582&PID_006D");
    let mut root_key: HKEY = ptr::null_mut();

    if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, root_path.as_ptr(), 0, KEY_READ, &mut root_key) } != ERROR_SUCCESS {
        return guids;
    }

    let mut index = 0;
    loop {
        let mut name_buf = [0u16; 256];
        let mut name_len = name_buf.len() as DWORD;
        let res = unsafe { RegEnumKeyExW(root_key, index, name_buf.as_mut_ptr(), &mut name_len, ptr::null_mut(), ptr::null_mut(), ptr::null_mut(), ptr::null_mut()) };
        if res != ERROR_SUCCESS {
            break;
        }
        let sub_name = String::from_utf16_lossy(&name_buf[..name_len as usize]);
        let param_path = to_wide(&format!("SYSTEM\\CurrentControlSet\\Enum\\USB\\VID_0582&PID_006D\\{}\\Device Parameters", sub_name));
        let mut param_key: HKEY = ptr::null_mut();

        if unsafe { RegOpenKeyExW(HKEY_LOCAL_MACHINE, param_path.as_ptr(), 0, KEY_READ, &mut param_key) } == ERROR_SUCCESS {
            let val_names = [to_wide("DeviceInterfaceGUIDs"), to_wide("DeviceInterfaceGUID")];
            for val_name in &val_names {
                let mut data_type: DWORD = 0;
                let mut data_size: DWORD = 0;
                if unsafe { RegQueryValueExW(param_key, val_name.as_ptr(), ptr::null_mut(), &mut data_type, ptr::null_mut(), &mut data_size) } == ERROR_SUCCESS && data_size > 0 {
                    let mut data = vec![0u8; data_size as usize];
                    if unsafe { RegQueryValueExW(param_key, val_name.as_ptr(), ptr::null_mut(), &mut data_type, data.as_mut_ptr(), &mut data_size) } == ERROR_SUCCESS {
                        let u16_slice = unsafe { std::slice::from_raw_parts(data.as_ptr() as *const u16, data_size as usize / 2) };
                        let full_str = String::from_utf16_lossy(u16_slice);
                        for g_str in full_str.split('\0') {
                            if let Some(parsed) = parse_guid_str(g_str) {
                                if !guids.contains(&parsed) {
                                    guids.push(parsed);
                                }
                            }
                        }
                    }
                }
            }
            unsafe { RegCloseKey(param_key) };
        }
        index += 1;
    }
    unsafe { RegCloseKey(root_key) };
    guids
}

pub struct WinUsbSession {
    file_handle: HANDLE,
    winusb_handle: HANDLE,
    is_active: Arc<AtomicBool>,
    reader_thread: Option<JoinHandle<()>>,
}

unsafe impl Send for WinUsbSession {}
unsafe impl Sync for WinUsbSession {}

impl WinUsbSession {
    pub fn open(path: &[u16]) -> Option<Self> {
        let file_handle = unsafe {
            CreateFileW(path.as_ptr(), GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, ptr::null_mut(), OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL | FILE_FLAG_OVERLAPPED, ptr::null_mut())
        };
        if file_handle == INVALID_HANDLE_VALUE {
            return None;
        }

        let mut winusb_handle: HANDLE = ptr::null_mut();
        if unsafe { WinUsb_Initialize(file_handle, &mut winusb_handle) } != TRUE {
            unsafe { CloseHandle(file_handle) };
            return None;
        }

        let disable_suspend: UCHAR = 0;
        unsafe { WinUsb_SetPowerPolicy(winusb_handle, AUTO_SUSPEND, 1, &disable_suspend as *const _ as *const c_void) };

        let allow_partial: UCHAR = 1;
        unsafe { WinUsb_SetPipePolicy(winusb_handle, 0x82, ALLOW_PARTIAL_READS, 1, &allow_partial as *const _ as *const c_void) };

        Some(Self {
            file_handle,
            winusb_handle,
            is_active: Arc::new(AtomicBool::new(true)),
            reader_thread: None,
        })
    }

    pub fn send_raw_usb_midi(&self, packets: &[u8]) -> bool {
        if !self.is_active.load(Ordering::SeqCst) || packets.is_empty() {
            return false;
        }
        let mut transferred: ULONG = 0;
        let mut ov: OVERLAPPED = unsafe { mem::zeroed() };
        let event = unsafe { CreateEventW(ptr::null_mut(), TRUE, FALSE, ptr::null()) };
        ov.h_event = event;

        let ok = unsafe {
            WinUsb_WritePipe(self.winusb_handle, 0x01, packets.as_ptr(), packets.len() as ULONG, &mut transferred, &mut ov)
        };

        let result = if ok == TRUE {
            true
        } else {
            let err = unsafe { GetLastError() };
            if err == 997 { // ERROR_IO_PENDING
                let mut bytes: ULONG = 0;
                unsafe { WinUsb_GetOverlappedResult(self.winusb_handle, &mut ov, &mut bytes, TRUE) == TRUE }
            } else {
                false
            }
        };

        unsafe { CloseHandle(event) };
        result
    }

    pub fn send_midi_message(&self, msg: &MidiMessage) -> bool {
        let raw_packets = UsbMidiEncoder::encode_message(msg, 0);
        let flat_buf: Vec<u8> = raw_packets.into_iter().flatten().collect();
        self.send_raw_usb_midi(&flat_buf)
    }

    pub fn start_reader_loop<F>(&mut self, mut on_midi_received: F)
    where
        F: FnMut(&[u8]) + Send + 'static,
    {
        let h_winusb = self.winusb_handle as usize;
        let is_active = self.is_active.clone();

        let handle = thread::spawn(move || {
            let winusb = h_winusb as HANDLE;
            let mut rx_buf = [0u8; 64];

            while is_active.load(Ordering::SeqCst) {
                let mut ov: OVERLAPPED = unsafe { mem::zeroed() };
                let event = unsafe { CreateEventW(ptr::null_mut(), TRUE, FALSE, ptr::null()) };
                ov.h_event = event;

                let mut transferred: ULONG = 0;
                let ok = unsafe { WinUsb_ReadPipe(winusb, 0x82, rx_buf.as_mut_ptr(), 64, &mut transferred, &mut ov) };
                if ok == FALSE {
                    let err = unsafe { GetLastError() };
                    if err == 997 { // ERROR_IO_PENDING
                        let mut bytes: ULONG = 0;
                        if unsafe { WinUsb_GetOverlappedResult(winusb, &mut ov, &mut bytes, TRUE) } == TRUE {
                            on_midi_received(&rx_buf[..bytes as usize]);
                        } else {
                            break;
                        }
                    } else {
                        break;
                    }
                } else {
                    on_midi_received(&rx_buf[..transferred as usize]);
                }
                unsafe { CloseHandle(event) };
            }
            is_active.store(false, Ordering::SeqCst);
        });

        self.reader_thread = Some(handle);
    }

    pub fn is_alive(&self) -> bool {
        self.is_active.load(Ordering::SeqCst)
    }
}

impl Drop for WinUsbSession {
    fn drop(&mut self) {
        self.is_active.store(false, Ordering::SeqCst);

        unsafe {
            if self.file_handle != INVALID_HANDLE_VALUE && !self.file_handle.is_null() {
                CancelIoEx(self.file_handle, ptr::null());
            }
        }

        if let Some(handle) = self.reader_thread.take() {
            let _ = handle.join();
        }

        unsafe {
            if !self.winusb_handle.is_null() {
                WinUsb_Free(self.winusb_handle);
                self.winusb_handle = ptr::null_mut();
            }
            if self.file_handle != INVALID_HANDLE_VALUE && !self.file_handle.is_null() {
                CloseHandle(self.file_handle);
                self.file_handle = INVALID_HANDLE_VALUE;
            }
        }
    }
}

pub struct WinUsbTransport;

impl WinUsbTransport {
    pub fn find_device_with_guids(guids: &[GUID]) -> Option<Vec<u16>> {
        for guid in guids {
            let dev_info = unsafe { SetupDiGetClassDevsW(guid, ptr::null(), ptr::null_mut(), DIGCF_PRESENT | DIGCF_DEVICEINTERFACE) };
            if dev_info == INVALID_HANDLE_VALUE {
                continue;
            }

            let mut iface_data: SP_DEVICE_INTERFACE_DATA = unsafe { mem::zeroed() };
            iface_data.cb_size = mem::size_of::<SP_DEVICE_INTERFACE_DATA>() as DWORD;

            if unsafe { SetupDiEnumDeviceInterfaces(dev_info, ptr::null_mut(), guid, 0, &mut iface_data) } == TRUE {
                let mut req_size: DWORD = 0;
                unsafe { SetupDiGetDeviceInterfaceDetailW(dev_info, &mut iface_data, ptr::null_mut(), 0, &mut req_size, ptr::null_mut()); }
                if req_size > 0 {
                    let mut buf = vec![0u8; req_size as usize];
                    let detail_ptr = buf.as_mut_ptr();
                    let cb_size_field = detail_ptr as *mut DWORD;
                    unsafe { *cb_size_field = if mem::size_of::<usize>() == 8 { 8 } else { 6 } };
                    if unsafe { SetupDiGetDeviceInterfaceDetailW(dev_info, &mut iface_data, detail_ptr, req_size, ptr::null_mut(), ptr::null_mut()) } == TRUE {
                        let path_u16 = unsafe {
                            let path_start = detail_ptr.add(4) as *const u16;
                            let mut len = 0;
                            while *path_start.add(len) != 0 { len += 1; }
                            std::slice::from_raw_parts(path_start, len + 1)
                        };
                        unsafe { SetupDiDestroyDeviceInfoList(dev_info) };
                        return Some(path_u16.to_vec());
                    }
                }
            }
            unsafe { SetupDiDestroyDeviceInfoList(dev_info) };
        }
        None
    }
}
