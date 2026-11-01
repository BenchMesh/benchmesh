//! Physical transports for the BenchMesh engine.
//!
//! - [`SerialTransport`]: RS-232 / USB-serial via `serialport` (Linux, macOS, Windows).
//! - [`TcpTransport`]: raw SCPI socket; [`vxi11`]: VXI-11 (ONC RPC) for LXI instruments without one.
//! - USBTMC on Windows / macOS: userspace protocol ([`usbtmc_proto`]) over `nusb`
//!   ([`usbtmc_nusb`], WinUSB on Windows), or an installed VISA ([`visa`]). [`winusb`] gives a
//!   Windows instrument the WinUSB driver (one UAC prompt).
//! - [`UsbTmcTransport`]: Linux kernel `usbtmc` driver (`/dev/usbtmcN`); one `read()`
//!   returns one whole device message - never split a message into several reads
//!   (split reads strand data on the device and desync the USBTMC tags; see the DGE
//!   screenshot capture).
//!
//! Every open transport holds its port ([`lock`]); a configured instrument is opened on the
//! port its entry names ([`attempts`]). A parked instrument is only looked for ([`presence`]),
//! never opened.

use std::io::{self, Read, Write};
use std::time::Duration;

use bm_devicedef::{Flow, LineState, Link, ModemControl, SerialLink, TcpLink};
use bm_driver::actor::Attempt;
use bm_driver::engine::Transport;

pub mod access;
pub mod discover;
pub mod lock;
pub mod presence;
pub mod usbtmc_nusb;
pub mod usbtmc_proto;
pub mod visa;
pub mod vxi11;
pub mod winusb;
#[cfg(target_os = "linux")]
mod usbtmc;
#[cfg(target_os = "linux")]
pub use usbtmc::UsbTmcTransport;

/// Where a configured device is attached (from config.yaml).
#[derive(Debug, Clone, Default)]
pub struct Endpoint {
    /// Serial port path, USBTMC device node / `usb:` path / VISA resource, or TCP `host[:port]`.
    pub path: String,
    /// Baud rate override (else the definition's).
    pub baud: Option<u32>,
    /// Serial format override, e.g. `8N1`.
    pub format: Option<String>,
}

/// Open the transport a definition's `link` describes (holding the port until dropped).
pub fn open(link: &Link, ep: &Endpoint) -> io::Result<Box<dyn Transport>> {
    let guard = lock::acquire(&ep.path)?;
    let inner: Box<dyn Transport> = match link {
        Link::Serial(s) => Box::new(SerialTransport::open(s, ep)?),
        Link::Usbtmc(_) if visa::is_resource(&ep.path) => Box::new(visa::VisaTransport::open(&ep.path)?),
        Link::Usbtmc(_) if ep.path.starts_with(usbtmc_nusb::PREFIX) => Box::new(usbtmc_nusb::open(&ep.path)?),
        #[cfg(target_os = "linux")]
        Link::Usbtmc(_) => Box::new(UsbTmcTransport::open(&ep.path)?),
        #[cfg(not(target_os = "linux"))]
        Link::Usbtmc(_) => {
            let msg = format!("{}: not a USBTMC instrument path (use a `usb:` path from the port list or a VISA resource)", ep.path);
            return Err(io::Error::new(io::ErrorKind::InvalidInput, msg));
        }
        Link::Tcp(t) => Box::new(TcpTransport::open(t, ep)?),
        Link::Vxi11(v) => Box::new(vxi11::Vxi11Transport::open(&ep.path, &v.device)?),
    };
    Ok(Box::new(Held { inner, _guard: guard }))
}

/// A transport together with its port lock.
struct Held {
    inner: Box<dyn Transport>,
    _guard: lock::PortGuard,
}

impl Transport for Held {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.inner.write(data)
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        self.inner.read(timeout)
    }

    fn take_notes(&mut self) -> Vec<String> {
        self.inner.take_notes()
    }

    fn reads_on_request(&self) -> bool {
        self.inner.reads_on_request()
    }
}

/// The kind of port a definition's link is found on.
pub fn port_kind(link: &Link) -> Option<discover::PortKind> {
    match link {
        Link::Serial(_) => Some(discover::PortKind::Serial),
        Link::Usbtmc(_) => Some(discover::PortKind::Usbtmc),
        Link::Tcp(_) | Link::Vxi11(_) => None,
    }
}

/// The port to try for a configured instrument: the one its entry names.
pub fn attempts(link: &Link, ep: &Endpoint) -> Vec<Attempt> {
    let (link, ep2) = (link.clone(), ep.clone());
    vec![Attempt { port: ep.path.clone(), open: Box::new(move || open(&link, &ep2)), accept: None }]
}

/// Raw socket to a LAN instrument (SCPI over TCP, usually port 5025 or 5555): a byte stream
/// like a serial port, framed by the definition's (or the tcp link's own) `framing`.
pub struct TcpTransport {
    stream: std::net::TcpStream,
    buf: Vec<u8>,
}

