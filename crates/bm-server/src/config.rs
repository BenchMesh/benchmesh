//! config.yaml: `{version: 1, devices: [{id, name, driver, model?, transport, port|device|host|match, baud?, serial?, parked?}]}`
//! (`host`: `tcpip` instruments, `name-or-address[:port]`; `parked: true`: not connected, see
//! `ai_context/PARKING.md`).
//! `match` = the instrument's identity, followed only by a build with a finder
//! (`extension::Finder`, the official build); `port`/`device` is then only the last known port.
//! This build opens the `port`/`device` an entry names.
//! Device entries are kept as raw JSON objects (unknown keys survive round trips).

use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use crate::extension::Finder;

#[derive(Debug, Clone)]
pub struct ConfigFile {
    pub path: PathBuf,
}

pub type DeviceEntry = Map<String, Value>;

impl ConfigFile {
    pub fn load(&self) -> Result<Vec<DeviceEntry>, String> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(format!("{}: {e}", self.path.display())),
        };
        let v: Value = serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", self.path.display()))?;
        Ok(v.get("devices")
            .and_then(|d| d.as_array())
            .map(|a| a.iter().filter_map(|d| d.as_object().cloned()).collect())
            .unwrap_or_default())
    }

    /// Atomic write (temp file in the same directory, then rename).
    pub fn save(&self, devices: &[DeviceEntry]) -> Result<(), String> {
        let doc = json!({"version": 1, "devices": devices});
        let text = serde_yaml_ng::to_string(&doc).map_err(|e| e.to_string())?;
        let dir = self.path.parent().filter(|p| !p.as_os_str().is_empty()).unwrap_or(Path::new("."));
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tmp = dir.join(format!(".config_{}.yaml.tmp", std::process::id()));
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

pub fn str_field<'a>(d: &'a DeviceEntry, k: &str) -> Option<&'a str> {
    d.get(k).and_then(|v| v.as_str())
}

/// Validate one device entry; returns a human-readable reason on failure.
/// `finder` follows `match` (the official build); without one an entry needs its `port` /
/// `device` and `match` is kept but not followed.
pub fn validate_entry(d: &DeviceEntry, known_drivers: &[String], finder: Option<&dyn Finder>) -> Result<(), String> {
    let id = str_field(d, "id").unwrap_or_default();
    if id.is_empty() || id.len() > 50 || !id.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c)) {
        return Err(format!("invalid device id {id:?} (1-50 chars: letters, digits, - _ .)"));
    }
    if id == crate::environment::DEVICE_ID {
        return Err(format!("device id {id:?} is reserved for the environmental sensor"));
    }
    let driver = str_field(d, "driver").unwrap_or_default();
    if !known_drivers.iter().any(|k| k == driver) {
        return Err(format!("unknown driver {driver:?}; available: {known_drivers:?}"));
    }
    if d.get("parked").is_some_and(|p| !p.is_boolean()) {
        return Err("`parked` must be true or false".into());
    }
    let identified = match (d.get("match"), finder) {
        (Some(m), Some(f)) => f.check(m).map(|_| true)?,
        _ => false,
    };
    let lan = matches!(str_field(d, "transport"), Some("tcpip" | "vxi11"));
    if lan && d.get("match").is_some() {
        return Err("`match` (USB identity) does not apply to a LAN device".into());
    }
    match str_field(d, "transport").unwrap_or("serial") {
        "serial" => {
            if !identified && str_field(d, "port").unwrap_or_default().is_empty() {
                return Err(needs("serial", "port", finder));
            }
            if let Some(b) = d.get("baud") {
                const BAUDS: [u64; 10] = [300, 1200, 2400, 4800, 9600, 19200, 38400, 57600, 115200, 230400];
                if !b.as_u64().is_some_and(|b| BAUDS.contains(&b)) {
                    return Err(format!("invalid baud {b}"));
                }
            }
            if let Some(s) = str_field(d, "serial") {
                let ok = s.len() == 3 && "5678".contains(&s[0..1]) && "NEO".contains(&s[1..2]) && "12".contains(&s[2..3]);
                if !ok {
                    return Err(format!("invalid serial format {s:?} (e.g. 8N1)"));
                }
            }
        }
        "usbtmc" => {
            if !identified && str_field(d, "device").unwrap_or_default().is_empty() {
                return Err(needs("usbtmc", "device", finder));
            }
        }
        "tcpip" | "vxi11" => {
            let host = str_field(d, "host").unwrap_or_default().trim();
            if host.is_empty() || host.chars().any(|c| c.is_whitespace() || c == '/') {
                return Err("a LAN device needs `host` (name or address, optionally :port)".into());
            }
        }
        t => return Err(format!("unsupported transport {t:?} (serial, usbtmc, tcpip, vxi11)")),
    }
    Ok(())
}

