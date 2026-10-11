//! Narrow libusb-win32 adapter for the installed ANT USB-m driver (0fcf:1009).
//! ABI declarations follow lusb0_usb.h; the system DLL is loaded, not bundled.
use std::{
    ffi::{c_char, c_int, c_void},
    ptr,
};
use windows::{
    core::HSTRING,
    Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE},
        System::Threading::CreateMutexW,
    },
};
struct Owner(HANDLE);
impl Owner {
    fn claim(name: &str) -> Result<Self, String> {
        let handle = unsafe { CreateMutexW(None, false, &HSTRING::from(name)) }
            .map_err(|e| e.to_string())?;
        let exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        let owner = Self(handle);
        if exists {
            return Err("Another OpenRide process owns this ANT adapter".into());
        }
        Ok(owner)
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}
type Handle = *mut c_void;
#[link(name = "kernel32")]
extern "system" {
    fn GetSystemDirectoryW(buffer: *mut u16, size: u32) -> u32;
    fn LoadLibraryExW(name: *const u16, file: Handle, flags: u32) -> Handle;
    fn GetProcAddress(module: Handle, name: *const u8) -> *mut c_void;
    fn FreeLibrary(module: Handle) -> i32;
}
// libusb-win32 packs all of these structures to one byte, including pointers.
#[repr(C, packed)]
struct Bus {
    next: *mut Bus,
    prev: *mut Bus,
    name: [u8; 512],
    devices: *mut Device,
}
#[repr(C, packed)]
struct Device {
    next: *mut Device,
    prev: *mut Device,
    name: [u8; 512],
    bus: *mut Bus,
    descriptor: [u8; 18],
    config: *mut Config,
}
type Unary = unsafe extern "C" fn(Handle) -> c_int;
type Interface = unsafe extern "C" fn(Handle, c_int) -> c_int;
type Bulk = unsafe extern "C" fn(Handle, c_int, *mut c_char, c_int, c_int) -> c_int;
#[repr(C, packed)]
struct Config {
    header: [u8; 9],
    interfaces: *mut InterfaceDescriptor,
    extra: *mut u8,
    extra_len: i32,
}
#[repr(C, packed)]
struct InterfaceDescriptor {
    alternatives: *mut Alternative,
    count: i32,
}
#[repr(C, packed)]
struct Alternative {
    header: [u8; 9],
    endpoints: *mut Endpoint,
    extra: *mut u8,
    extra_len: i32,
}
#[repr(C, packed)]
struct Endpoint {
    header: [u8; 9],
    extra: *mut u8,
    extra_len: i32,
}
struct Library(Handle);
impl Drop for Library {
    fn drop(&mut self) {
        unsafe {
            FreeLibrary(self.0);
        }
    }
}
pub struct Usb {
    _library: Library,
    _owner: Owner,
    handle: Handle,
    close: Unary,
    release: Interface,
    read: Bulk,
    write: Bulk,
    input: u8,
    output: u8,
    claimed: bool,
}
impl Usb {
    pub fn open() -> Result<Self, String> {
        let owner = Owner::claim("Local\\OpenRide-ANTUSBm")?;
        // SAFETY: functions come from the system libusb0 DLL, with signatures and
        // packed structure prefixes matching its public C ABI. No pointers escape.
        unsafe {
            let mut path = [0u16; 32768];
            let len = GetSystemDirectoryW(path.as_mut_ptr(), path.len() as u32) as usize;
            if len == 0 || len + 13 >= path.len() {
                return Err("Cannot locate Windows system directory".into());
            }
            let mut path = path[..len].to_vec();
            path.extend("\\libusb0.dll".encode_utf16());
            path.push(0);
            let library = Library(LoadLibraryExW(path.as_ptr(), ptr::null_mut(), 0x800));
            if library.0.is_null() {
                return Err("Cannot load system libusb0.dll; install the matching ANT USB-m libusb-win32 driver".into());
            }
            macro_rules! symbol {
                ($name:literal,$ty:ty) => {{
                    let p = GetProcAddress(library.0, concat!($name, "\0").as_ptr());
                    if p.is_null() {
                        return Err(concat!("Missing libusb function ", $name).into());
                    }
                    std::mem::transmute::<*mut c_void, $ty>(p)
                }};
            }
            let init = symbol!("usb_init", unsafe extern "C" fn());
            let buses = symbol!("usb_find_busses", unsafe extern "C" fn() -> c_int);
            let devices = symbol!("usb_find_devices", unsafe extern "C" fn() -> c_int);
            let list = symbol!("usb_get_busses", unsafe extern "C" fn() -> *mut Bus);
            let open = symbol!("usb_open", unsafe extern "C" fn(*mut Device) -> Handle);
            let close = symbol!("usb_close", Unary);
            let release = symbol!("usb_release_interface", Interface);
            let claim = symbol!("usb_claim_interface", Interface);
            let read = symbol!("usb_bulk_read", Bulk);
            let write = symbol!("usb_bulk_write", Bulk);
            let configure = symbol!("usb_set_configuration", Interface);
            init();
            if buses() < 0 || devices() < 0 {
                return Err("libusb device enumeration failed".into());
            }
            let mut matches = vec![];
            let mut bus = list();
            let mut budget = 1024;
            while !bus.is_null() {
                if budget == 0 {
                    return Err("USB enumeration limit".into());
                }
                budget -= 1;
                let mut device = (*bus).devices;
                while !device.is_null() {
                    if budget == 0 {
                        return Err("USB enumeration limit".into());
                    }
                    budget -= 1;
                    let d = (*device).descriptor;
                    if u16::from_le_bytes([d[8], d[9]]) == 0x0fcf
                        && u16::from_le_bytes([d[10], d[11]]) == 0x1009
                    {
                        matches.push(device);
                    }
                    device = (*device).next;
                }
                bus = (*bus).next;
            }
            if matches.len() != 1 {
                return Err(format!(
                    "Expected one ANT USB-m 0fcf:1009; found {}",
                    matches.len()
                ));
            }
            let handle = open(matches[0]);
            if handle.is_null() {
                return Err(
                    "Cannot open ANT dongle; close other ANT applications and check driver access"
                        .into(),
                );
            }
            let mut usb = Self {
                _library: library,
                _owner: owner,
                handle,
                close,
                release,
                read,
                write,
                input: 0,
                output: 0,
                claimed: false,
            };
            // Use the descriptors already fetched during libusb enumeration.
            let config = (*matches[0]).config;
            if config.is_null()
                || (*config).header[5] != 1
                || (*config).header[4] == 0
                || (*config).interfaces.is_null()
            {
                return Err("Missing ANT USB configuration/interface descriptors".into());
            }
            let interface = (*config).interfaces;
            if (*interface).count < 1 || (*interface).alternatives.is_null() {
                return Err("Missing ANT USB alternate setting".into());
            }
            let alternate = (*interface).alternatives;
            let header = (*alternate).header;
            if header[2] != 0
                || header[3] != 0
                || !(2..=32).contains(&header[4])
                || (*alternate).endpoints.is_null()
            {
                return Err("Unexpected ANT USB interface/endpoint layout".into());
            }
            let mut descriptor = header.to_vec();
            for i in 0..header[4] as usize {
                let h = (*(*alternate).endpoints.add(i)).header;
                descriptor.extend_from_slice(&h[..7]);
            }
            let (input, output) = endpoints(&descriptor)?;
            usb.input = input;
            usb.output = output;
            let configured = configure(handle, 1);
            if configured < 0 {
                return Err(format!("ANT USB configuration failed: {configured}"));
            }
            let result = claim(handle, 0);
            if result < 0 {
                return Err(format!(
                    "Cannot claim ANT interface 0: {result}; close other ANT apps"
                ));
            }
            usb.claimed = true;
            Ok(usb)
        }
    }
    pub fn send(&mut self, bytes: &[u8]) -> Result<(), String> {
        let mut bytes = bytes.to_vec();
        let result = unsafe {
            (self.write)(
                self.handle,
                i32::from(self.output),
                bytes.as_mut_ptr().cast(),
                bytes.len() as i32,
                1000,
            )
        };
        if result != bytes.len() as i32 {
            return Err(format!("ANT USB write failed/partial: {result}"));
        }
        Ok(())
    }
    pub fn receive(&mut self) -> Result<Vec<u8>, String> {
        let mut b = vec![0; 256];
        let n = unsafe {
            (self.read)(
                self.handle,
                i32::from(self.input),
                b.as_mut_ptr().cast(),
                256,
                200,
            )
        };
        if n == -116 {
            return Ok(vec![]);
        } // libusb-win32 ETIMEDOUT; disconnects are errors.
        if !(0..=256).contains(&n) {
            return Err(format!("ANT USB read failed: {n}"));
        }
        b.truncate(n as usize);
        Ok(b)
    }
}
impl Drop for Usb {
    fn drop(&mut self) {
        unsafe {
            if self.claimed {
                (self.release)(self.handle, 0);
            }
            (self.close)(self.handle);
        }
    }
}
fn endpoints(bytes: &[u8]) -> Result<(u8, u8), String> {
    let mut input = None;
    let mut output = None;
    let mut selected = false;
    let mut offset = 0;
    while offset + 2 <= bytes.len() {
        let len = bytes[offset] as usize;
        if len < 2 || offset + len > bytes.len() {
            return Err("Malformed USB descriptor".into());
        }
        let d = &bytes[offset..offset + len];
        if d[1] == 4 && len >= 9 {
            selected = d[2] == 0 && d[3] == 0;
        }
        if selected && d[1] == 5 && len >= 7 && d[3] & 3 == 2 {
            if d[2] & 0x80 != 0 {
                input = Some(d[2]);
            } else {
                output = Some(d[2]);
            }
        }
        offset += len;
    }
    input
        .zip(output)
        .ok_or("ANT interface 0 bulk endpoints missing".into())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exclusive_owner_releases_on_drop() {
        let name = format!("Local\\OpenRide-ANTUSBm-test-{}", std::process::id());
        let owner = Owner::claim(&name).unwrap();
        assert!(Owner::claim(&name).is_err());
        drop(owner);
        assert!(Owner::claim(&name).is_ok());
    }
    #[test]
    fn descriptor_endpoint_selection() {
        let bytes = [
            9, 2, 32, 0, 1, 1, 0, 128, 50, 9, 4, 0, 0, 2, 255, 0, 0, 0, 7, 5, 0x81, 2, 64, 0, 0, 7,
            5, 1, 2, 64, 0, 0,
        ];
        assert_eq!(endpoints(&bytes).unwrap(), (0x81, 1));
        assert!(endpoints(&bytes[..25]).is_err());
        assert_eq!(
            std::mem::offset_of!(Device, descriptor),
            2 * std::mem::size_of::<usize>() + 512 + std::mem::size_of::<usize>()
        );
    }
}
