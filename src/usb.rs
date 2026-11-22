//! USB transport backed by libusb, loaded dynamically at runtime.
//!
//! libusb is loaded via `libloading` from `libusb-1.0.dll` (no link-time or
//! compile-time C dependency), so the binary builds with just the Rust
//! toolchain and uses the libusb DLL already shipped alongside it. Only the
//! handful of functions we need are bound.
//!
//! The Qualcomm EDL (9008) interface is assumed to be interface 0 with the
//! common bulk endpoints OUT 0x02 / IN 0x81; both are overridable.

use anyhow::{bail, Result};
use libloading::{Library, Symbol};
use std::ffi::c_void;
use std::os::raw::{c_int, c_uchar, c_uint};
use std::ptr::null_mut;

use crate::transport::Transport;

pub const QUALCOMM_VID: u16 = 0x05C6;
pub const EDL_PID: u16 = 0x9008;

#[repr(C)]
#[derive(Default)]
struct DeviceDescriptor {
    b_length: u8,
    b_descriptor_type: u8,
    bcd_usb: u16,
    b_device_class: u8,
    b_device_sub_class: u8,
    b_device_protocol: u8,
    b_max_packet_size0: u8,
    id_vendor: u16,
    id_product: u16,
    bcd_device: u16,
    i_manufacturer: u8,
    i_product: u8,
    i_serial_number: u8,
    b_num_configurations: u8,
}

type FnInit = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type FnExit = unsafe extern "C" fn(*mut c_void);
type FnGetDevList = unsafe extern "C" fn(*mut c_void, *mut *mut *mut c_void) -> isize;
type FnFreeDevList = unsafe extern "C" fn(*mut *mut c_void, c_int);
type FnGetDesc = unsafe extern "C" fn(*mut c_void, *mut DeviceDescriptor) -> c_int;
type FnGetBus = unsafe extern "C" fn(*mut c_void) -> u8;
type FnGetAddr = unsafe extern "C" fn(*mut c_void) -> u8;
type FnOpen = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int;
type FnClose = unsafe extern "C" fn(*mut c_void);
type FnClaim = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type FnRelease = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type FnBulk = unsafe extern "C" fn(
    *mut c_void,
    c_uchar,
    *mut c_uchar,
    c_int,
    *mut c_int,
    c_uint,
) -> c_int;

#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub bus: u8,
    pub address: u8,
}

impl UsbDeviceInfo {
    pub fn is_edl(&self) -> bool {
        self.pid == EDL_PID
    }
}

/// Holds the loaded libusb library and an initialised context.
pub struct Libusb {
    _lib: Library,
    ctx: *mut c_void,
    init: FnInit,
    exit: FnExit,
    get_dev_list: FnGetDevList,
    free_dev_list: FnFreeDevList,
    get_desc: FnGetDesc,
    get_bus: FnGetBus,
    get_addr: FnGetAddr,
    open: FnOpen,
    close: FnClose,
    claim: FnClaim,
    release: FnRelease,
    bulk: FnBulk,
}

// Load a symbol, tolerating both the with-NUL and without-NUL conventions.
macro_rules! sym {
    ($lib:expr, $name:literal, $ty:ty) => {{
        let s: Symbol<$ty> = $lib
            .get(concat!($name, "\0").as_bytes())
            .or_else(|_| $lib.get($name.as_bytes()))?;
        *s
    }};
}

fn load_library() -> Result<Library> {
    // Try the plain DLL name (PATH / exe dir), then next to the executable.
    for name in ["libusb-1.0.dll", "libusb-1.0-0.dll"] {
        if let Ok(l) = unsafe { Library::new(name) } {
            return Ok(l);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            for name in ["libusb-1.0.dll", "libusb-1.0-0.dll"] {
                let p = dir.join(name);
                if p.exists() {
                    if let Ok(l) = unsafe { Library::new(&p) } {
                        return Ok(l);
                    }
                }
            }
        }
    }
    bail!("could not load libusb-1.0.dll (place it next to the exe or on PATH)")
}

impl Libusb {
    pub fn load() -> Result<Self> {
        let lib = load_library()?;
        unsafe {
            let init: FnInit = sym!(lib, "libusb_init", FnInit);
            let exit: FnExit = sym!(lib, "libusb_exit", FnExit);
            let get_dev_list: FnGetDevList = sym!(lib, "libusb_get_device_list", FnGetDevList);
            let free_dev_list: FnFreeDevList = sym!(lib, "libusb_free_device_list", FnFreeDevList);
            let get_desc: FnGetDesc =
                sym!(lib, "libusb_get_device_descriptor", FnGetDesc);
            let get_bus: FnGetBus = sym!(lib, "libusb_get_bus_number", FnGetBus);
            let get_addr: FnGetAddr = sym!(lib, "libusb_get_device_address", FnGetAddr);
            let open: FnOpen = sym!(lib, "libusb_open", FnOpen);
            let close: FnClose = sym!(lib, "libusb_close", FnClose);
            let claim: FnClaim = sym!(lib, "libusb_claim_interface", FnClaim);
            let release: FnRelease = sym!(lib, "libusb_release_interface", FnRelease);
            let bulk: FnBulk = sym!(lib, "libusb_bulk_transfer", FnBulk);

            let mut ctx: *mut c_void = null_mut();
            let rc = init(&mut ctx);
            if rc != 0 {
                bail!("libusb_init failed: {}", rc);
            }
            Ok(Libusb {
                _lib: lib,
                ctx,
                init,
                exit,
                get_dev_list,
                free_dev_list,
                get_desc,
                get_bus,
                get_addr,
                open,
                close,
                claim,
                release,
                bulk,
            })
        }
    }

