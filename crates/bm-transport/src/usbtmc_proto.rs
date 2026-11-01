//! USBTMC protocol (USB Test & Measurement Class 1.0, bulk messages) over any bulk pipe -
//! the userspace USBTMC transports (nusb on Windows / macOS) speak it themselves instead of a
//! kernel driver.
//!
//! One [`Transport::read`] returns one whole device message, like the Linux kernel transport.
//! Replies carry the tag of the request they answer; a reply with an older tag (a late answer
//! to a request that timed out) is discarded instead of being taken for the current one - the
//! failure mode that left the DHO804 "one reply behind" on the kernel driver.

use std::io;
use std::time::{Duration, Instant};

use bm_driver::engine::Transport;

const DEV_DEP_MSG_OUT: u8 = 1;
const REQUEST_DEV_DEP_MSG_IN: u8 = 2;
const DEV_DEP_MSG_IN: u8 = 2;
const HEADER: usize = 12;
/// Largest message we ask for (a DGE screenshot is ~392 KB).
pub const MAX_MESSAGE: u32 = 1 << 20;

/// Bulk endpoints of one USBTMC interface.
pub trait Pipe: Send {
    fn bulk_out(&mut self, data: &[u8], timeout: Duration) -> io::Result<()>;
    /// One bulk-IN transfer of up to `max` bytes; `Ok(None)` = nothing before the timeout
    /// (the pending transfer is cancelled).
    fn bulk_in(&mut self, max: usize, timeout: Duration) -> io::Result<Option<Vec<u8>>>;
    /// After a timeout: tell the device to drop the reply to `tag` (INITIATE_ABORT_BULK_IN +
    /// CHECK_ABORT_BULK_IN_STATUS). Best effort.
    fn abort_in(&mut self, tag: u8);
    /// After a bulk-OUT transfer (`tag`) failed: abort it, clear halted endpoints, device clear.
    /// Best effort; returns how each step went, for the wire log.
    fn recover(&mut self, tag: u8) -> String;
}

/// Bulk-OUT timeout of one message.
const OUT_TIMEOUT: Duration = Duration::from_secs(2);

/// The error of a write that failed again after a USB recovery: what the user can still do.
pub fn not_recovered(e: io::Error) -> io::Error {
    io::Error::new(
        e.kind(),
        format!("the instrument stopped answering on USB and did not recover ({e}); unplug and replug its USB cable, or switch it off and on"),
    )
}

/// bTag: 1..=255, never 0.
fn next_tag(t: &mut u8) -> u8 {
    *t = if *t == 255 { 1 } else { *t + 1 };
    *t
}

fn header(msg_id: u8, tag: u8, size: u32, attributes: u8) -> [u8; HEADER] {
    let s = size.to_le_bytes();
    [msg_id, tag, !tag, 0, s[0], s[1], s[2], s[3], attributes, 0, 0, 0]
}

/// DEV_DEP_MSG_OUT carrying a whole message (EOM set), padded to a multiple of 4 bytes.
pub fn msg_out(tag: u8, data: &[u8]) -> Vec<u8> {
    let mut v = header(DEV_DEP_MSG_OUT, tag, data.len() as u32, 0x01).to_vec();
    v.extend_from_slice(data);
    v.resize(v.len().div_ceil(4) * 4, 0);
    v
}

