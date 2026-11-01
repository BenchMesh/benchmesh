//! bm-probe - run device definitions against real instruments.
//!
//! ```text
//! bm-probe [--config FILE] [--devices DIR] verify [--id ID]... [--polls N] [--screenshots]
//! bm-probe [...] get --id ID CLASS OP [--ch N]
//! bm-probe [...] set --id ID CLASS OP VALUE [--ch N]
//! bm-probe [...] screenshot --id ID CLASS --out FILE [--ch N]
//! ```
//! `verify` is read-only apart from the definitions' session steps (e.g. OEL remote mode,
//! returned to local on exit): it polls and calls every readable operation. Only `set`
//! changes instrument state. Config default: ~/.benchmesh/config.yaml.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use bm_devicedef::{load_catalog, Catalog, DeviceDef, Repr};
use bm_driver::engine::{Engine, Output};
use bm_transport::Endpoint;
use serde_json::{json, Value};

struct Args {
    config: PathBuf,
    devices: PathBuf,
    rest: Vec<String>,
}

fn parse_args() -> Args {
    let mut config = std::env::var("HOME").map(|h| PathBuf::from(h).join(".benchmesh/config.yaml")).unwrap_or_default();
    let mut devices = PathBuf::from("devices");
    let mut rest = Vec::new();
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        match a.as_str() {
            "--config" => config = it.next().map(PathBuf::from).unwrap_or(config),
            "--devices" => devices = it.next().map(PathBuf::from).unwrap_or(devices),
            _ => rest.push(a),
        }
    }
    Args { config, devices, rest }
}

/// `--flag value` from the remaining args (removed); repeated flags collect all values.
fn take(rest: &mut Vec<String>, flag: &str) -> Vec<String> {
    let mut out = Vec::new();
    while let Some(i) = rest.iter().position(|a| a == flag) {
        rest.remove(i);
        if i < rest.len() {
            out.push(rest.remove(i));
        }
    }
    out
}

fn take_switch(rest: &mut Vec<String>, flag: &str) -> bool {
    let found = rest.iter().any(|a| a == flag);
    rest.retain(|a| a != flag);
    found
}

struct Configured {
    id: String,
    driver: String,
    transport: Option<String>,
    /// The port the entry names (`match` is not followed: bm-probe opens fixed ports only).
    endpoint: Endpoint,
}

fn load_config(path: &PathBuf) -> Result<Vec<Configured>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let v: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
    let devs = v.get("devices").and_then(|d| d.as_sequence()).ok_or("config has no `devices` list")?;
    let s = |d: &serde_yaml_ng::Value, k: &str| d.get(k).and_then(|x| x.as_str()).map(str::to_string);
    Ok(devs
        .iter()
        .map(|d| Configured {
            id: s(d, "id").unwrap_or_default(),
            driver: s(d, "driver").unwrap_or_default(),
            transport: s(d, "transport"),
            endpoint: Endpoint {
                path: s(d, "port").or_else(|| s(d, "device")).or_else(|| s(d, "host")).unwrap_or_default(),
                baud: d.get("baud").and_then(|b| b.as_u64()).map(|b| b as u32),
                format: s(d, "serial"),
            },
        })
        .collect())
}

fn connect(cat: &Catalog, dev: &Configured) -> Result<(Engine, Arc<DeviceDef>), String> {
    let def = cat.devices.get(&dev.driver).ok_or(format!("{}: no definition `{}`", dev.id, dev.driver))?.1.for_transport(dev.transport.as_deref());
    let def = Arc::new(def.map_err(|e| format!("{}: {e}", dev.id))?);
    let link = def.link.single().ok_or(format!("{}: no single link", dev.id))?;
    let attempts = bm_transport::attempts(&link, &dev.endpoint);
    let mut errors = Vec::new();
    for a in attempts {
        let port = a.port.clone();
        match a.connect(def.clone()) {
            Ok(e) => return Ok((e, def)),
            Err(e) => errors.push(format!("{port}: {e}")),
        }
    }
    Err(format!("{}: connect: {}", dev.id, errors.join("; ")))
}

fn ms(t: Instant) -> f64 {
    (t.elapsed().as_secs_f64() * 1e4).round() / 10.0
}

/// Compact rendering: SI dicts as "number symbol".
fn show(v: &Value) -> String {
    match v.get("si") {
        Some(si) => format!("{}{}", si["number"].as_str().unwrap_or("?"), si.get("symbol").and_then(|s| s.as_str()).unwrap_or("")),
        None => v.to_string(),
    }
}

