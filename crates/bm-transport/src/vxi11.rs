//! VXI-11 (LXI) client: SCPI over ONC RPC for LAN instruments without a raw socket.
//!
//! The portmapper (TCP 111, program 100000 v2, GETPORT) gives the core channel's port; the core
//! channel (program 0x0607AF v1) opens a link to a logical device (`inst0`), then `device_write`
//! sends a message (END flag on its last chunk) and `device_read` returns one (until the END
//! reason). Like USBTMC, one [`Transport::read`] returns one whole device message, so definitions
//! use `rx.mode: message`. XDR: big-endian 32-bit words, opaque data padded to 4 bytes; RPC over
//! TCP with record marking (4-byte header, top bit = last fragment).

use std::io::{self, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

use bm_driver::engine::Transport;

const PORTMAPPER: u16 = 111;
const PMAP_PROG: u32 = 100_000;
const PMAP_VERS: u32 = 2;
const PMAP_GETPORT: u32 = 3;
const CORE_PROG: u32 = 0x0607AF;
const CORE_VERS: u32 = 1;
const CREATE_LINK: u32 = 10;
const DEVICE_WRITE: u32 = 11;
const DEVICE_READ: u32 = 12;
const DESTROY_LINK: u32 = 23;
const FLAG_END: u32 = 0x08;
const REASON_END: u32 = 0x04;
const ERR_IO_TIMEOUT: u32 = 15;
const CONNECT: Duration = Duration::from_secs(3);
/// How long a write may wait inside the instrument (its input buffer full).
const IO_TIMEOUT_MS: u32 = 5000;
/// Largest message chunk we ask the instrument for in one `device_read`.
const READ_CHUNK: u32 = 1 << 20;

/// XDR encoder.
#[derive(Default)]
struct Xdr(Vec<u8>);

impl Xdr {
    fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn opaque(mut self, data: &[u8]) -> Self {
        self = self.u32(data.len() as u32);
        self.0.extend_from_slice(data);
        self.0.resize(self.0.len().next_multiple_of(4), 0);
        self
    }
}

/// XDR decoder over a reply body.
struct Rd<'a>(&'a [u8]);

impl Rd<'_> {
    fn u32(&mut self) -> io::Result<u32> {
        if self.0.len() < 4 {
            return Err(bad("short RPC reply"));
        }
        let (h, t) = self.0.split_at(4);
        self.0 = t;
        Ok(u32::from_be_bytes([h[0], h[1], h[2], h[3]]))
    }
    fn opaque(&mut self) -> io::Result<Vec<u8>> {
        let n = self.u32()? as usize;
        let padded = n.next_multiple_of(4);
        if self.0.len() < padded {
            return Err(bad("short RPC opaque"));
        }
        let data = self.0[..n].to_vec();
        self.0 = &self.0[padded..];
        Ok(data)
    }
}

fn bad(m: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, m.to_string())
}

/// One ONC RPC connection (TCP, record marking).
struct Rpc {
    stream: TcpStream,
    xid: u32,
}