/// What a USB entry without a usable path lacks.
fn needs(transport: &str, field: &str, finder: Option<&dyn Finder>) -> String {
    match finder {
        Some(_) => format!("{transport} device needs `{field}` or `match`"),
        None => format!("{transport} device needs `{field}` (this build does not find instruments by `match`)"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(v: serde_json::Value) -> DeviceEntry {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn tcpip_entries_need_a_host_and_no_usb_identity() {
        let drivers = vec!["rigol_dp800".to_string()];
        let ok = entry(serde_json::json!({"id": "psu", "driver": "rigol_dp800", "transport": "tcpip", "host": "192.168.1.50:5555"}));
        assert!(validate_entry(&ok, &drivers, None).is_ok());
        for bad in [
            serde_json::json!({"id": "psu", "driver": "rigol_dp800", "transport": "tcpip"}),
            serde_json::json!({"id": "psu", "driver": "rigol_dp800", "transport": "tcpip", "host": "a b"}),
            serde_json::json!({"id": "psu", "driver": "rigol_dp800", "transport": "tcpip", "host": "h",
                               "match": {"usb": {"vid": "1ab1", "pid": "0e11"}}}),
        ] {
            assert!(validate_entry(&entry(bad.clone()), &drivers, None).is_err(), "{bad}");
        }
    }

    #[test]
    fn parked_is_a_boolean() {
        let drivers = vec!["tenma_72".to_string()];
        let e = |p: serde_json::Value| entry(serde_json::json!({"id": "t", "driver": "tenma_72", "port": "/dev/ttyACM0", "parked": p}));
        assert!(validate_entry(&e(serde_json::json!(true)), &drivers, None).is_ok());
        assert!(validate_entry(&e(serde_json::json!(false)), &drivers, None).is_ok());
        assert_eq!(validate_entry(&e(serde_json::json!("yes")), &drivers, None).unwrap_err(), "`parked` must be true or false");
    }

    /// Accepts a `match` with `usb`, opens nothing.
    struct Fake;
    impl Finder for Fake {
        fn check(&self, m: &Value) -> Result<(), String> {
            m.get("usb").map(|_| ()).ok_or_else(|| "invalid `match`".to_string())
        }
        fn connector(&self, _: &DeviceEntry, _: &bm_devicedef::Link, _: &bm_transport::Endpoint) -> bm_driver::actor::Connector {
            Box::new(|| Err("none".into()))
        }
        fn presence(&self, _: &DeviceEntry, _: &bm_devicedef::Link, _: &bm_transport::Endpoint) -> bm_driver::actor::Presence {
            Box::new(|| Err("none".into()))
        }
    }

    #[test]
    fn match_replaces_the_port_only_with_a_finder() {
        let drivers = vec!["owon_xdm".to_string()];
        let m = json!({"usb": {"vid": "1a86", "pid": "7523"}});
        let bare = entry(json!({"id": "x", "driver": "owon_xdm", "match": m}));
        let err = validate_entry(&bare, &drivers, None).unwrap_err();
        assert!(err.contains("needs `port`") && err.contains("`match`"), "{err}");
        assert!(validate_entry(&bare, &drivers, Some(&Fake)).is_ok());
        let hinted = entry(json!({"id": "x", "driver": "owon_xdm", "match": m, "port": "/dev/ttyUSB0"}));
        assert!(validate_entry(&hinted, &drivers, None).is_ok(), "kept, the named port is opened");
        let bad = entry(json!({"id": "x", "driver": "owon_xdm", "match": {}, "port": "/dev/ttyUSB0"}));
        assert_eq!(validate_entry(&bad, &drivers, Some(&Fake)).unwrap_err(), "invalid `match`");
    }
}
