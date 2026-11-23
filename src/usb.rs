//! USB transport backed by libusb, loaded dynamically at runtime.
//!
//! libusb is loaded via `libloading` from `libusb-1.0.dll` (no link-time or
//! compile-time C dependency), so the binary builds with just the Rust
//! toolchain and uses the libusb DLL already shipped alongside it. Only the
//! handful of functions we need are bound.
//!
//! Qualcomm EDL USB interfaces are selected from the active configuration's
//! descriptors, rather than assuming interface 0 or fixed endpoint addresses.
//! On Windows the device still needs a libusb-compatible driver such as WinUSB.

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

#[repr(C)]
struct EndpointDescriptor {
    b_length: u8,
    b_descriptor_type: u8,
    b_endpoint_address: u8,
    bm_attributes: u8,
    w_max_packet_size: u16,
    b_interval: u8,
    b_refresh: u8,
    b_synch_address: u8,
    extra: *const u8,
    extra_length: c_int,
}

#[repr(C)]
struct InterfaceDescriptor {
    b_length: u8,
    b_descriptor_type: u8,
    b_interface_number: u8,
    b_alternate_setting: u8,
    b_num_endpoints: u8,
    b_interface_class: u8,
    b_interface_sub_class: u8,
    b_interface_protocol: u8,
    i_interface: u8,
    endpoint: *const EndpointDescriptor,
    extra: *const u8,
    extra_length: c_int,
}

#[repr(C)]
struct UsbInterface {
    altsetting: *const InterfaceDescriptor,
    num_altsetting: c_int,
}

#[repr(C)]
struct ConfigDescriptor {
    b_length: u8,
    b_descriptor_type: u8,
    w_total_length: u16,
    b_num_interfaces: u8,
    b_configuration_value: u8,
    i_configuration: u8,
    bm_attributes: u8,
    max_power: u8,
    interface: *const UsbInterface,
    extra: *const u8,
    extra_length: c_int,
}

type FnInit = unsafe extern "C" fn(*mut *mut c_void) -> c_int;
type FnExit = unsafe extern "C" fn(*mut c_void);
type FnGetDevList = unsafe extern "C" fn(*mut c_void, *mut *mut *mut c_void) -> isize;
type FnFreeDevList = unsafe extern "C" fn(*mut *mut c_void, c_int);
type FnGetDesc = unsafe extern "C" fn(*mut c_void, *mut DeviceDescriptor) -> c_int;
type FnGetActiveConfigDesc =
    unsafe extern "C" fn(*mut c_void, *mut *mut ConfigDescriptor) -> c_int;
type FnFreeConfigDesc = unsafe extern "C" fn(*mut ConfigDescriptor);
type FnGetBus = unsafe extern "C" fn(*mut c_void) -> u8;
type FnGetAddr = unsafe extern "C" fn(*mut c_void) -> u8;
type FnOpen = unsafe extern "C" fn(*mut c_void, *mut *mut c_void) -> c_int;
type FnClose = unsafe extern "C" fn(*mut c_void);
type FnClaim = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type FnRelease = unsafe extern "C" fn(*mut c_void, c_int) -> c_int;
type FnSetAltSetting = unsafe extern "C" fn(*mut c_void, c_int, c_int) -> c_int;
type FnBulk = unsafe extern "C" fn(
    *mut c_void,
    c_uchar,
    *mut c_uchar,
    c_int,
    *mut c_int,
    c_uint,
) -> c_int;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BulkEndpoints {
    interface_number: u8,
    alternate_setting: u8,
    out_endpoint: u8,
    in_endpoint: u8,
}

#[derive(Debug, Clone)]
struct EndpointInfo {
    address: u8,
    attributes: u8,
}

#[derive(Debug, Clone)]
struct InterfaceAltSetting {
    interface_number: u8,
    alternate_setting: u8,
    endpoints: Vec<EndpointInfo>,
}

