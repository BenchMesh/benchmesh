//! Port discovery: the configuration UI's port lists, and the USB ports with their USB
//! information ([`ports`], see `ai_context/migration/IDENTITY.md`).
//! Linux: `/dev` + sysfs (no libudev); Windows: SetupAPI; macOS: IORegistry. No rights needed.

mod ids;
#[cfg(target_os = "linux")]
mod linux;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UsbInfo {
    /// 4 lowercase hex digits, e.g. `1a86`.
    pub vendor_id: Option<String>,
    pub product_id: Option<String>,
    pub manufacturer: Option<String>,
    pub product: Option<String>,
    /// USB serial number; `None` when the device has none.
    pub serial: Option<String>,
    /// `bcdDevice`, 4 lowercase hex digits (chip/firmware revision).
    pub revision: Option<String>,
    /// Physical port position (OS-specific text); stable while the device stays on that port.
    pub location: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PortKind {
    Serial,
    Usbtmc,
}

impl PortKind {
    pub fn as_str(self) -> &'static str {
        match self {
            PortKind::Serial => "serial",
            PortKind::Usbtmc => "usbtmc",
        }
    }
}

/// A USB-attached port an instrument can be identified on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Port {
    pub kind: PortKind,
    /// What to open: `/dev/ttyUSB0`, `COM5`, `/dev/cu.usbserial-110`, `/dev/usbtmc2`.
    pub path: String,
    pub usb: UsbInfo,
}

/// USB serial ports and USBTMC instruments currently attached (non-USB ports are left out).
pub fn ports() -> Vec<Port> {
    #[cfg(target_os = "linux")]
    let mut out = linux::ports();
    #[cfg(windows)]
    let mut out = windows::ports();
    #[cfg(target_os = "macos")]
    let mut out = macos::ports();
    #[cfg(not(any(target_os = "linux", windows, target_os = "macos")))]
    let mut out: Vec<Port> = Vec::new();
    #[cfg(not(target_os = "linux"))]
    out.extend(usbtmc_ports());
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// USBTMC instruments on Windows / macOS (Linux: kernel nodes, listed by `linux.rs`), each with
/// why it cannot be opened (`None`: it can). On Windows an instrument whose driver belongs to an
/// installed VISA is reached through that VISA.
#[cfg(not(target_os = "linux"))]
fn usbtmc_ports_noted() -> Vec<(Port, Option<String>)> {
    #[cfg(windows)]
    {
        windows_usbtmc()
    }
    #[cfg(not(windows))]
    {
        crate::usbtmc_nusb::ports().into_iter().map(|(p, _)| (p, None)).collect()
    }
}

#[cfg(not(target_os = "linux"))]
fn usbtmc_ports() -> Vec<Port> {
    usbtmc_ports_noted().into_iter().map(|(p, _)| p).collect()
}

/// The Windows device tree lists every USBTMC instrument (also ones without a driver); `nusb`
/// supplies the descriptor strings of those it can see, a VISA resource replaces the path of
/// those its driver owns.
#[cfg(windows)]
fn windows_usbtmc() -> Vec<(Port, Option<String>)> {
    let visa: Vec<(String, crate::visa::UsbId)> =
        crate::visa::resources().into_iter().filter_map(|r| Some((r.clone(), crate::visa::parse_usb_resource(&r)?))).collect();
    let seen: Vec<Port> = crate::usbtmc_nusb::ports().into_iter().map(|(p, _)| p).collect();
    windows::usbtmc_ports()
        .into_iter()
        .map(|(port, service, _)| {
            let same_usb = |u: &UsbInfo| {
                u.vendor_id == port.usb.vendor_id && u.product_id == port.usb.product_id && same_serial(u.serial.as_deref(), port.usb.serial.as_deref())
            };
            let mut p = seen.iter().find(|n| same_usb(&n.usb)).cloned().unwrap_or(port);
            if crate::usbtmc_nusb::usable_service(&service) {
                return (p, None);
            }
            let via_visa = visa.iter().find(|(_, (v, pid, s))| {
                p.usb.vendor_id == Some(format!("{v:04x}")) && p.usb.product_id == Some(format!("{pid:04x}")) && same_serial(s.as_deref(), p.usb.serial.as_deref())
            });
            match via_visa {
                Some((resource, _)) => {
                    p.path = resource.clone();
                    (p, None)
                }
                None => (p, Some(crate::usbtmc_nusb::driver_problem(&service))),
            }
        })
        .collect()
}

/// `usb:` path and Windows service (driver; `""`: none) of each attached USBTMC instrument.
#[cfg(windows)]
pub(crate) fn windows_usbtmc_services() -> Vec<(String, String)> {
    windows::usbtmc_ports().into_iter().map(|(p, service, _)| (p.path, service)).collect()
}

/// Instance id of the device node of the USBTMC instrument at `usb:` path `path` (the node a
/// driver is installed on).
#[cfg(windows)]
pub(crate) fn windows_usbtmc_instance(path: &str) -> Option<String> {
    windows::usbtmc_ports().into_iter().find(|(p, _, _)| p.path.eq_ignore_ascii_case(path)).map(|(_, _, id)| id)
}

/// Windows: the driver (service) of the attached USB device `vid`:`pid` (`""`: none installed),
/// `None` when it is not attached. Other systems need no driver for `nusb`: always `None`.
pub fn windows_usb_service(vid: u16, pid: u16) -> Option<String> {
    #[cfg(windows)]
    {
        windows::usb_devices(vid, pid).into_iter().next().map(|(service, _)| service)
    }
    #[cfg(not(windows))]
    {
        let _ = (vid, pid);
        None
    }
}

/// Instance id of the device node of the attached USB device `vid`:`pid`.
#[cfg(windows)]
pub(crate) fn windows_usb_instance(vid: u16, pid: u16) -> Option<String> {
    windows::usb_devices(vid, pid).into_iter().next().map(|(_, id)| id)
}

/// Windows reports serial numbers in instance ids without their original case.
#[cfg(windows)]
fn same_serial(a: Option<&str>, b: Option<&str>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => a.eq_ignore_ascii_case(b),
        (a, b) => a.is_none() && b.is_none(),
    }
}

