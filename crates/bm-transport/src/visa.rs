//! VISA (NI-VISA, Keysight IO Libraries, Rigol UltraSigma, R&S VISA …), loaded at run time when
//! one is installed - no SDK needed to build. On Windows a VISA's driver owns the instrument,
//! so this is how BenchMesh reaches USBTMC instruments there without changing drivers.
//!
//! Port path: the VISA resource string, e.g. `USB0::0x1AB1::0x044D::DHO8A272907001::INSTR`.

use std::ffi::{c_char, CStr, CString};
use std::io;
use std::sync::OnceLock;
use std::time::Duration;

use bm_driver::engine::Transport;
use libloading::Library;

type ViStatus = i32;
type ViSession = u32;
#[cfg(target_pointer_width = "64")]
type ViAttrState = u64;
#[cfg(not(target_pointer_width = "64"))]
type ViAttrState = u32;

const VI_SUCCESS_TERM_CHAR: ViStatus = 0x3FFF_0005;
const VI_SUCCESS_MAX_CNT: ViStatus = 0x3FFF_0006;
const VI_ERROR_TMO: ViStatus = 0xBFFF_0015_u32 as i32;
const VI_ATTR_TMO_VALUE: u32 = 0x3FFF_001A;
const VI_ATTR_SEND_END_EN: u32 = 0x3FFF_0016;
const VI_ATTR_TERMCHAR_EN: u32 = 0x3FFF_0038;

type OpenDefaultRm = unsafe extern "system" fn(*mut ViSession) -> ViStatus;
type FindRsrc = unsafe extern "system" fn(ViSession, *const c_char, *mut u32, *mut u32, *mut c_char) -> ViStatus;
type FindNext = unsafe extern "system" fn(u32, *mut c_char) -> ViStatus;
type Open = unsafe extern "system" fn(ViSession, *const c_char, u32, u32, *mut ViSession) -> ViStatus;
type Close = unsafe extern "system" fn(u32) -> ViStatus;
type ReadFn = unsafe extern "system" fn(ViSession, *mut u8, u32, *mut u32) -> ViStatus;
type WriteFn = unsafe extern "system" fn(ViSession, *const u8, u32, *mut u32) -> ViStatus;
type SetAttribute = unsafe extern "system" fn(ViSession, u32, ViAttrState) -> ViStatus;
type StatusDesc = unsafe extern "system" fn(ViSession, ViStatus, *mut c_char) -> ViStatus;

struct Api {
    _lib: Library,
    rm: ViSession,
    find_rsrc: FindRsrc,
    find_next: FindNext,
    open: Open,
    close: Close,
    read: ReadFn,
    write: WriteFn,
    set_attribute: SetAttribute,
    status_desc: StatusDesc,
}

const LIBRARIES: &[&str] = if cfg!(windows) {
    &["visa64.dll", "visa32.dll"]
} else if cfg!(target_os = "macos") {
    &["/Library/Frameworks/VISA.framework/VISA"]
} else {
    &["libvisa.so", "libvisa.so.0", "librsvisa.so"]
};

fn load() -> Option<Api> {
    LIBRARIES.iter().find_map(|name| {
        // SAFETY: loading a VISA library runs its initialisers; VISA libraries are designed for this.
        let lib = unsafe { Library::new(name) }.ok()?;
        // SAFETY: the symbol types follow the VISA specification (visa.h, VPP-4.3.2).
        unsafe {
            let open_default_rm = *lib.get::<OpenDefaultRm>(b"viOpenDefaultRM\0").ok()?;
            let api = Api {
                rm: 0,
                find_rsrc: *lib.get(b"viFindRsrc\0").ok()?,
                find_next: *lib.get(b"viFindNext\0").ok()?,
                open: *lib.get(b"viOpen\0").ok()?,
                close: *lib.get(b"viClose\0").ok()?,
                read: *lib.get(b"viRead\0").ok()?,
                write: *lib.get(b"viWrite\0").ok()?,
                set_attribute: *lib.get(b"viSetAttribute\0").ok()?,
                status_desc: *lib.get(b"viStatusDesc\0").ok()?,
                _lib: lib,
            };
            let mut rm = 0;
            (open_default_rm(&mut rm) >= 0).then_some(Api { rm, ..api })
        }
    })
}

fn api() -> Option<&'static Api> {
    static API: OnceLock<Option<Api>> = OnceLock::new();
    API.get_or_init(load).as_ref()
}

pub fn available() -> bool {
    api().is_some()
}

fn error(a: &Api, vi: ViSession, status: ViStatus, what: &str) -> io::Error {
    let mut desc = [0 as c_char; 256];
    // SAFETY: viStatusDesc writes at most 256 bytes including the terminator.
    unsafe { (a.status_desc)(vi, status, desc.as_mut_ptr()) };
    // SAFETY: NUL-terminated by viStatusDesc (zero-initialised buffer otherwise).
    let text = unsafe { CStr::from_ptr(desc.as_ptr()) }.to_string_lossy().to_string();
    io::Error::other(format!("VISA {what}: {text} (0x{:08X})", status as u32))
}