impl Rpc {
    fn connect(addr: &str) -> io::Result<Rpc> {
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{addr}: no address"));
        for a in addr.to_socket_addrs()? {
            match TcpStream::connect_timeout(&a, CONNECT) {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    return Ok(Rpc { stream, xid: 1 });
                }
                Err(e) => last = crate::access::explained(&e, crate::access::Port::Network, addr, crate::access::Platform::current())
                    .unwrap_or_else(|| io::Error::new(e.kind(), format!("{addr}: {e}"))),
            }
        }
        Err(last)
    }

    /// Call `proc` and return the result body. `wait`: how long the reply may take.
    fn call(&mut self, prog: u32, vers: u32, proc_: u32, args: &[u8], wait: Duration) -> io::Result<Vec<u8>> {
        self.xid = self.xid.wrapping_add(1);
        let xid = self.xid;
        // xid, CALL, rpcvers 2, prog, vers, proc, cred AUTH_NONE, verf AUTH_NONE, args
        let mut msg = Xdr::default().u32(xid).u32(0).u32(2).u32(prog).u32(vers).u32(proc_).u32(0).u32(0).u32(0).u32(0).0;
        msg.extend_from_slice(args);
        let mut rec = (0x8000_0000u32 | msg.len() as u32).to_be_bytes().to_vec();
        rec.extend(msg);
        self.stream.write_all(&rec)?;
        self.stream.set_read_timeout(Some(wait))?;
        loop {
            let reply = self.record()?;
            let mut r = Rd(&reply);
            if r.u32()? != xid {
                continue; // a late reply to an earlier (timed-out) call
            }
            if r.u32()? != 1 || r.u32()? != 0 {
                return Err(bad("RPC call rejected"));
            }
            let _verf_flavor = r.u32()?;
            let _verf = r.opaque()?;
            let accept = r.u32()?;
            if accept != 0 {
                return Err(bad(&format!("RPC call not accepted (status {accept})")));
            }
            return Ok(r.0.to_vec());
        }
    }

    /// One RPC record (all its fragments).
    fn record(&mut self) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        loop {
            let mut h = [0u8; 4];
            self.stream.read_exact(&mut h)?;
            let h = u32::from_be_bytes(h);
            let mut frag = vec![0u8; (h & 0x7FFF_FFFF) as usize];
            self.stream.read_exact(&mut frag)?;
            out.extend(frag);
            if h & 0x8000_0000 != 0 {
                return Ok(out);
            }
        }
    }
}

pub struct Vxi11Transport {
    rpc: Rpc,
    lid: u32,
    max_recv: usize,
}

/// `host` or `host:port` → (host, port of the portmapper).
fn split_host(path: &str) -> io::Result<(String, u16)> {
    let p = path.trim();
    if p.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "VXI-11 instrument needs a host"));
    }
    match p.rsplit_once(':') {
        Some((h, port)) if !h.contains(':') || h.ends_with(']') => {
            let port = port.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, format!("{p}: bad port")))?;
            Ok((h.trim_matches(['[', ']']).to_string(), port))
        }
        _ => Ok((p.trim_matches(['[', ']']).to_string(), PORTMAPPER)),
    }
}

/// `host[:port]` → the portmapper's `host:port` (what a presence check connects to).
pub fn portmapper_target(path: &str) -> io::Result<String> {
    split_host(path).map(|(h, p)| join(&h, p))
}

fn join(host: &str, port: u16) -> String {
    if host.contains(':') { format!("[{host}]:{port}") } else { format!("{host}:{port}") }
}

impl Vxi11Transport {
    /// `path`: `host` (portmapper on 111) or `host:port` (another portmapper port - tests,
    /// port forwarding); `device`: the logical device, usually `inst0`.
    pub fn open(path: &str, device: &str) -> io::Result<Self> {
        let (host, pmap_port) = split_host(path)?;
        let mut pmap = Rpc::connect(&join(&host, pmap_port))?;
        let args = Xdr::default().u32(CORE_PROG).u32(CORE_VERS).u32(6).u32(0).0;
        let reply = pmap.call(PMAP_PROG, PMAP_VERS, PMAP_GETPORT, &args, CONNECT)?;
        let port = Rd(&reply).u32()?;
        if port == 0 || port > u16::MAX as u32 {
            return Err(io::Error::new(io::ErrorKind::NotFound, format!("{host}: no VXI-11 core service registered")));
        }
        let mut rpc = Rpc::connect(&join(&host, port as u16))?;
        // clientId, lockDevice false, lock_timeout, device name
        let args = Xdr::default().u32(0x4243_4d48).u32(0).u32(0).opaque(device.as_bytes()).0;
        let reply = rpc.call(CORE_PROG, CORE_VERS, CREATE_LINK, &args, CONNECT)?;
        let mut r = Rd(&reply);
        let error = r.u32()?;
        if error != 0 {
            return Err(io::Error::other(format!("{host}: VXI-11 create_link to {device:?} failed (error {error})")));
        }
        let lid = r.u32()?;
        let _abort_port = r.u32()?;
        let max_recv = r.u32()?.clamp(256, 1 << 24) as usize;
        Ok(Vxi11Transport { rpc, lid, max_recv })
    }
}

