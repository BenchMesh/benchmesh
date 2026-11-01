//! Linux kernel USBTMC (`/dev/usbtmcN`).
//!
//! Each `read()` on the device node issues one USBTMC `REQUEST_DEV_DEP_MSG_IN`; the
//! buffer must be large enough for the whole message (the OWON DGE ignores the requested
//! transfer size and streams the full message). The per-call timeout is set with
//! `USBTMC_IOCTL_SET_TIMEOUT`.
//!
//! A device that stops taking commands (a write times out or stalls; seen on the DHO804 after a
//! read request timed out, 2026-10-21) gets one USB recovery before the write is given up:
//! abort the transfers both ways, clear halted endpoints, device clear - all control requests,
//! which still reach a device whose bulk endpoints hang. What happened goes to the wire log.

use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::fd::AsRawFd;
use std::time::Duration;

use bm_driver::engine::Transport;

const USBTMC_IOC_NR: u64 = 91;
// _IOW(USBTMC_IOC_NR, 10, __u32)
const USBTMC_IOCTL_SET_TIMEOUT: u64 = (1 << 30) | (4 << 16) | (USBTMC_IOC_NR << 8) | 10;
// _IO(USBTMC_IOC_NR, 2)
const USBTMC_IOCTL_CLEAR: u64 = (USBTMC_IOC_NR << 8) | 2;
// _IO(USBTMC_IOC_NR, 3 / 4 / 6 / 7)
const USBTMC_IOCTL_ABORT_BULK_OUT: u64 = (USBTMC_IOC_NR << 8) | 3;
const USBTMC_IOCTL_ABORT_BULK_IN: u64 = (USBTMC_IOC_NR << 8) | 4;
const USBTMC_IOCTL_CLEAR_OUT_HALT: u64 = (USBTMC_IOC_NR << 8) | 6;
const USBTMC_IOCTL_CLEAR_IN_HALT: u64 = (USBTMC_IOC_NR << 8) | 7;
/// The recovery, in order.
const RECOVERY: [(&str, u64); 5] = [
    ("abort bulk-out", USBTMC_IOCTL_ABORT_BULK_OUT),
    ("abort bulk-in", USBTMC_IOCTL_ABORT_BULK_IN),
    ("clear out halt", USBTMC_IOCTL_CLEAR_OUT_HALT),
    ("clear in halt", USBTMC_IOCTL_CLEAR_IN_HALT),
    ("device clear", USBTMC_IOCTL_CLEAR),
];
/// Kernel minimum (USBTMC_MIN_TIMEOUT).
const MIN_TIMEOUT_MS: u32 = 100;
/// Large enough for any message we expect (a DGE screenshot is ~392 KB).
const MAX_MESSAGE: usize = 1 << 20;

pub struct UsbTmcTransport {
    file: File,
    buf: Vec<u8>,
    timeout_ms: u32,
    notes: Vec<String>,
}

impl UsbTmcTransport {
    /// Open the device. No automatic device clear: on the DHO804 a clear sometimes recovers a
    /// device left one reply behind by an interrupted session, and sometimes leaves the next
    /// exchange unanswered (verified 2026-10-17). Interrupted sessions are avoided instead
    /// (graceful shutdown); proper abort/resync needs the userspace USBTMC transport.
    pub fn open(path: &str) -> io::Result<Self> {
        let file = OpenOptions::new().read(true).write(true).open(path).map_err(|e| crate::access::explain(e, crate::access::Port::Usb, path))?;
        Ok(UsbTmcTransport { file, buf: vec![0; MAX_MESSAGE], timeout_ms: 0, notes: Vec::new() })
    }

    fn set_timeout(&mut self, t: Duration) -> io::Result<()> {
        let ms = (t.as_millis() as u32).max(MIN_TIMEOUT_MS);
        if ms != self.timeout_ms {
            let v: u32 = ms;
            // SAFETY: valid fd; the ioctl reads one u32 through the pointer.
            let r = unsafe { libc::ioctl(self.file.as_raw_fd(), USBTMC_IOCTL_SET_TIMEOUT as _, &v as *const u32) };
            if r < 0 {
                return Err(io::Error::last_os_error());
            }
            self.timeout_ms = ms;
        }
        Ok(())
    }

    /// USBTMC device clear (resets the device's input/output buffers).
    pub fn clear(&mut self) -> io::Result<()> {
        self.request(USBTMC_IOCTL_CLEAR)
    }

    /// An argument-less USBTMC ioctl.
    fn request(&self, nr: u64) -> io::Result<()> {
        request_on(&self.file, nr)
    }
}