/// Connect timeout of a TCP instrument (an absent host fails fast instead of hanging).
const TCP_CONNECT: Duration = Duration::from_secs(3);

impl TcpTransport {
    /// `ep.path` is `host` or `host:port` (`[v6]:port`); without a port the link's is used.
    pub fn open(link: &TcpLink, ep: &Endpoint) -> io::Result<Self> {
        use std::net::ToSocketAddrs;
        let target = tcp_target(&ep.path, link.port)?;
        let addrs: Vec<_> = target.to_socket_addrs()?.collect();
        let mut last = io::Error::new(io::ErrorKind::NotFound, format!("{target}: no address"));
        for a in addrs {
            match std::net::TcpStream::connect_timeout(&a, TCP_CONNECT) {
                Ok(stream) => {
                    stream.set_nodelay(true)?;
                    return Ok(TcpTransport { stream, buf: vec![0; 65536] });
                }
                Err(e) => last = access::explained(&e, access::Port::Network, &target, access::Platform::current())
                    .unwrap_or_else(|| io::Error::new(e.kind(), format!("{target}: {e}"))),
            }
        }
        Err(last)
    }
}

/// `host` → `host:port`; `host:port`, `[v6]:port` unchanged.
pub(crate) fn tcp_target(path: &str, port: u16) -> io::Result<String> {
    let p = path.trim();
    if p.is_empty() {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "TCP instrument needs a host"));
    }
    let has_port = if p.starts_with('[') { p.contains("]:") } else { p.matches(':').count() == 1 };
    Ok(if has_port {
        p.to_string()
    } else if p.contains(':') {
        format!("[{p}]:{port}")
    } else {
        format!("{p}:{port}")
    })
}

impl Transport for TcpTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        self.stream.write_all(data)?;
        self.stream.flush()
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        self.stream.set_read_timeout(Some(timeout.max(Duration::from_millis(1))))?;
        match self.stream.read(&mut self.buf) {
            Ok(0) => Err(io::Error::new(io::ErrorKind::ConnectionAborted, "the instrument closed the connection")),
            Ok(n) => Ok(self.buf[..n].to_vec()),
            Err(e) if matches!(e.kind(), io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock) => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }
}

pub struct SerialTransport {
    port: Box<dyn serialport::SerialPort>,
    buf: Vec<u8>,
    /// Wait for the output to drain after a write. Only without flow control: with RTS/CTS a
    /// device that never raises CTS (wrong device, unplugged cable) would block the drain forever.
    drain: bool,
}

fn parse_format(f: &str) -> io::Result<(serialport::DataBits, serialport::Parity, serialport::StopBits)> {
    use serialport::{DataBits, Parity, StopBits};
    let bad = || io::Error::new(io::ErrorKind::InvalidInput, format!("invalid serial format {f:?}"));
    let mut c = f.chars();
    let bits = match c.next() {
        Some('5') => DataBits::Five,
        Some('6') => DataBits::Six,
        Some('7') => DataBits::Seven,
        Some('8') => DataBits::Eight,
        _ => return Err(bad()),
    };
    let parity = match c.next() {
        Some('N') => Parity::None,
        Some('E') => Parity::Even,
        Some('O') => Parity::Odd,
        _ => return Err(bad()),
    };
    let stop = match c.as_str() {
        "1" => StopBits::One,
        "2" => StopBits::Two,
        _ => return Err(bad()),
    };
    Ok((bits, parity, stop))
}

impl SerialTransport {
    pub fn open(link: &SerialLink, ep: &Endpoint) -> io::Result<Self> {
        let (bits, parity, stop) = parse_format(ep.format.as_deref().unwrap_or(&link.format))?;
        let flow = match link.flow {
            Flow::None => serialport::FlowControl::None,
            Flow::RtsCts => serialport::FlowControl::Hardware,
            Flow::XonXoff => serialport::FlowControl::Software,
        };
        let mut port = serialport::new(&ep.path, ep.baud.unwrap_or(link.baud))
            .data_bits(bits)
            .parity(parity)
            .stop_bits(stop)
            .flow_control(flow)
            .timeout(Duration::from_millis(100))
            .open()
            .map_err(|e| access::explain(io::Error::from(e), access::Port::Serial, &ep.path))?;
        let required = link.modem_control == ModemControl::Required;
        let dtr = set_line(link.dtr, |v| port.write_data_terminal_ready(v));
        let rts = set_line(link.rts, |v| port.write_request_to_send(v));
        for r in [dtr, rts] {
            // best_effort: some adapters / pseudo-terminals have no modem-control lines
            if let (Err(e), true) = (r, required) {
                return Err(io::Error::from(e));
            }
        }
        std::thread::sleep(Duration::from_millis(link.settle_ms as u64));
        let _ = port.clear(serialport::ClearBuffer::Input);
        Ok(SerialTransport { port, buf: vec![0; 4096], drain: link.flow == Flow::None })
    }
}

