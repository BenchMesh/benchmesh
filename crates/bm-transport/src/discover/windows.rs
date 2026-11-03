//! Windows: USB serial ports (COM) via SetupAPI / CfgMgr32 (no admin rights needed).
//!
//! Ports-class devices → `PortName` from the device registry key → walk up the device tree to the
//! USB device node (CDC-ACM interface `USB\..&MI_00\..`, FTDI `FTDIBUS\..` are skipped over) →
//! vid/pid/revision from its hardware id, serial from its instance id, strings and location from
//! its properties. Ports without a USB ancestor (COM1, Bluetooth) are left out.
//!
//! USBTMC instruments: every present USB node whose compatible ids name the USBTMC class (the
//! device itself or, for a composite device, its interface), with the driver Windows bound to
//! it. Listed also when no driver is installed (such devices are invisible to `nusb`), as is any
//! USB device asked for by its vendor and product id (`usb_devices`).

use std::ptr;

use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
    CM_Get_DevNode_PropertyW, CM_Get_DevNode_Registry_PropertyW, CM_Get_Device_IDW, CM_Get_Parent, SetupDiDestroyDeviceInfoList, SetupDiEnumDeviceInfo,
    SetupDiGetClassDevsW, SetupDiOpenDevRegKey, CM_DRP_COMPATIBLEIDS, CM_DRP_DEVICEDESC, CM_DRP_HARDWAREID, CM_DRP_LOCATION_INFORMATION, CM_DRP_LOCATION_PATHS,
    CM_DRP_MFG, CM_DRP_SERVICE, CR_SUCCESS, DICS_FLAG_GLOBAL, DIGCF_ALLCLASSES, DIGCF_PRESENT, DIREG_DEV, GUID_DEVCLASS_PORTS, MAX_DEVICE_ID_LEN,
    SP_DEVINFO_DATA,
};
use windows_sys::Win32::Devices::Properties::{DEVPKEY_Device_BusReportedDeviceDesc, DEVPROPKEY, DEVPROP_TYPE_STRING};
use windows_sys::Win32::Foundation::INVALID_HANDLE_VALUE;
use windows_sys::Win32::System::Registry::{RegCloseKey, RegQueryValueExW, KEY_READ, REG_SZ};

use super::ids::{win_hardware_id, win_instance_serial, win_is_usb_device, win_is_usb_id, win_is_usbtmc_compatible_id};
use super::{text, Port, PortKind, UsbInfo};

/// Device-tree levels walked up from the COM port (port → interface/FTDIBUS → USB device).
const MAX_DEPTH: usize = 8;

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(Some(0)).collect()
}

/// UTF-16 up to the first NUL (the first entry of a REG_MULTI_SZ), trimmed, `None` when empty.
fn first_string(buf: &[u16]) -> Option<String> {
    let end = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    text(Some(String::from_utf16_lossy(&buf[..end])))
}

fn device_id(devinst: u32) -> Option<String> {
    let mut buf = [0u16; MAX_DEVICE_ID_LEN as usize + 1];
    // SAFETY: `buf` is valid for `buf.len()` u16s; the length passed leaves room for the NUL.
    let r = unsafe { CM_Get_Device_IDW(devinst, buf.as_mut_ptr(), buf.len() as u32 - 1, 0) };
    (r == CR_SUCCESS).then(|| first_string(&buf)).flatten()
}

fn parent(devinst: u32) -> Option<u32> {
    let mut p = 0u32;
    // SAFETY: `p` is a valid out pointer.
    let r = unsafe { CM_Get_Parent(&mut p, devinst, 0) };
    (r == CR_SUCCESS).then_some(p)
}

/// UTF-16 multi-sz → its non-empty entries.
fn all_strings(buf: &[u16]) -> Vec<String> {
    buf.split(|&c| c == 0).map(String::from_utf16_lossy).map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).collect()
}

/// A string (first entry of a multi-sz) registry property of a device node.
fn registry_property(devinst: u32, prop: u32) -> Option<String> {
    registry_strings(devinst, prop).into_iter().next()
}