fn verify(cat: &Catalog, devs: &[&Configured], polls: usize, screenshots: bool) -> bool {
    let mut ok = true;
    for dev in devs {
        let place = &dev.endpoint.path;
        println!("=== {} ({}) {place}", dev.id, dev.driver);
        let t0 = Instant::now();
        let (mut e, def) = match connect(cat, dev) {
            Ok(x) => x,
            Err(err) => {
                println!("  FAIL {err}");
                ok = false;
                continue;
            }
        };
        println!("  connected in {} ms: {:?} -> model {}", ms(t0), e.idn, e.model.name);
        let mut last: std::collections::BTreeMap<(String, u8), bm_driver::engine::StatusUpdate> = Default::default();
        for i in 0..polls {
            // make every group due
            let t = Instant::now();
            while e.next_due().is_some_and(|d| d > Instant::now()) {
                std::thread::sleep(std::time::Duration::from_millis(5));
                if t.elapsed().as_secs() > 10 {
                    break;
                }
            }
            let t = Instant::now();
            match e.poll_due() {
                Ok(u) => {
                    println!("  poll {}: {} ms, {} class channel(s)", i + 1, ms(t), u.len());
                    for x in u {
                        last.insert((x.class.clone(), x.ch), x);
                    }
                }
                Err(err) => {
                    println!("  FAIL poll {}: {err}", i + 1);
                    ok = false;
                }
            }
        }
        for u in last.values() {
            let nulls: Vec<&String> = u.status.iter().filter(|(_, v)| v.is_null()).map(|(k, _)| k).collect();
            let shown: Vec<String> = u.status.iter().map(|(k, v)| format!("{k}={}", show(v))).collect();
            println!("  {} ch{}: {}", u.class, u.ch, shown.join(" "));
            if !nulls.is_empty() {
                println!("    note: null (unknown) keys: {nulls:?}");
            }
        }
        for (class, b) in &def.classes {
            let chans = e.channels(class).unwrap_or(1);
            for (op, o) in &b.operations {
                if o.get.is_none() || (o.repr == Repr::Bytes && !screenshots) {
                    continue;
                }
                for ch in 1..=chans {
                    let t = Instant::now();
                    match e.get(class, op, ch) {
                        Ok(Output::Value(v)) => println!("  GET {class}/{op} ch{ch}: {} ({} ms)", show(&v), ms(t)),
                        Ok(Output::Bytes(b)) => println!("  GET {class}/{op} ch{ch}: {} bytes ({} ms)", b.len(), ms(t)),
                        Err(err) if state_refusal(&err) => println!("  REFUSED GET {class}/{op} ch{ch}: {err} (the instrument's state now, not a fault)"),
                        Err(err) => {
                            println!("  FAIL GET {class}/{op} ch{ch}: {err}");
                            ok = false;
                        }
                    }
                }
            }
        }
        e.disconnect();
    }
    ok
}

/// A read the definition itself declines in the instrument's state now (a `requires`, e.g. the
/// waveform of a channel that is not displayed): no fault of the definition, the check passes. A
/// Modbus exception (the register is wrong for this model) is one.
fn state_refusal(e: &bm_driver::Error) -> bool {
    matches!(e, bm_driver::Error::Refused(m) if !m.starts_with("modbus exception"))
}

fn main() -> ExitCode {
    let a = parse_args();
    let mut rest = a.rest;
    let cat = match load_catalog(&a.devices) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{}", e.join("\n"));
            return ExitCode::FAILURE;
        }
    };
    let config = match load_config(&a.config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
    };
    let ids = take(&mut rest, "--id");
    let chosen: Vec<&Configured> = config.iter().filter(|d| ids.is_empty() || ids.contains(&d.id)).collect();
    let ch: u8 = take(&mut rest, "--ch").first().and_then(|c| c.parse().ok()).unwrap_or(1);
    let cmd = if rest.is_empty() { String::new() } else { rest.remove(0) };
    let one = || -> Result<&Configured, String> {
        match chosen.as_slice() {
            [d] => Ok(*d),
            _ => Err("select exactly one device with --id".into()),
        }
    };
    let result: Result<bool, String> = match cmd.as_str() {
        "verify" => {
            let polls = take(&mut rest, "--polls").first().and_then(|p| p.parse().ok()).unwrap_or(3);
            let shots = take_switch(&mut rest, "--screenshots");
            Ok(verify(&cat, &chosen, polls, shots))
        }
        "get" | "set" => (|| {
            let dev = one()?;
            let (mut e, _) = connect(&cat, dev)?;
            let r = if cmd == "get" {
                let [class, op] = rest.as_slice() else { return Err("usage: get --id ID CLASS OP".into()) };
                e.get(class, op, ch).map(|o| json!({"value": o.value()}))
            } else {
                let [class, op, value] = rest.as_slice() else { return Err("usage: set --id ID CLASS OP VALUE".into()) };
                e.set(class, op, ch, value).map(|_| json!({"ok": true}))
            };
            e.disconnect();
            r.map(|v| {
                println!("{v}");
                true
            })
            .map_err(|err| err.to_string())
        })(),
        "screenshot" => (|| {
            let dev = one()?;
            let out = take(&mut rest, "--out").first().cloned().ok_or("--out FILE required")?;
            let [class] = rest.as_slice() else { return Err("usage: screenshot --id ID CLASS --out FILE".into()) };
            let (mut e, _) = connect(&cat, dev)?;
            let t = Instant::now();
            let r = e.get(class, "screenshot", ch);
            e.disconnect();
            match r.map_err(|err| err.to_string())? {
                Output::Bytes(b) => {
                    std::fs::write(&out, &b).map_err(|err| err.to_string())?;
                    println!("{} bytes in {} ms -> {out}", b.len(), ms(t));
                    Ok(true)
                }
                Output::Value(v) => Err(format!("not a binary operation: {v}")),
            }
        })(),
        _ => Err("usage: bm-probe [--config FILE] [--devices DIR] verify|get|set|screenshot ... (see source header)".into()),
    };
    match result {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(e) => {
            eprintln!("error: {e}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bm_driver::Error;

    #[test]
    fn a_requires_refusal_is_the_instruments_state_not_a_failure() {
        assert!(state_refusal(&Error::Refused("channel is not displayed".into())));
        assert!(!state_refusal(&Error::Refused("modbus exception 2 (illegal data address)".into())));
        assert!(!state_refusal(&Error::Timeout { partial: Vec::new() }));
        assert!(!state_refusal(&Error::Parse("x".into())));
    }
}