/// USB instrument resources (`USB?*INSTR`) the VISA sees.
pub fn resources() -> Vec<String> {
    let Some(a) = api() else { return Vec::new() };
    let expr = CString::new("USB?*INSTR").unwrap();
    let (mut list, mut count) = (0u32, 0u32);
    let mut desc = [0 as c_char; 256];
    // SAFETY: desc holds VI_FIND_BUFLEN (256) bytes; list/count are valid out-pointers.
    let st = unsafe { (a.find_rsrc)(a.rm, expr.as_ptr(), &mut list, &mut count, desc.as_mut_ptr()) };
    if st < 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    for i in 0..count {
        // SAFETY: NUL-terminated by viFindRsrc / viFindNext.
        out.push(unsafe { CStr::from_ptr(desc.as_ptr()) }.to_string_lossy().to_string());
        // SAFETY: as above.
        if i + 1 < count && unsafe { (a.find_next)(list, desc.as_mut_ptr()) } < 0 {
            break;
        }
    }
    // SAFETY: closing the find list returned above.
    unsafe { (a.close)(list) };
    out
}

/// (vid, pid, serial) of a USB instrument.
pub type UsbId = (u16, u16, Option<String>);

/// [`UsbId`] of a USB resource string; VISA allows hex (`0x1AB1`) or decimal ids.
pub fn parse_usb_resource(r: &str) -> Option<UsbId> {
    let f: Vec<&str> = r.split("::").collect();
    if !f.first()?.to_ascii_uppercase().starts_with("USB") || f.len() < 4 {
        return None;
    }
    let num = |s: &str| match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u16::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    };
    let serial = Some(f[3].to_string()).filter(|s| !s.is_empty() && !s.eq_ignore_ascii_case("INSTR"));
    Some((num(f[1])?, num(f[2])?, serial))
}

pub fn is_resource(path: &str) -> bool {
    path.contains("::")
}

pub struct VisaTransport {
    vi: ViSession,
    timeout_ms: u32,
    buf: Vec<u8>,
}

impl VisaTransport {
    pub fn open(resource: &str) -> io::Result<Self> {
        let a = api().ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "no VISA library installed"))?;
        let name = CString::new(resource).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "bad resource string"))?;
        let mut vi = 0;
        // SAFETY: valid resource manager, NUL-terminated name, valid out-pointer. Mode 0 = no lock.
        let st = unsafe { (a.open)(a.rm, name.as_ptr(), 0, 2000, &mut vi) };
        if st < 0 {
            return Err(error(a, a.rm, st, &format!("open {resource}")));
        }
        // SAFETY: valid session. Messages end with USBTMC EOM (END), never on a term char.
        unsafe {
            (a.set_attribute)(vi, VI_ATTR_TERMCHAR_EN, 0);
            (a.set_attribute)(vi, VI_ATTR_SEND_END_EN, 1);
        }
        Ok(VisaTransport { vi, timeout_ms: 0, buf: vec![0; 64 * 1024] })
    }
}

impl Drop for VisaTransport {
    fn drop(&mut self) {
        if let Some(a) = api() {
            // SAFETY: closing our own session once.
            unsafe { (a.close)(self.vi) };
        }
    }
}

impl Transport for VisaTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let a = api().ok_or_else(|| io::Error::other("VISA unloaded"))?;
        let mut done = 0usize;
        while done < data.len() {
            let mut n = 0u32;
            let chunk = &data[done..];
            // SAFETY: pointer/length describe `chunk`.
            let st = unsafe { (a.write)(self.vi, chunk.as_ptr(), chunk.len() as u32, &mut n) };
            if st < 0 {
                return Err(error(a, self.vi, st, "write"));
            }
            done += n as usize;
        }
        Ok(())
    }

    /// One whole message (until END); empty = nothing before the timeout.
    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        let a = api().ok_or_else(|| io::Error::other("VISA unloaded"))?;
        let ms = (timeout.as_millis() as u32).max(1);
        if ms != self.timeout_ms {
            // SAFETY: valid session.
            unsafe { (a.set_attribute)(self.vi, VI_ATTR_TMO_VALUE, ms as ViAttrState) };
            self.timeout_ms = ms;
        }
        let mut message = Vec::new();
        loop {
            let mut n = 0u32;
            // SAFETY: pointer/length describe `self.buf`.
            let st = unsafe { (a.read)(self.vi, self.buf.as_mut_ptr(), self.buf.len() as u32, &mut n) };
            message.extend_from_slice(&self.buf[..n as usize]);
            match st {
                VI_SUCCESS_MAX_CNT => continue, // buffer full, message goes on
                VI_ERROR_TMO => return Ok(message),
                s if s < 0 => return Err(error(a, self.vi, s, "read")),
                VI_SUCCESS_TERM_CHAR => return Ok(message),
                _ => return Ok(message), // VI_SUCCESS: END (USBTMC EOM)
            }
        }
    }

    fn reads_on_request(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usb_resource_strings() {
        assert_eq!(parse_usb_resource("USB0::0x1AB1::0x044D::DHO8A272907001::INSTR"), Some((0x1ab1, 0x044d, Some("DHO8A272907001".into()))));
        assert_eq!(parse_usb_resource("USB0::6833::1101::DHO8A272907001::0::INSTR"), Some((0x1ab1, 0x044d, Some("DHO8A272907001".into()))));
        assert_eq!(parse_usb_resource("usb1::0x5345::0x1235::25130086::INSTR").map(|r| r.0), Some(0x5345));
        assert_eq!(parse_usb_resource("TCPIP0::192.168.1.5::INSTR"), None);
        assert!(is_resource("USB0::0x1AB1::0x044D::X::INSTR") && !is_resource("/dev/usbtmc2") && !is_resource("usb:1ab1:044d:X"));
    }
}