/// All entries of a multi-sz (or the one string) registry property of a device node.
fn registry_strings(devinst: u32, prop: u32) -> Vec<String> {
    let mut ty = 0u32;
    let mut len = 0u32;
    // SAFETY: a null buffer with length 0 only asks for the required size (CR_BUFFER_SMALL).
    unsafe { CM_Get_DevNode_Registry_PropertyW(devinst, prop, &mut ty, ptr::null_mut(), &mut len, 0) };
    if len == 0 {
        return Vec::new();
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` is valid for `len` bytes.
    let r = unsafe { CM_Get_DevNode_Registry_PropertyW(devinst, prop, &mut ty, buf.as_mut_ptr().cast(), &mut len, 0) };
    if r == CR_SUCCESS { all_strings(&buf) } else { Vec::new() }
}

/// A string device property (DEVPKEY) of a device node.
fn string_property(devinst: u32, key: &DEVPROPKEY) -> Option<String> {
    let mut ty = 0u32;
    let mut len = 0u32;
    // SAFETY: a null buffer with length 0 only asks for the required size (CR_BUFFER_SMALL).
    unsafe { CM_Get_DevNode_PropertyW(devinst, key, &mut ty, ptr::null_mut(), &mut len, 0) };
    if len == 0 {
        return None;
    }
    let mut buf = vec![0u16; (len as usize).div_ceil(2) + 1];
    let mut len = (buf.len() * 2) as u32;
    // SAFETY: `buf` is valid for `len` bytes; `key` and the out pointers are valid.
    let r = unsafe { CM_Get_DevNode_PropertyW(devinst, key, &mut ty, buf.as_mut_ptr().cast(), &mut len, 0) };
    (r == CR_SUCCESS && ty == DEVPROP_TYPE_STRING).then(|| first_string(&buf)).flatten()
}

/// The USB device node at or above `devinst` (its devinst and instance id).
fn usb_device(devinst: u32) -> Option<(u32, String)> {
    let mut node = devinst;
    for _ in 0..MAX_DEPTH {
        let id = device_id(node)?;
        if win_is_usb_device(&id) {
            return Some((node, id));
        }
        node = parent(node)?;
    }
    None
}

fn usb_info(devinst: u32) -> Option<UsbInfo> {
    let (node, instance_id) = usb_device(devinst)?;
    let (vendor_id, product_id, revision) = registry_property(node, CM_DRP_HARDWAREID).map(|h| win_hardware_id(&h)).unwrap_or_else(|| win_hardware_id(&instance_id));
    Some(UsbInfo {
        vendor_id,
        product_id,
        manufacturer: registry_property(node, CM_DRP_MFG),
        product: string_property(node, &DEVPKEY_Device_BusReportedDeviceDesc).or_else(|| registry_property(node, CM_DRP_DEVICEDESC)),
        serial: win_instance_serial(&instance_id),
        revision,
        location: registry_property(node, CM_DRP_LOCATION_PATHS).or_else(|| registry_property(node, CM_DRP_LOCATION_INFORMATION)),
    })
}

/// `PortName` (`COM5`) from the device's hardware registry key.
fn port_name(set: isize, info: &SP_DEVINFO_DATA) -> Option<String> {
    // SAFETY: `set` is a live device info set and `info` one of its elements.
    let key = unsafe { SetupDiOpenDevRegKey(set, info, DICS_FLAG_GLOBAL, 0, DIREG_DEV, KEY_READ) };
    if key == INVALID_HANDLE_VALUE {
        return None;
    }
    let name = wide("PortName");
    let mut buf = [0u16; 64];
    let mut len = (buf.len() * 2 - 2) as u32;
    let mut ty = 0u32;
    // SAFETY: `key` is open; `buf` is valid for `len` bytes (one u16 kept for the NUL).
    let r = unsafe { RegQueryValueExW(key, name.as_ptr(), ptr::null(), &mut ty, buf.as_mut_ptr().cast(), &mut len) };
    // SAFETY: `key` was opened above and is closed exactly once.
    unsafe { RegCloseKey(key) };
    (r == 0 && ty == REG_SZ).then(|| first_string(&buf[..(len as usize / 2).min(buf.len())])).flatten()
}

pub fn ports() -> Vec<Port> {
    // SAFETY: valid class GUID pointer, no enumerator, no window.
    let set = unsafe { SetupDiGetClassDevsW(&GUID_DEVCLASS_PORTS, ptr::null(), 0, DIGCF_PRESENT) };
    if set == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0.. {
        // SAFETY: SP_DEVINFO_DATA is plain data; all-zero is valid before setting cbSize.
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set` is live and `info` has its cbSize set.
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut info) } == 0 {
            break;
        }
        let Some(path) = port_name(set, &info).filter(|p| p.to_ascii_uppercase().starts_with("COM")) else { continue };
        if let Some(usb) = usb_info(info.DevInst) {
            out.push(Port { kind: PortKind::Serial, path, usb });
        }
    }
    // SAFETY: `set` came from SetupDiGetClassDevsW and is destroyed exactly once.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}