/// `0x1A86`, `1a86`, `6790` (decimal from some APIs is converted by the caller) → `1a86`.
pub(crate) fn hex4(s: &str) -> Option<String> {
    let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
    (!t.is_empty() && t.len() <= 4 && t.chars().all(|c| c.is_ascii_hexdigit())).then(|| format!("{:0>4}", t.to_ascii_lowercase()))
}

/// Trimmed, `None` when empty.
pub(crate) fn text(s: Option<String>) -> Option<String> {
    s.map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SerialPortInfo {
    /// Path to open (stable `/dev/serial/by-id/...` links are listed too).
    pub device: String,
    pub description: String,
    pub usb: UsbInfo,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UsbTmcInfo {
    pub device: String,
    pub name: String,
    pub usb: UsbInfo,
    /// Why the instrument cannot be opened (e.g. Windows has no usable driver for it).
    pub note: Option<String>,
    /// BenchMesh can give it the WinUSB driver ([`crate::winusb::install`]; Windows only).
    pub driver_installable: bool,
}

#[cfg(target_os = "linux")]
pub fn serial_ports() -> Vec<SerialPortInfo> {
    linux::serial_ports()
}

#[cfg(not(target_os = "linux"))]
pub fn serial_ports() -> Vec<SerialPortInfo> {
    let mut out: Vec<SerialPortInfo> = serialport::available_ports()
        .unwrap_or_default()
        .into_iter()
        .map(|p| {
            let usb = match &p.port_type {
                serialport::SerialPortType::UsbPort(u) => UsbInfo {
                    vendor_id: Some(format!("{:04x}", u.vid)),
                    product_id: Some(format!("{:04x}", u.pid)),
                    manufacturer: u.manufacturer.clone(),
                    product: u.product.clone(),
                    serial: u.serial_number.clone(),
                    ..UsbInfo::default()
                },
                _ => UsbInfo::default(),
            };
            SerialPortInfo { description: usb.product.clone().unwrap_or_else(|| "Unknown Device".into()), device: p.port_name, usb }
        })
        .collect();
    out.sort_by(|a, b| a.device.cmp(&b.device));
    out
}

/// USBTMC instruments: Linux kernel device nodes `/dev/usbtmcN`; Windows / macOS `usb:` paths
/// (or VISA resources).
pub fn usbtmc_devices() -> Vec<UsbTmcInfo> {
    #[cfg(target_os = "linux")]
    let noted: Vec<(Port, Option<String>)> = ports().into_iter().filter(|p| p.kind == PortKind::Usbtmc).map(|p| (p, None)).collect();
    #[cfg(not(target_os = "linux"))]
    let mut noted = usbtmc_ports_noted();
    #[cfg(not(target_os = "linux"))]
    noted.sort_by(|a, b| a.0.path.cmp(&b.0.path));
    noted
        .into_iter()
        .map(|(p, note)| UsbTmcInfo {
            name: p.path.rsplit('/').next().unwrap_or_default().to_string(),
            driver_installable: cfg!(windows) && note.is_some() && p.path.starts_with("usb:"),
            device: p.path,
            usb: p.usb,
            note,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hex4_normalises() {
        assert_eq!(hex4("1A86").as_deref(), Some("1a86"));
        assert_eq!(hex4("0x264").as_deref(), Some("0264"));
        assert_eq!(hex4(" 0264\n").as_deref(), Some("0264"));
        assert_eq!(hex4(""), None);
        assert_eq!(hex4("12345"), None);
        assert_eq!(hex4("zz"), None);
    }
}