impl Drop for SerialTransport {
    fn drop(&mut self) {
        // With flow control, bytes the device never accepted would hold close() for the kernel's
        // closing_wait (30 s on Linux) - and the port with it. Without, the output has drained.
        // Turning flow control off first lets the adapter release bytes it holds back for CTS.
        if !self.drain {
            let _ = self.port.set_flow_control(serialport::FlowControl::None);
            let _ = self.port.clear(serialport::ClearBuffer::All);
        }
    }
}

fn set_line(state: LineState, mut set: impl FnMut(bool) -> serialport::Result<()>) -> serialport::Result<()> {
    match state {
        LineState::Untouched => Ok(()),
        LineState::Low => set(false),
        LineState::High => set(true),
    }
}

impl Transport for SerialTransport {
    fn write(&mut self, data: &[u8]) -> io::Result<()> {
        // a write waits at most this long for the device to accept it (flow control)
        self.port.set_timeout(Duration::from_secs(2)).map_err(io::Error::from)?;
        self.port.write_all(data)?;
        if self.drain {
            self.port.flush()?;
        }
        Ok(())
    }

    fn read(&mut self, timeout: Duration) -> io::Result<Vec<u8>> {
        self.port.set_timeout(timeout.max(Duration::from_millis(1))).map_err(io::Error::from)?;
        match self.port.read(&mut self.buf) {
            Ok(n) => Ok(self.buf[..n].to_vec()),
            Err(e) if e.kind() == io::ErrorKind::TimedOut => Ok(Vec::new()),
            Err(e) => Err(e),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A file this user may not open (mode 000), or None when the user may anyway (root).
    #[cfg(target_os = "linux")]
    fn forbidden_node(name: &str) -> Option<std::path::PathBuf> {
        use std::os::unix::fs::PermissionsExt;
        let p = std::env::temp_dir().join(format!("bm-access-{}-{name}", std::process::id()));
        std::fs::write(&p, b"").unwrap();
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        if std::fs::OpenOptions::new().read(true).write(true).open(&p).is_ok() {
            let _ = std::fs::remove_file(&p);
            return None;
        }
        Some(p)
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn a_port_linux_refuses_says_what_to_do() {
        let links: [(Link, &str); 2] = [
            (Link::Usbtmc(serde_json::from_value(serde_json::json!({})).unwrap()), access::USB_HELP),
            (Link::Serial(serde_json::from_value(serde_json::json!({"baud": 9600})).unwrap()), access::SERIAL_HELP),
        ];
        for (i, (link, help)) in links.into_iter().enumerate() {
            let Some(node) = forbidden_node(&i.to_string()) else { return };
            let path = node.to_string_lossy().to_string();
            let err = open(&link, &Endpoint { path: path.clone(), ..Default::default() }).err().expect("refused");
            let _ = std::fs::remove_file(&node);
            let msg = err.to_string();
            assert!(msg.contains(&path) && msg.ends_with(help), "{msg}");
        }
    }

    #[test]
    fn tcp_targets_get_the_link_port_unless_they_name_one() {
        assert_eq!(tcp_target("192.168.1.50", 5025).unwrap(), "192.168.1.50:5025");
        assert_eq!(tcp_target("scope.lan:5555", 5025).unwrap(), "scope.lan:5555");
        assert_eq!(tcp_target("fe80::1", 5025).unwrap(), "[fe80::1]:5025");
        assert_eq!(tcp_target("[fe80::1]:5555", 5025).unwrap(), "[fe80::1]:5555");
        assert!(tcp_target(" ", 5025).is_err());
    }

    #[test]
    fn tcp_transport_exchanges_bytes_and_reports_silence_as_empty() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = std::thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut b = [0u8; 64];
            let n = s.read(&mut b).unwrap();
            assert_eq!(&b[..n], b"*IDN?\n");
            s.write_all(b"RIGOL TECHNOLOGIES,DP832,DP8A0000,00.01.16\n").unwrap();
            std::thread::sleep(Duration::from_millis(300));
        });
        let link = TcpLink { port, framing: None };
        let mut t = TcpTransport::open(&link, &Endpoint { path: "127.0.0.1".into(), ..Endpoint::default() }).unwrap();
        t.write(b"*IDN?\n").unwrap();
        let mut got = Vec::new();
        while !got.ends_with(b"\n") {
            got.extend(t.read(Duration::from_millis(500)).unwrap());
        }
        assert!(got.starts_with(b"RIGOL TECHNOLOGIES,DP832"));
        assert!(t.read(Duration::from_millis(50)).unwrap().is_empty(), "silence is an empty read");
        server.join().unwrap();
    }

    #[test]
    fn serial_formats() {
        assert!(parse_format("8N1").is_ok());
        assert!(parse_format("7E2").is_ok());
        assert!(parse_format("9N1").is_err());
        assert!(parse_format("8X1").is_err());
    }
}
