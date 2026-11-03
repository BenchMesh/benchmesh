//! Windows: give a USBTMC instrument - or a USB device named by its vendor and product id, such as
//! an adapter a distribution reads sensors through - Microsoft's generic WinUSB driver (the in-box `winusb.inf`),
//! the same as Device Manager → Update driver → Browse → Let me pick → Universal Serial Bus
//! devices → WinUsb Device. Nothing ships with BenchMesh, nothing is downloaded or self-signed.
//!
//! Installing a driver needs administrator rights, so [`install`] (called by the server, which
//! runs as the user) starts the running executable again elevated - one UAC prompt - with
//! [`HELPER_FLAG`]. Every binary's `main` hands such a start to [`helper_main`] before anything
//! else. The helper writes its error text into a result file named on its command line.

use std::io;

/// `<exe> --install-winusb <usb: path> <result file>`
pub const HELPER_FLAG: &str = "--install-winusb";

/// First thing in `main`: `Some(exit code)` when this process is the elevated helper.
pub fn helper_main() -> Option<i32> {
    let args: Vec<String> = std::env::args().collect();
    let i = args.iter().position(|a| a == HELPER_FLAG)?;
    let (Some(path), Some(result)) = (args.get(i + 1), args.get(i + 2)) else { return Some(2) };
    let r = imp::bind(path);
    let _ = std::fs::write(result, r.as_ref().err().map(|e| e.to_string()).unwrap_or_default());
    Some(if r.is_ok() { 0 } else { 1 })
}

/// `usb:1a86:5512`: a USB device named by its vendor and product id only. No instrument path has
/// this form (those carry a serial number or `@location`).
pub fn usb_id_path(vid: u16, pid: u16) -> String {
    format!("{}{vid:04x}:{pid:04x}", crate::usbtmc_nusb::PREFIX)
}

/// The vendor and product id of a [`usb_id_path`].
pub fn parse_usb_id_path(path: &str) -> Option<(u16, u16)> {
    let (v, p) = path.strip_prefix(crate::usbtmc_nusb::PREFIX)?.split_once(':')?;
    let hex = |s: &str| (s.len() == 4 && s.chars().all(|c| c.is_ascii_hexdigit())).then(|| u16::from_str_radix(s, 16).ok()).flatten();
    Some((hex(v)?, hex(p)?))
}

/// Give the attached USB device `vid`:`pid` the WinUSB driver (see [`install`]).
pub fn install_usb_id(vid: u16, pid: u16) -> io::Result<()> {
    install(&usb_id_path(vid, pid))
}

/// Give the instrument at `usb:` path `path` (or the device of a [`usb_id_path`]) the WinUSB driver. Blocks until the elevated helper
/// is done (including the time the user takes to answer the UAC prompt).
pub fn install(path: &str) -> io::Result<()> {
    if path.contains('"') || !path.starts_with(crate::usbtmc_nusb::PREFIX) {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, format!("{path:?} is not a usb: instrument path")));
    }
    imp::install_elevated(path)
}

#[cfg(not(windows))]
mod imp {
    use std::io;

    fn unsupported() -> io::Error {
        io::Error::new(io::ErrorKind::Unsupported, "driver installation is only needed (and possible) on Windows")
    }

    pub fn bind(_path: &str) -> io::Result<()> {
        Err(unsupported())
    }

    pub fn install_elevated(_path: &str) -> io::Result<()> {
        Err(unsupported())
    }
}

#[cfg(windows)]
mod imp {
    use std::io;
    use std::ptr;
    use std::sync::atomic::{AtomicU32, Ordering};

    use windows_sys::core::GUID;
    use windows_sys::Win32::Devices::DeviceAndDriverInstallation::{
        DiInstallDevice, SetupDiBuildDriverInfoList, SetupDiCreateDeviceInfoList, SetupDiDestroyDeviceInfoList, SetupDiDestroyDriverInfoList,
        SetupDiEnumDriverInfoW, SetupDiGetDeviceInstallParamsW, SetupDiOpenDeviceInfoW, SetupDiSetDeviceInstallParamsW, SetupDiSetDeviceRegistryPropertyW,
        DI_ENUMSINGLEINF, DI_FLAGSEX_ALLOWEXCLUDEDDRVS, HDEVINFO, SPDIT_CLASSDRIVER, SPDRP_CLASSGUID, SP_DEVINFO_DATA, SP_DEVINSTALL_PARAMS_W,
        SP_DRVINFO_DATA_V2_W,
    };
    use windows_sys::Win32::Foundation::{CloseHandle, BOOL, ERROR_CANCELLED, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject, INFINITE};
    use windows_sys::Win32::UI::Shell::{ShellExecuteExW, SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW};
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    /// Setup class "Universal Serial Bus devices" (`USBDevice`), the class of `winusb.inf`.
    const USB_DEVICE_CLASS: GUID = GUID::from_u128(0x88bae032_5a81_49f0_bc3d_a4ff138216d6);
    const USB_DEVICE_CLASS_TEXT: &str = "{88BAE032-5A81-49F0-BC3D-A4FF138216D6}";