impl Transport for Vxi11Transport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        let chunks: Vec<&[u8]> = if data.is_empty() { vec![data] } else { data.chunks(self.max_recv).collect() };
        let last = chunks.len() - 1;
        for (i, c) in chunks.into_iter().enumerate() {
            let flags = if i == last { FLAG_END } else { 0 };
            let args = Xdr::default().u32(self.lid).u32(IO_TIMEOUT_MS).u32(0).u32(flags).opaque(c).0;
            let wait = Duration::from_millis(IO_TIMEOUT_MS as u64) + CONNECT;
            let reply = self.rpc.call(CORE_PROG, CORE_VERS, DEVICE_WRITE, &args, wait)?;
            let error = Rd(&reply).u32()?;
            if error != 0 {
                return Err(io::Error::other(format!("VXI-11 device_write failed (error {error})")));
            }
        }
        Ok(())
    }

    /// One whole device message, or nothing within `timeout` (VXI-11 I/O timeout).
    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        let io_ms = timeout.as_millis().clamp(1, u32::MAX as u128) as u32;
        let mut out = Vec::new();
        loop {
            let args = Xdr::default().u32(self.lid).u32(READ_CHUNK).u32(io_ms).u32(0).u32(0).u32(0).0;
            let reply = self.rpc.call(CORE_PROG, CORE_VERS, DEVICE_READ, &args, timeout + CONNECT)?;
            let mut r = Rd(&reply);
            let error = r.u32()?;
            let reason = r.u32()?;
            let data = r.opaque()?;
            match error {
                0 => {}
                ERR_IO_TIMEOUT if out.is_empty() => return Ok(Vec::new()),
                e => return Err(io::Error::other(format!("VXI-11 device_read failed (error {e})"))),
            }
            out.extend(data);
            if reason & REASON_END != 0 {
                return Ok(out);
            }
        }
    }

    fn reads_on_request(&self) -> bool {
        true
    }
}