/// USBTMC instruments attached, each with the service (driver) Windows bound to its USBTMC
/// interface (`""` when none is installed) and the instance id of the device node that driver
/// belongs to (the device, or its USBTMC interface on a composite device). The path is the
/// `usb:` path of `usbtmc_nusb`.
pub fn usbtmc_ports() -> Vec<(Port, String, String)> {
    let usb = wide("USB");
    // SAFETY: no class GUID (all classes), a valid enumerator string, no window.
    let set = unsafe { SetupDiGetClassDevsW(ptr::null(), usb.as_ptr(), 0, DIGCF_ALLCLASSES | DIGCF_PRESENT) };
    if set == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut out: Vec<(Port, String, String)> = Vec::new();
    let mut seen: Vec<u32> = Vec::new();
    for index in 0.. {
        // SAFETY: SP_DEVINFO_DATA is plain data; all-zero is valid before setting cbSize.
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set` is live and `info` has its cbSize set.
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut info) } == 0 {
            break;
        }
        if !registry_strings(info.DevInst, CM_DRP_COMPATIBLEIDS).iter().any(|id| win_is_usbtmc_compatible_id(id)) {
            continue;
        }
        let Some((node, _)) = usb_device(info.DevInst) else { continue };
        let Some(usb) = usb_info(node) else { continue };
        if seen.contains(&node) {
            continue;
        }
        seen.push(node);
        let id = |h: &Option<String>| h.as_deref().and_then(|h| u16::from_str_radix(h, 16).ok());
        let (Some(vid), Some(pid)) = (id(&usb.vendor_id), id(&usb.product_id)) else { continue };
        let path = crate::usbtmc_nusb::path_of(vid, pid, usb.serial.as_deref(), usb.location.as_deref().unwrap_or_default());
        let service = registry_property(info.DevInst, CM_DRP_SERVICE).unwrap_or_default();
        let Some(instance_id) = device_id(info.DevInst) else { continue };
        out.push((Port { kind: PortKind::Usbtmc, path, usb }, service, instance_id));
    }
    // SAFETY: `set` came from SetupDiGetClassDevsW and is destroyed exactly once.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}

/// Each attached USB device (the device node, not an interface of a composite device) with the
/// vendor and product id `vid`:`pid`: the service (driver) Windows bound to it (`""` when none
/// is installed) and its instance id. Listed whatever its driver, unlike in `nusb`.
pub fn usb_devices(vid: u16, pid: u16) -> Vec<(String, String)> {
    let usb = wide("USB");
    // SAFETY: no class GUID (all classes), a valid enumerator string, no window.
    let set = unsafe { SetupDiGetClassDevsW(ptr::null(), usb.as_ptr(), 0, DIGCF_ALLCLASSES | DIGCF_PRESENT) };
    if set == INVALID_HANDLE_VALUE {
        return Vec::new();
    }
    let mut out = Vec::new();
    for index in 0.. {
        // SAFETY: SP_DEVINFO_DATA is plain data; all-zero is valid before setting cbSize.
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set` is live and `info` has its cbSize set.
        if unsafe { SetupDiEnumDeviceInfo(set, index, &mut info) } == 0 {
            break;
        }
        let Some(id) = device_id(info.DevInst) else { continue };
        if win_is_usb_device(&id) && win_is_usb_id(&id, vid, pid) {
            out.push((registry_property(info.DevInst, CM_DRP_SERVICE).unwrap_or_default(), id));
        }
    }
    // SAFETY: `set` came from SetupDiGetClassDevsW and is destroyed exactly once.
    unsafe { SetupDiDestroyDeviceInfoList(set) };
    out
}