    fn device_list(&self) -> Result<(*mut *mut c_void, isize)> {
        let mut list: *mut *mut c_void = null_mut();
        let count = unsafe { (self.get_dev_list)(self.ctx, &mut list) };
        if count < 0 {
            bail!("libusb_get_device_list failed: {}", count);
        }
        Ok((list, count))
    }

    /// Enumerate connected USB devices.
    pub fn enumerate(&self) -> Result<Vec<UsbDeviceInfo>> {
        let (list, count) = self.device_list()?;
        let mut out = Vec::new();
        for i in 0..count as usize {
            let dev = unsafe { *list.add(i) };
            let mut desc: DeviceDescriptor = unsafe { std::mem::zeroed() };
            unsafe {
                (self.get_desc)(dev, &mut desc);
                let bus = (self.get_bus)(dev);
                let addr = (self.get_addr)(dev);
                out.push(UsbDeviceInfo {
                    vid: desc.id_vendor,
                    pid: desc.id_product,
                    bus,
                    address: addr,
                });
            }
        }
        unsafe { (self.free_dev_list)(list, 1) };
        Ok(out)
    }

    /// Find connected Qualcomm EDL (9008) devices.
    pub fn detect_edl(&self) -> Result<Vec<UsbDeviceInfo>> {
        Ok(self
            .enumerate()?
            .into_iter()
            .filter(|d| d.is_edl())
            .collect())
    }

    /// Open the first device matching vid/pid and claim interface 0 for bulk
    /// I/O. Defaults to the common Qualcomm 9008 endpoints (OUT 0x02, IN 0x81).
    pub fn open_transport(
        &self,
        vid: u16,
        pid: u16,
        out_ep: u8,
        in_ep: u8,
    ) -> Result<LibusbTransport<'_>> {
        let (list, count) = self.device_list()?;
        let mut found: *mut c_void = null_mut();
        for i in 0..count as usize {
            let dev = unsafe { *list.add(i) };
            let mut desc: DeviceDescriptor = unsafe { std::mem::zeroed() };
            unsafe {
                (self.get_desc)(dev, &mut desc);
                if desc.id_vendor == vid && desc.id_product == pid {
                    found = dev;
                    break;
                }
            }
        }
        if found.is_null() {
            unsafe { (self.free_dev_list)(list, 1) };
            bail!("no USB device {:04x}:{:04x} found", vid, pid);
        }
        let mut handle: *mut c_void = null_mut();
        let rc = unsafe { (self.open)(found, &mut handle) };
        // The open handle holds its own device reference; safe to free the list.
        unsafe { (self.free_dev_list)(list, 1) };
        if rc != 0 || handle.is_null() {
            bail!("libusb_open failed: {}", rc);
        }
        let rc = unsafe { (self.claim)(handle, 0) };
        if rc != 0 {
            unsafe {
                (self.close)(handle);
            }
            bail!("libusb_claim_interface(0) failed: {}", rc);
        }
        Ok(LibusbTransport {
            libusb: self,
            handle,
            out_ep,
            in_ep,
            timeout_ms: 5000,
            interface: 0,
        })
    }
}

impl Drop for Libusb {
    fn drop(&mut self) {
        unsafe {
            (self.exit)(self.ctx);
        }
    }
}

/// A `Transport` over a libusb bulk endpoint pair.
pub struct LibusbTransport<'a> {
    libusb: &'a Libusb,
    handle: *mut c_void,
    out_ep: u8,
    in_ep: u8,
    timeout_ms: u32,
    interface: c_int,
}

impl Transport for LibusbTransport<'_> {
    fn write_all(&mut self, data: &[u8]) -> Result<usize> {
        let mut transferred: c_int = 0;
        let rc = unsafe {
            (self.libusb.bulk)(
                self.handle,
                self.out_ep,
                data.as_ptr() as *mut c_uchar,
                data.len() as c_int,
                &mut transferred,
                self.timeout_ms,
            )
        };
        if rc != 0 {
            bail!("bulk OUT transfer failed: libusb error {}", rc);
        }
        Ok(transferred as usize)
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let mut transferred: c_int = 0;
        let rc = unsafe {
            (self.libusb.bulk)(
                self.handle,
                self.in_ep,
                buf.as_mut_ptr() as *mut c_uchar,
                buf.len() as c_int,
                &mut transferred,
                self.timeout_ms,
            )
        };
        if rc != 0 {
            bail!("bulk IN transfer failed: libusb error {}", rc);
        }
        Ok(transferred as usize)
    }

    fn set_read_timeout_ms(&mut self, ms: u32) {
        self.timeout_ms = ms;
    }
}

impl Drop for LibusbTransport<'_> {
    fn drop(&mut self) {
        unsafe {
            (self.libusb.release)(self.handle, self.interface);
            (self.libusb.close)(self.handle);
        }
    }
}