impl Drop for Vxi11Transport {
    fn drop(&mut self) {
        let args = Xdr::default().u32(self.lid).0;
        let _ = self.rpc.call(CORE_PROG, CORE_VERS, DESTROY_LINK, &args, Duration::from_millis(500));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::{Arc, Mutex};

    /// A minimal VXI-11 instrument: portmapper + core channel; answers `*IDN?`, splits its reply
    /// into two `device_read` chunks (only the second carries END) and times out when idle.
    fn instrument() -> (u16, Arc<Mutex<Vec<String>>>) {
        let core = TcpListener::bind("127.0.0.1:0").unwrap();
        let core_port = core.local_addr().unwrap().port();
        let pmap = TcpListener::bind("127.0.0.1:0").unwrap();
        let pmap_port = pmap.local_addr().unwrap().port();
        let seen = Arc::new(Mutex::new(Vec::new()));
        fn serve(listener: TcpListener, mut handle: impl FnMut(u32, &[u8]) -> Vec<u8> + Send + 'static) {
            std::thread::spawn(move || {
                let (mut s, _) = listener.accept().unwrap();
                loop {
                    let mut h = [0u8; 4];
                    if s.read_exact(&mut h).is_err() {
                        return;
                    }
                    let mut msg = vec![0u8; (u32::from_be_bytes(h) & 0x7FFF_FFFF) as usize];
                    s.read_exact(&mut msg).unwrap();
                    let mut r = Rd(&msg);
                    let xid = r.u32().unwrap();
                    for _ in 0..4 {
                        r.u32().unwrap(); // CALL, rpcvers, prog, vers
                    }
                    let proc_ = r.u32().unwrap();
                    for _ in 0..4 {
                        r.u32().unwrap(); // cred, verf
                    }
                    let body = handle(proc_, r.0);
                    let mut reply = Xdr::default().u32(xid).u32(1).u32(0).u32(0).u32(0).u32(0).0;
                    reply.extend(body);
                    let mut rec = (0x8000_0000u32 | reply.len() as u32).to_be_bytes().to_vec();
                    rec.extend(reply);
                    s.write_all(&rec).unwrap();
                }
            });
        }
        serve(pmap, move |_, _| Xdr::default().u32(core_port as u32).0);
        let log = seen.clone();
        let mut pending: Vec<Vec<u8>> = Vec::new();
        serve(core, move |proc_, args| {
            let mut r = Rd(args);
            match proc_ {
                CREATE_LINK => {
                    let _ = (r.u32(), r.u32(), r.u32());
                    log.lock().unwrap().push(format!("link {}", String::from_utf8(r.opaque().unwrap()).unwrap()));
                    Xdr::default().u32(0).u32(7).u32(0).u32(1024).0
                }
                DEVICE_WRITE => {
                    let _ = (r.u32(), r.u32(), r.u32(), r.u32());
                    let data = r.opaque().unwrap();
                    log.lock().unwrap().push(String::from_utf8_lossy(&data).to_string());
                    if data.starts_with(b"*IDN?") {
                        pending = vec![b"RIGOL TECHNOLOGIES,DM3068,".to_vec(), b"DM3O000000001,01.01.00.01.11.00\n".to_vec()];
                    }
                    Xdr::default().u32(0).u32(data.len() as u32).0
                }
                DEVICE_READ => match pending.is_empty() {
                    true => Xdr::default().u32(ERR_IO_TIMEOUT).u32(0).opaque(b"").0,
                    false => {
                        let chunk = pending.remove(0);
                        let reason = if pending.is_empty() { REASON_END } else { 1 };
                        Xdr::default().u32(0).u32(reason).opaque(&chunk).0
                    }
                },
                DESTROY_LINK => {
                    log.lock().unwrap().push("unlink".into());
                    Xdr::default().u32(0).0
                }
                _ => Xdr::default().u32(8).0,
            }
        });
        (pmap_port, seen)
    }

    #[test]
    fn a_vxi11_exchange_returns_one_whole_message() {
        let (pmap_port, seen) = instrument();
        let mut t = Vxi11Transport::open(&format!("127.0.0.1:{pmap_port}"), "inst0").unwrap();
        assert!(t.read(Duration::from_millis(50)).unwrap().is_empty(), "nothing pending: an I/O timeout is silence");
        t.write(b"*IDN?\n").unwrap();
        let reply = t.read(Duration::from_millis(500)).unwrap();
        assert_eq!(reply, b"RIGOL TECHNOLOGIES,DM3068,DM3O000000001,01.01.00.01.11.00\n", "chunks joined up to END");
        drop(t);
        std::thread::sleep(Duration::from_millis(100));
        assert_eq!(*seen.lock().unwrap(), ["link inst0", "*IDN?\n", "unlink"]);
    }

    #[test]
    fn hosts_take_an_optional_portmapper_port() {
        assert_eq!(split_host("192.168.1.9").unwrap(), ("192.168.1.9".into(), 111));
        assert_eq!(split_host("dmm.lan:1111").unwrap(), ("dmm.lan".into(), 1111));
        assert_eq!(split_host("fe80::1").unwrap(), ("fe80::1".into(), 111));
        assert_eq!(split_host("[fe80::1]:1111").unwrap(), ("fe80::1".into(), 1111));
        assert!(split_host(" ").is_err());
    }
}