/// REQUEST_DEV_DEP_MSG_IN: the device may send up to `max` bytes (no term char).
pub fn request_in(tag: u8, max: u32) -> Vec<u8> {
    header(REQUEST_DEV_DEP_MSG_IN, tag, max, 0).to_vec()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InHeader {
    pub tag: u8,
    pub size: usize,
    pub eom: bool,
}

pub fn parse_in(h: &[u8]) -> io::Result<InHeader> {
    let bad = |m: &str| io::Error::new(io::ErrorKind::InvalidData, format!("USBTMC: {m}"));
    if h.len() < HEADER {
        return Err(bad("short reply header"));
    }
    if h[0] != DEV_DEP_MSG_IN {
        return Err(bad(&format!("unexpected MsgID {}", h[0])));
    }
    if h[1] != !h[2] {
        return Err(bad("corrupt bTag"));
    }
    Ok(InHeader { tag: h[1], size: u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize, eom: h[8] & 0x01 != 0 })
}

pub struct Session<P: Pipe> {
    pipe: P,
    tag: u8,
    /// Bulk-IN transfer size (a multiple of the endpoint's max packet size).
    chunk: usize,
    notes: Vec<String>,
}

impl<P: Pipe> Session<P> {
    pub fn new(pipe: P, max_packet: usize) -> Self {
        let mp = max_packet.max(1);
        Session { pipe, tag: 0, chunk: (64 * 1024 / mp).max(1) * mp, notes: Vec::new() }
    }

    /// One DEV_DEP_MSG_IN transfer (header + payload, possibly over several bulk transfers).
    fn read_transfer(&mut self, deadline: Instant) -> io::Result<Option<(InHeader, Vec<u8>)>> {
        let mut buf = Vec::new();
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let Some(part) = self.pipe.bulk_in(self.chunk, left)? else { return Ok(None) };
            let short = part.len() < self.chunk;
            buf.extend_from_slice(&part);
            if buf.len() >= HEADER {
                let h = parse_in(&buf)?;
                if buf.len() >= HEADER + h.size || short {
                    if buf.len() < HEADER + h.size {
                        return Err(io::Error::new(io::ErrorKind::InvalidData, "USBTMC: reply shorter than its header says"));
                    }
                    let payload = buf[HEADER..HEADER + h.size].to_vec();
                    return Ok(Some((h, payload)));
                }
            } else if short && !buf.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidData, "USBTMC: short reply header"));
            }
        }
    }
}