/// A write error a USB recovery may cure: the device stopped taking data (timeout) or halted
/// the endpoint (stall). A device that is gone (`ESHUTDOWN`, `ENODEV`) is not one.
fn stuck(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(libc::ETIMEDOUT) | Some(libc::EPIPE))
}

/// Run the recovery steps through `request`, every one even when an earlier one fails;
/// returns how each went, for the wire log.
fn recover(mut request: impl FnMut(u64) -> io::Result<()>) -> String {
    let done: Vec<String> = RECOVERY
        .iter()
        .map(|(name, nr)| match request(*nr) {
            Ok(()) => format!("{name} ok"),
            Err(e) => format!("{name} failed ({e})"),
        })
        .collect();
    done.join(", ")
}

/// `write` once more after a recovery when the first try left the device stuck.
fn write_recovering(
    notes: &mut Vec<String>,
    mut write: impl FnMut() -> io::Result<()>,
    request: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<()> {
    let e = match write() {
        Err(e) if stuck(&e) => e,
        r => return r,
    };
    notes.push(format!("write failed ({e}), USB recovery: {}", recover(request)));
    write().map_err(crate::usbtmc_proto::not_recovered)?;
    notes.push("recovered: the instrument takes commands again".into());
    Ok(())
}

impl Transport for UsbTmcTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let (file, notes) = (&self.file, &mut self.notes);
        write_recovering(notes, || (&*file).write_all(data), |nr| request_on(file, nr))
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        self.set_timeout(timeout)?;
        match self.file.read(&mut self.buf) {
            Ok(n) => Ok(self.buf[..n].to_vec()),
            Err(e) if e.raw_os_error() == Some(libc::ETIMEDOUT) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }

    fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn reads_on_request(&self) -> bool {
        true
    }
}

fn request_on(file: &File, nr: u64) -> io::Result<()> {
    // SAFETY: valid fd, argument-less ioctl.
    let r = unsafe { libc::ioctl(file.as_raw_fd(), nr as _) };
    if r < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ioctl_numbers_match_linux_headers() {
        // linux/usb/tmc.h: USBTMC_IOCTL_SET_TIMEOUT = 0x40045b0a, USBTMC_IOCTL_CLEAR = 0x5b02
        assert_eq!(USBTMC_IOCTL_SET_TIMEOUT, 0x4004_5b0a);
        assert_eq!(USBTMC_IOCTL_CLEAR, 0x5b02);
        // ABORT_BULK_OUT 3, ABORT_BULK_IN 4, CLEAR_OUT_HALT 6, CLEAR_IN_HALT 7
        let nrs: Vec<u64> = RECOVERY.iter().map(|r| r.1).collect();
        assert_eq!(nrs, [0x5b03, 0x5b04, 0x5b06, 0x5b07, 0x5b02]);
    }

    fn os(code: i32) -> io::Error {
        io::Error::from_raw_os_error(code)
    }

    #[test]
    fn a_stuck_write_is_recovered_and_sent_again() {
        let (mut notes, mut tries, mut requests) = (Vec::new(), 0, Vec::new());
        let r = write_recovering(
            &mut notes,
            || {
                tries += 1;
                if tries == 1 { Err(os(libc::ETIMEDOUT)) } else { Ok(()) }
            },
            |nr| {
                requests.push(nr);
                if nr == USBTMC_IOCTL_ABORT_BULK_IN { Err(os(libc::EPIPE)) } else { Ok(()) }
            },
        );
        assert!(r.is_ok());
        assert_eq!(tries, 2);
        assert_eq!(requests.len(), RECOVERY.len(), "every step runs, also after one failed");
        assert!(notes[0].starts_with("write failed (") && notes[0].contains("abort bulk-in failed") && notes[0].ends_with("device clear ok"), "{notes:?}");
        assert_eq!(notes[1], "recovered: the instrument takes commands again");
    }

    #[test]
    fn a_device_that_does_not_recover_says_what_to_do() {
        let mut notes = Vec::new();
        let e = write_recovering(&mut notes, || Err(os(libc::ETIMEDOUT)), |_| Ok(())).unwrap_err();
        assert!(e.to_string().contains("unplug and replug"), "{e}");
        assert_eq!(notes.len(), 1);
    }

    #[test]
    fn other_errors_and_successes_are_left_alone() {
        let mut notes = Vec::new();
        let mut requests = 0;
        assert!(write_recovering(&mut notes, || Ok(()), |_| { requests += 1; Ok(()) }).is_ok());
        let e = write_recovering(&mut notes, || Err(os(libc::ESHUTDOWN)), |_| { requests += 1; Ok(()) }).unwrap_err();
        assert_eq!(e.raw_os_error(), Some(libc::ESHUTDOWN), "a device that is gone is reported as it is");
        assert_eq!((requests, notes.len()), (0, 0));
    }
}