fn choose_bulk_endpoints(interfaces: &[InterfaceAltSetting]) -> Option<BulkEndpoints> {
    for interface in interfaces {
        let mut out_endpoint = None;
        let mut in_endpoint = None;
        for endpoint in &interface.endpoints {
            // libusb transfer type 2 is bulk; bit 7 marks an IN endpoint.
            if endpoint.attributes & 0x03 != 0x02 {
                continue;
            }
            if endpoint.address & 0x80 == 0 {
                out_endpoint.get_or_insert(endpoint.address);
            } else {
                in_endpoint.get_or_insert(endpoint.address);
            }
        }
        if let (Some(out_endpoint), Some(in_endpoint)) = (out_endpoint, in_endpoint) {
            return Some(BulkEndpoints {
                interface_number: interface.interface_number,
                alternate_setting: interface.alternate_setting,
                out_endpoint,
                in_endpoint,
            });
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct UsbDeviceInfo {
    pub vid: u16,
    pub pid: u16,
    pub bus: u8,
    pub address: u8,
}

impl UsbDeviceInfo {
    pub fn is_edl(&self) -> bool {
        self.vid == QUALCOMM_VID && self.pid == EDL_PID
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovers_bulk_pair_on_nonzero_interface_and_alternate_setting() {
        let interfaces = vec![
            InterfaceAltSetting {
                interface_number: 0,
                alternate_setting: 0,
                endpoints: vec![EndpointInfo {
                    address: 0x02,
                    attributes: 0x02,
                }],
            },
            InterfaceAltSetting {
                interface_number: 3,
                alternate_setting: 1,
                endpoints: vec![
                    EndpointInfo {
                        address: 0x82,
                        attributes: 0x03,
                    },
                    EndpointInfo {
                        address: 0x03,
                        attributes: 0x02,
                    },
                    EndpointInfo {
                        address: 0x84,
                        attributes: 0x02,
                    },
                ],
            },
        ];

        assert_eq!(
            choose_bulk_endpoints(&interfaces),
            Some(BulkEndpoints {
                interface_number: 3,
                alternate_setting: 1,
                out_endpoint: 0x03,
                in_endpoint: 0x84,
            })
        );
    }

    #[test]
    fn does_not_pair_bulk_endpoints_from_different_interfaces() {
        let interfaces = vec![
            InterfaceAltSetting {
                interface_number: 0,
                alternate_setting: 0,
                endpoints: vec![EndpointInfo {
                    address: 0x02,
                    attributes: 0x02,
                }],
            },
            InterfaceAltSetting {
                interface_number: 1,
                alternate_setting: 0,
                endpoints: vec![EndpointInfo {
                    address: 0x81,
                    attributes: 0x02,
                }],
            },
        ];

        assert_eq!(choose_bulk_endpoints(&interfaces), None);
    }

    #[test]
    fn identifies_only_qualcomm_edl_devices() {
        let qualcomm_edl = UsbDeviceInfo {
            vid: QUALCOMM_VID,
            pid: EDL_PID,
            bus: 1,
            address: 1,
        };
        let other_vendor_same_product = UsbDeviceInfo {
            vid: 0x1234,
            pid: EDL_PID,
            bus: 1,
            address: 2,
        };
        let qualcomm_non_edl = UsbDeviceInfo {
            vid: QUALCOMM_VID,
            pid: 0x9007,
            bus: 1,
            address: 3,
        };

        assert!(qualcomm_edl.is_edl());
        assert!(!other_vendor_same_product.is_edl());
        assert!(!qualcomm_non_edl.is_edl());
    }
}

/// Holds the loaded libusb library and an initialised context.
pub struct Libusb {
    _lib: Library,
    ctx: *mut c_void,
    exit: FnExit,
    get_dev_list: FnGetDevList,
    free_dev_list: FnFreeDevList,
    get_desc: FnGetDesc,
    get_active_config_desc: FnGetActiveConfigDesc,
    free_config_desc: FnFreeConfigDesc,
    get_bus: FnGetBus,
    get_addr: FnGetAddr,
    open: FnOpen,
    close: FnClose,
    claim: FnClaim,
    release: FnRelease,
    set_alt_setting: FnSetAltSetting,
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
            let get_active_config_desc: FnGetActiveConfigDesc = sym!(
                lib,
                "libusb_get_active_config_descriptor",
                FnGetActiveConfigDesc
            );
            let free_config_desc: FnFreeConfigDesc =
                sym!(lib, "libusb_free_config_descriptor", FnFreeConfigDesc);
            let get_bus: FnGetBus = sym!(lib, "libusb_get_bus_number", FnGetBus);
            let get_addr: FnGetAddr = sym!(lib, "libusb_get_device_address", FnGetAddr);
            let open: FnOpen = sym!(lib, "libusb_open", FnOpen);
            let close: FnClose = sym!(lib, "libusb_close", FnClose);
            let claim: FnClaim = sym!(lib, "libusb_claim_interface", FnClaim);
            let release: FnRelease = sym!(lib, "libusb_release_interface", FnRelease);
            let set_alt_setting: FnSetAltSetting =
                sym!(lib, "libusb_set_interface_alt_setting", FnSetAltSetting);
            let bulk: FnBulk = sym!(lib, "libusb_bulk_transfer", FnBulk);

            let mut ctx: *mut c_void = null_mut();
            let rc = init(&mut ctx);
            if rc != 0 {
                bail!("libusb_init failed: {}", rc);
            }
            Ok(Libusb {
                _lib: lib,
                ctx,
                exit,
                get_dev_list,
                free_dev_list,
                get_desc,
                get_active_config_desc,
                free_config_desc,
                get_bus,
                get_addr,
                open,
                close,
                claim,
                release,
                set_alt_setting,
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

    fn discover_bulk_endpoints(&self, dev: *mut c_void) -> Result<BulkEndpoints> {
        let mut config: *mut ConfigDescriptor = null_mut();
        let rc = unsafe { (self.get_active_config_desc)(dev, &mut config) };
        if rc != 0 || config.is_null() {
            bail!("libusb_get_active_config_descriptor failed: {}", rc);
        }

        let mut interfaces = Vec::new();
        unsafe {
            let descriptor = &*config;
            if !descriptor.interface.is_null() {
                for interface_index in 0..descriptor.b_num_interfaces as usize {
                    let interface = &*descriptor.interface.add(interface_index);
                    if interface.num_altsetting <= 0 || interface.altsetting.is_null() {
                        continue;
                    }
                    for alt_index in 0..interface.num_altsetting as usize {
                        let alt = &*interface.altsetting.add(alt_index);
                        let mut endpoints = Vec::with_capacity(alt.b_num_endpoints as usize);
                        if !alt.endpoint.is_null() {
                            for endpoint_index in 0..alt.b_num_endpoints as usize {
                                let endpoint = &*alt.endpoint.add(endpoint_index);
                                endpoints.push(EndpointInfo {
                                    address: endpoint.b_endpoint_address,
                                    attributes: endpoint.bm_attributes,
                                });
                            }
                        }
                        interfaces.push(InterfaceAltSetting {
                            interface_number: alt.b_interface_number,
                            alternate_setting: alt.b_alternate_setting,
                            endpoints,
                        });
                    }
                }
            }
            (self.free_config_desc)(config);
        }

        choose_bulk_endpoints(&interfaces).ok_or_else(|| {
            anyhow::anyhow!("no USB interface with both bulk IN and bulk OUT endpoints was found")
        })
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

    /// Open a detected device and select its bulk endpoint pair from descriptors.
    pub fn open_transport(&self, device: &UsbDeviceInfo) -> Result<LibusbTransport<'_>> {
        let (list, count) = self.device_list()?;
        let mut found: *mut c_void = null_mut();
        for i in 0..count as usize {
            let dev = unsafe { *list.add(i) };
            let mut desc: DeviceDescriptor = unsafe { std::mem::zeroed() };
            unsafe {
                if (self.get_desc)(dev, &mut desc) == 0
                    && desc.id_vendor == device.vid
                    && desc.id_product == device.pid
                    && (self.get_bus)(dev) == device.bus
                    && (self.get_addr)(dev) == device.address
                {
                    found = dev;
                    break;
                }
            }
        }
        if found.is_null() {
            unsafe { (self.free_dev_list)(list, 1) };
            bail!(
                "USB device {:04x}:{:04x} at bus {} address {} is no longer present",
                device.vid,
                device.pid,
                device.bus,
                device.address
            );
        }
        let mut handle: *mut c_void = null_mut();
        let rc = unsafe { (self.open)(found, &mut handle) };
        if rc != 0 || handle.is_null() {
            unsafe { (self.free_dev_list)(list, 1) };
            bail!("libusb_open failed: {}", rc);
        }

        let endpoints = match self.discover_bulk_endpoints(found) {
            Ok(endpoints) => endpoints,
            Err(error) => {
                unsafe {
                    (self.close)(handle);
                    (self.free_dev_list)(list, 1);
                }
                return Err(error);
            }
        };
        // The open handle holds its own device reference; safe to free the list.
        unsafe { (self.free_dev_list)(list, 1) };

        let interface = endpoints.interface_number as c_int;
        let rc = unsafe { (self.claim)(handle, interface) };
        if rc != 0 {
            unsafe {
                (self.close)(handle);
            }
            bail!(
                "libusb_claim_interface({}) failed: {}. On Windows, this USB path needs a libusb-compatible driver (such as WinUSB); the Qualcomm QDLoader COM-port driver is not supported.",
                interface,
                rc
            );
        }
        if endpoints.alternate_setting != 0 {
            let rc = unsafe {
                (self.set_alt_setting)(
                    handle,
                    interface,
                    endpoints.alternate_setting as c_int,
                )
            };
            if rc != 0 {
                unsafe {
                    (self.release)(handle, interface);
                    (self.close)(handle);
                }
                bail!(
                    "libusb_set_interface_alt_setting({}, {}) failed: {}",
                    interface,
                    endpoints.alternate_setting,
                    rc
                );
            }
        }
        Ok(LibusbTransport {
            libusb: self,
            handle,
            out_ep: endpoints.out_endpoint,
            in_ep: endpoints.in_endpoint,
            timeout_ms: 5000,
            interface,
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