    fn same_guid(a: &GUID, b: &GUID) -> bool {
        (a.data1, a.data2, a.data3, a.data4) == (b.data1, b.data2, b.data3, b.data4)
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    fn check(ok: BOOL, what: &str) -> io::Result<()> {
        if ok != 0 {
            return Ok(());
        }
        let e = io::Error::last_os_error();
        Err(io::Error::new(e.kind(), format!("{what}: {e}")))
    }

    fn winusb_inf() -> io::Result<Vec<u16>> {
        let mut buf = [0u16; 260];
        // SAFETY: `buf` is valid for `buf.len()` u16s.
        let n = unsafe { GetWindowsDirectoryW(buf.as_mut_ptr(), buf.len() as u32) } as usize;
        if n == 0 || n >= buf.len() {
            return Err(io::Error::other("cannot find the Windows directory"));
        }
        Ok(wide(&format!("{}\\INF\\winusb.inf", String::from_utf16_lossy(&buf[..n]))))
    }

    pub fn bind(path: &str) -> io::Result<()> {
        let id = crate::discover::windows_usbtmc_instance(path)
            .or_else(|| super::parse_usb_id_path(path).and_then(|(vid, pid)| crate::discover::windows_usb_instance(vid, pid)))
            .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, format!("{path}: no such USB device attached")))?;
        // SAFETY: no class GUID, no window.
        let set = unsafe { SetupDiCreateDeviceInfoList(ptr::null(), 0) };
        if set == INVALID_HANDLE_VALUE {
            return Err(io::Error::last_os_error());
        }
        let r = bind_in(set, &id);
        // SAFETY: `set` came from SetupDiCreateDeviceInfoList and is destroyed exactly once.
        unsafe { SetupDiDestroyDeviceInfoList(set) };
        r
    }

    fn bind_in(set: HDEVINFO, instance_id: &str) -> io::Result<()> {
        let id = wide(instance_id);
        // SAFETY: SP_DEVINFO_DATA is plain data; all-zero is valid before setting cbSize.
        let mut info: SP_DEVINFO_DATA = unsafe { std::mem::zeroed() };
        info.cbSize = std::mem::size_of::<SP_DEVINFO_DATA>() as u32;
        // SAFETY: `set` is live, `id` is NUL-terminated, `info` has its cbSize set.
        check(unsafe { SetupDiOpenDeviceInfoW(set, id.as_ptr(), 0, 0, &mut info) }, &format!("open {instance_id}"))?;
        let driver = match winusb_driver(set, &mut info)? {
            Some(d) => d,
            None => {
                // The class driver list only holds drivers of the device's own setup class; a
                // device without a driver has none (or another one): make it a USB device first.
                // SAFETY: `set` and `info` are live; the list is rebuilt below.
                unsafe { SetupDiDestroyDriverInfoList(set, &info, SPDIT_CLASSDRIVER) };
                let class = wide(USB_DEVICE_CLASS_TEXT);
                // SAFETY: `class` is valid for the byte length passed (including the NUL).
                let ok = unsafe { SetupDiSetDeviceRegistryPropertyW(set, &mut info, SPDRP_CLASSGUID, class.as_ptr().cast(), (class.len() * 2) as u32) };
                check(ok, "set the device class to Universal Serial Bus devices")?;
                winusb_driver(set, &mut info)?.ok_or_else(|| io::Error::other("winusb.inf offers no driver for this instrument"))?
            }
        };
        // SAFETY: `set`, `info` and `driver` are live; a null NeedReboot lets Windows ask for a
        // restart itself if one is needed.
        check(unsafe { DiInstallDevice(0, set, &info, &driver, 0, ptr::null_mut()) }, "install WinUSB")
    }

    /// The WinUsb Device driver of `winusb.inf`, if Windows offers it for the device's class.
    fn winusb_driver(set: HDEVINFO, info: &mut SP_DEVINFO_DATA) -> io::Result<Option<SP_DRVINFO_DATA_V2_W>> {
        // SAFETY: plain data; cbSize set before use.
        let mut params: SP_DEVINSTALL_PARAMS_W = unsafe { std::mem::zeroed() };
        params.cbSize = std::mem::size_of::<SP_DEVINSTALL_PARAMS_W>() as u32;
        // SAFETY: `set` and `info` are live, `params` has its cbSize set.
        check(unsafe { SetupDiGetDeviceInstallParamsW(set, info, &mut params) }, "read install parameters")?;
        params.Flags |= DI_ENUMSINGLEINF as u32;
        params.FlagsEx |= DI_FLAGSEX_ALLOWEXCLUDEDDRVS as u32;
        let inf = winusb_inf()?;
        params.DriverPath = [0; 260];
        params.DriverPath[..inf.len()].copy_from_slice(&inf);
        // SAFETY: as above.
        check(unsafe { SetupDiSetDeviceInstallParamsW(set, info, &params) }, "set install parameters")?;
        // SAFETY: as above.
        check(unsafe { SetupDiBuildDriverInfoList(set, info, SPDIT_CLASSDRIVER) }, "list the WinUSB driver")?;
        if !same_guid(&info.ClassGuid, &USB_DEVICE_CLASS) {
            return Ok(None);
        }
        // SAFETY: plain data; cbSize set before use.
        let mut driver: SP_DRVINFO_DATA_V2_W = unsafe { std::mem::zeroed() };
        driver.cbSize = std::mem::size_of::<SP_DRVINFO_DATA_V2_W>() as u32;
        // SAFETY: `set` and `info` are live, `driver` has its cbSize set.
        let found = unsafe { SetupDiEnumDriverInfoW(set, info, SPDIT_CLASSDRIVER, 0, &mut driver) } != 0;
        Ok(found.then_some(driver))
    }

    static RUN: AtomicU32 = AtomicU32::new(0);

    pub fn install_elevated(path: &str) -> io::Result<()> {
        let exe = std::env::current_exe()?;
        let result = std::env::temp_dir().join(format!("benchmesh-winusb-{}-{}.txt", std::process::id(), RUN.fetch_add(1, Ordering::Relaxed)));
        let _ = std::fs::remove_file(&result);
        let verb = wide("runas");
        let file = wide(&exe.to_string_lossy());
        let params = wide(&format!("{} \"{path}\" \"{}\"", super::HELPER_FLAG, result.display()));
        // SAFETY: plain data; cbSize set before use.
        let mut sei: SHELLEXECUTEINFOW = unsafe { std::mem::zeroed() };
        sei.cbSize = std::mem::size_of::<SHELLEXECUTEINFOW>() as u32;
        sei.fMask = SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC;
        sei.lpVerb = verb.as_ptr();
        sei.lpFile = file.as_ptr();
        sei.lpParameters = params.as_ptr();
        sei.nShow = SW_HIDE;
        // SAFETY: every string pointer stays valid for the call.
        if unsafe { ShellExecuteExW(&mut sei) } == 0 {
            let e = io::Error::last_os_error();
            if e.raw_os_error() == Some(ERROR_CANCELLED as i32) {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "the administrator approval was declined; the driver was not installed"));
            }
            return Err(io::Error::new(e.kind(), format!("start the driver installation: {e}")));
        }
        let mut code = 1u32;
        // SAFETY: `hProcess` is the helper's handle (SEE_MASK_NOCLOSEPROCESS), closed exactly once.
        unsafe {
            WaitForSingleObject(sei.hProcess, INFINITE);
            GetExitCodeProcess(sei.hProcess, &mut code);
            CloseHandle(sei.hProcess);
        }
        let text = std::fs::read_to_string(&result).unwrap_or_default();
        let _ = std::fs::remove_file(&result);
        match (code, text.trim()) {
            (0, _) => Ok(()),
            (_, "") => Err(io::Error::other(format!("the driver installation failed (exit code {code})"))),
            (_, t) => Err(io::Error::other(t.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_usb_paths_reach_the_elevated_helper() {
        for bad in ["/dev/usbtmc0", "USB0::0x1AB1::0x044D::X::INSTR", "usb:1ab1:044d:X\" --evil \""] {
            assert_eq!(install(bad).unwrap_err().kind(), io::ErrorKind::InvalidInput, "{bad}");
        }
    }

    #[test]
    fn a_normal_start_is_not_the_helper() {
        assert_eq!(helper_main(), None);
    }

    #[test]
    fn a_device_is_named_by_its_ids_in_a_form_no_instrument_path_has() {
        assert_eq!(usb_id_path(0x1a86, 0x5512), "usb:1a86:5512");
        assert_eq!(parse_usb_id_path("usb:1a86:5512"), Some((0x1a86, 0x5512)));
        assert_eq!(parse_usb_id_path(&crate::usbtmc_nusb::path_of(0x1ab1, 0x044d, Some("DHO8A1"), "")), None);
        assert_eq!(parse_usb_id_path(&crate::usbtmc_nusb::path_of(0x1ab1, 0x044d, None, "1-2")), None);
        assert_eq!(parse_usb_id_path("usb:1a86:55"), None);
        assert_eq!(parse_usb_id_path("1a86:5512"), None);
    }
}