impl<P: Pipe> Transport for Session<P> {
    /// A message the device does not take gets one USB recovery and is sent once more.
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let tag = next_tag(&mut self.tag);
        let e = match self.pipe.bulk_out(&msg_out(tag, data), OUT_TIMEOUT) {
            Ok(()) => return Ok(()),
            Err(e) => e,
        };
        let done = self.pipe.recover(tag);
        self.notes.push(format!("write failed ({e}), USB recovery: {done}"));
        let tag = next_tag(&mut self.tag);
        self.pipe.bulk_out(&msg_out(tag, data), OUT_TIMEOUT).map_err(not_recovered)?;
        self.notes.push("recovered: the instrument takes commands again".into());
        Ok(())
    }

    /// One whole message; empty = nothing before the timeout.
    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        let deadline = Instant::now() + timeout;
        let mut message = Vec::new();
        loop {
            let tag = next_tag(&mut self.tag);
            self.pipe.bulk_out(&request_in(tag, MAX_MESSAGE), OUT_TIMEOUT)?;
            loop {
                match self.read_transfer(deadline)? {
                    None => {
                        self.pipe.abort_in(tag);
                        return Ok(Vec::new());
                    }
                    // A late reply to an earlier request: drop it, ours is still coming.
                    Some((h, _)) if h.tag != tag => continue,
                    Some((h, payload)) => {
                        message.extend_from_slice(&payload);
                        if h.eom {
                            return Ok(message);
                        }
                        break; // more of this message: request the next part
                    }
                }
            }
        }
    }

    fn take_notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn reads_on_request(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    #[test]
    fn message_layouts_match_the_spec() {
        assert_eq!(msg_out(1, b"*IDN?\n"), [1, 1, 0xFE, 0, 6, 0, 0, 0, 1, 0, 0, 0, b'*', b'I', b'D', b'N', b'?', b'\n', 0, 0]);
        assert_eq!(request_in(255, 0x0010_0000), [2, 255, 0, 0, 0, 0, 0x10, 0, 0, 0, 0, 0]);
        let h = parse_in(&[2, 7, 0xF8, 0, 3, 0, 0, 0, 1, 0, 0, 0]).unwrap();
        assert_eq!(h, InHeader { tag: 7, size: 3, eom: true });
        assert!(parse_in(&[2, 7, 7, 0, 3, 0, 0, 0, 1, 0, 0, 0]).is_err(), "bTag check");
        assert!(parse_in(&[1, 7, 0xF8, 0, 3, 0, 0, 0, 1, 0, 0, 0]).is_err(), "MsgID check");
    }

    #[test]
    fn tags_skip_zero() {
        let mut t = 254;
        assert_eq!([next_tag(&mut t), next_tag(&mut t), next_tag(&mut t)], [255, 1, 2]);
    }

    /// A simulated USBTMC instrument behind the pipe.
    #[derive(Default)]
    struct Device {
        /// Bulk-IN transfers queued for the host.
        pending_in: VecDeque<Vec<u8>>,
        /// Replies for the next queries (payload, split into parts of this size = no EOM).
        answers: VecDeque<Vec<u8>>,
        split: Option<usize>,
        last_query: Option<Vec<u8>>,
        requests: Vec<u8>,
        aborts: Vec<u8>,
        /// Answer the next request this many requests late (a reply that missed its read).
        silent_requests: usize,
        /// Bulk-OUT transfers that fail (a device that stopped taking data), and recoveries.
        failing_out: usize,
        recoveries: Vec<u8>,
        max_packet: usize,
    }

    fn reply(tag: u8, payload: &[u8], eom: bool) -> Vec<u8> {
        let mut v = header(DEV_DEP_MSG_IN, tag, payload.len() as u32, eom as u8).to_vec();
        v.extend_from_slice(payload);
        v.resize(v.len().div_ceil(4) * 4, 0);
        v
    }

    impl Device {
        fn queue(&mut self, transfer: Vec<u8>) {
            // the host controller delivers it in max-packet-sized pieces; a transfer ends with a short packet
            for c in transfer.chunks(self.max_packet) {
                self.pending_in.push_back(c.to_vec());
            }
            if transfer.len() % self.max_packet == 0 {
                self.pending_in.push_back(Vec::new());
            }
        }
    }

    struct SimPipe(std::sync::Arc<std::sync::Mutex<Device>>);

    impl Pipe for SimPipe {
        fn bulk_out(&mut self, data: &[u8], _: Duration) -> io::Result<()> {
            let mut d = self.0.lock().unwrap();
            if d.failing_out > 0 {
                d.failing_out -= 1;
                return Err(io::Error::new(io::ErrorKind::TimedOut, "USBTMC write: timeout"));
            }
            let h = &data[..HEADER];
            assert_eq!(h[1], !h[2]);
            let size = u32::from_le_bytes([h[4], h[5], h[6], h[7]]) as usize;
            match h[0] {
                DEV_DEP_MSG_OUT => d.last_query = Some(data[HEADER..HEADER + size].to_vec()),
                REQUEST_DEV_DEP_MSG_IN => {
                    d.requests.push(h[1]);
                    if d.silent_requests > 0 {
                        d.silent_requests -= 1;
                        return Ok(());
                    }
                    let Some(answer) = d.answers.pop_front() else { return Ok(()) };
                    match d.split {
                        Some(n) if answer.len() > n => {
                            let (first, rest) = answer.split_at(n);
                            let first = reply(h[1], first, false);
                            d.queue(first);
                            d.answers.push_front(rest.to_vec());
                        }
                        _ => {
                            let r = reply(h[1], &answer, true);
                            d.queue(r);
                        }
                    }
                }
                id => panic!("unexpected MsgID {id}"),
            }
            Ok(())
        }

        fn bulk_in(&mut self, max: usize, _: Duration) -> io::Result<Option<Vec<u8>>> {
            let mut d = self.0.lock().unwrap();
            let mut out = Vec::new();
            while out.len() < max {
                match d.pending_in.pop_front() {
                    Some(p) => {
                        let short = p.len() < d.max_packet;
                        out.extend_from_slice(&p);
                        if short {
                            break;
                        }
                    }
                    None if out.is_empty() => return Ok(None),
                    None => break,
                }
            }
            Ok(Some(out))
        }

        fn abort_in(&mut self, tag: u8) {
            self.0.lock().unwrap().aborts.push(tag);
        }

        fn recover(&mut self, tag: u8) -> String {
            self.0.lock().unwrap().recoveries.push(tag);
            "abort bulk-out ok".into()
        }
    }

    fn session(dev: Device) -> (Session<SimPipe>, std::sync::Arc<std::sync::Mutex<Device>>) {
        let mp = dev.max_packet;
        let d = std::sync::Arc::new(std::sync::Mutex::new(dev));
        (Session::new(SimPipe(d.clone()), mp), d)
    }

    const T: Duration = Duration::from_millis(100);

    #[test]
    fn query_and_reply() {
        let (mut s, d) = session(Device { max_packet: 512, answers: ["RIGOL TECHNOLOGIES,DHO804,DHO8A272907001,00.01.03\n".into()].into(), ..Default::default() });
        s.write(b"*IDN?\n").unwrap();
        assert_eq!(s.read(T).unwrap(), b"RIGOL TECHNOLOGIES,DHO804,DHO8A272907001,00.01.03\n");
        assert_eq!(d.lock().unwrap().last_query.as_deref(), Some(&b"*IDN?\n"[..]));
    }

    #[test]
    fn large_block_over_many_packets_and_exact_packet_multiples() {
        let block: Vec<u8> = (0..1012u32).map(|i| i as u8).collect(); // 12 + 1012 = 1024 = 2 packets exactly
        let big: Vec<u8> = (0..392_000u32).map(|i| (i * 7) as u8).collect();
        let (mut s, _) = session(Device { max_packet: 512, answers: [block.clone(), big.clone()].into(), ..Default::default() });
        assert_eq!(s.read(T).unwrap(), block);
        assert_eq!(s.read(T).unwrap(), big);
    }

    #[test]
    fn message_in_several_transfers_without_eom_is_reassembled() {
        let (mut s, d) = session(Device { max_packet: 64, split: Some(100), answers: [vec![b'x'; 250]].into(), ..Default::default() });
        assert_eq!(s.read(T).unwrap(), vec![b'x'; 250]);
        assert_eq!(d.lock().unwrap().requests.len(), 3, "one request per part");
    }

    #[test]
    fn a_late_reply_is_discarded_not_taken_for_the_next_one() {
        // The first query's reply arrives after the host gave up (request 1 answered late).
        let (mut s, d) = session(Device { max_packet: 512, silent_requests: 1, answers: ["late\n".into(), "fresh\n".into()].into(), ..Default::default() });
        s.write(b"Q1?\n").unwrap();
        assert_eq!(s.read(T).unwrap(), b"", "timeout");
        assert_eq!(d.lock().unwrap().aborts.len(), 1, "abort sent for the timed-out request");
        {
            // the device now answers the old request after all, then the new one
            let mut dev = d.lock().unwrap();
            let old = dev.requests[0];
            let late = reply(old, b"late\n", true);
            dev.queue(late);
            dev.answers.pop_front();
        }
        s.write(b"Q2?\n").unwrap();
        assert_eq!(s.read(T).unwrap(), b"fresh\n");
    }

    #[test]
    fn a_write_the_device_does_not_take_is_recovered_and_sent_again() {
        let (mut s, d) = session(Device { max_packet: 512, failing_out: 1, answers: ["ok\n".into()].into(), ..Default::default() });
        s.write(b"*IDN?\n").unwrap();
        assert_eq!(d.lock().unwrap().recoveries, [1], "recovery for the failed transfer's tag");
        assert_eq!(s.take_notes(), ["write failed (USBTMC write: timeout), USB recovery: abort bulk-out ok", "recovered: the instrument takes commands again"]);
        assert_eq!(s.read(T).unwrap(), b"ok\n");
        assert!(s.take_notes().is_empty());
    }

    #[test]
    fn a_device_that_does_not_recover_says_what_to_do() {
        let (mut s, _) = session(Device { max_packet: 512, failing_out: 2, ..Default::default() });
        let e = s.write(b"*IDN?\n").unwrap_err();
        assert!(e.to_string().contains("unplug and replug"), "{e}");
    }

    #[test]
    fn nothing_to_read_times_out_empty() {
        let (mut s, _) = session(Device { max_packet: 512, ..Default::default() });
        assert_eq!(s.read(T).unwrap(), b"");
    }
}
