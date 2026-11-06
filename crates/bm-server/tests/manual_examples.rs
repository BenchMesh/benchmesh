//! Speculative coverage check: every `devices/examples/<definition>.yaml` drives its definition
//! through the real engine with replies quoted from the instrument's manual - identify, one
//! poll of every class (required status keys present and typed), every readable operation and
//! every setter with a valid value. No hardware; this is what "Awaiting confirmation"
//! definitions are tested with before a user confirms them.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use bm_devicedef::{load_catalog, Catalog, DeviceDef, RxMode};
use bm_driver::engine::Engine;
use bm_driver::sim::SimTransport;
use bm_driver::Error;
use regex::Regex;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Example {
    definition: String,
    /// Config transport (link + its framing); default: the definition's only/serial link.
    #[serde(default)]
    transport: Option<String>,
    /// Model the identify reply must select.
    model: String,
    #[serde(default)]
    replies: Vec<ReplyRule>,
    /// Modbus instruments: the register map a simulated slave answers from (writes change it).
    #[serde(default)]
    modbus: Option<ModbusMap>,
    /// class -> channel -> status key -> expected value (numbers compared approximately; SI
    /// values by their number).
    #[serde(default)]
    expect: BTreeMap<String, BTreeMap<u8, BTreeMap<String, Value>>>,
}

#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct ModbusMap {
    #[serde(default)]
    holding: BTreeMap<u16, u16>,
    #[serde(default)]
    input: BTreeMap<u16, u16>,
    #[serde(default)]
    coils: BTreeMap<u16, bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReplyRule {
    /// Regex over the whole command text (without the tx terminator).
    send: String,
    /// Reply text; the rx terminator is appended in `terminator` mode when missing.
    reply: String,
    #[allow(dead_code)]
    #[serde(default)]
    source: Option<String>,
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn examples() -> Vec<(PathBuf, Example)> {
    let dir = repo().join("devices/examples");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().path()).filter(|p| p.extension().is_some_and(|x| x == "yaml")).collect();
    files.sort();
    files
        .into_iter()
        .map(|f| {
            let text = std::fs::read_to_string(&f).unwrap();
            let ex: Example = serde_yaml_ng::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
            (f, ex)
        })
        .collect()
}

/// The definition as the example's transport sees it, with short timeouts (the simulator
/// answers at once; a missing reply should not cost the full device timeout).
fn resolved(cat: &Catalog, ex: &Example) -> Result<DeviceDef, String> {
    let (_, def) = cat.devices.get(&ex.definition).ok_or(format!("no definition `{}`", ex.definition))?;
    let mut d = def.for_transport(ex.transport.as_deref())?;
    d.framing.rx.timeout_ms = d.framing.rx.timeout_ms.min(150);
    for c in d.commands.values_mut() {
        c.timeout_ms = c.timeout_ms.map(|t| t.min(150));
        c.gap_after_ms = c.gap_after_ms.map(|g| g.min(5));
        c.settle_ms = None;
    }
    d.framing.timing.gap_after_write_ms = d.framing.timing.gap_after_write_ms.min(5);
    if let Some(s) = d.link.serial.as_mut() {
        s.settle_ms = 0;
    }
    Ok(d)
}

fn responder(def: &DeviceDef, rules: &[ReplyRule]) -> Result<bm_driver::sim::Responder, String> {
    let tx = def.framing.tx.terminator.clone();
    let rx = &def.framing.rx;
    let term = (rx.mode == RxMode::Terminator).then(|| rx.terminator.clone().unwrap_or_default());
    let rules: Vec<(Regex, Vec<u8>)> = rules
        .iter()
        .map(|r| {
            let re = Regex::new(&format!("^(?:{})$", r.send)).map_err(|e| format!("send {:?}: {e}", r.send))?;
            let mut reply = r.reply.chars().map(|c| c as u32 as u8).collect::<Vec<u8>>();
            if let Some(t) = &term {
                if !reply.ends_with(t.as_bytes()) {
                    reply.extend_from_slice(t.as_bytes());
                }
            }
            Ok((re, reply))
        })
        .collect::<Result<_, String>>()?;
    Ok(Box::new(move |w: &[u8]| {
        let text: String = w.iter().map(|&b| b as char).collect();
        let cmd = text.strip_suffix(tx.as_str()).unwrap_or(&text);
        // one write may carry several `;`-joined commands that each answer: not supported, the
        // whole write is matched
        rules.iter().find(|(re, _)| re.is_match(cmd)).map(|(_, r)| r.clone())
    }))
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        Value::Object(o) => o.get("val").and_then(num),
        _ => None,
    }
}

/// A value a setter must accept, from its `/methods` value spec.
fn sample_value(def: &DeviceDef, cls: &str, op: &str) -> Option<String> {
    let o = &def.classes[cls].operations[op];
    let (spec, _) = bm_server::opspec::setter(def, o)?;
    let spec = spec?;
    match spec["type"].as_str()? {
        "number" | "integer" => {
            let min = spec["min"].as_f64().unwrap_or(0.0);
            let max = spec["max"].as_f64().unwrap_or(min + 1.0);
            let v = if min <= 1.0 && 1.0 <= max { 1.0 } else { min };
            Some(if spec["type"] == "integer" { format!("{}", v as i64) } else { format!("{v}") })
        }
        "enum" => spec["values"].as_array()?.first()?.as_str().map(str::to_string),
        "bool" => Some("ON".into()),
        _ => None,
    }
}

fn check(cat: &Catalog, ex: &Example) -> Vec<String> {
    let mut problems = Vec::new();
    let def = match resolved(cat, ex) {
        Ok(d) => Arc::new(d),
        Err(e) => return vec![e],
    };
    let sim = match (&ex.modbus, &def.framing.modbus) {
        (Some(map), Some(m)) => {
            let mut slave = bm_driver::modbus::Slave { address: m.address, holding: map.holding.clone(), input: map.input.clone(), coils: map.coils.clone() };
            SimTransport::new(Box::new(move |w: &[u8]| slave.respond(w)))
        }
        (Some(_), None) => return vec!["`modbus` register map for a definition without framing.modbus".into()],
        (None, _) => match responder(&def, &ex.replies) {
            Ok(r) => SimTransport::new(r),
            Err(e) => return vec![e],
        },
    };
    let mut e = match Engine::open(def.clone(), Box::new(sim)) {
        Ok(e) => e,
        Err(err) => return vec![format!("identify: {err}")],
    };
    if e.model.name != ex.model {
        problems.push(format!("identify selected model {} (expected {})", e.model.name, ex.model));
    }
    let updates = match e.poll_due() {
        Ok(u) => u,
        Err(err) => return vec![format!("poll: {err}")],
    };
    for (cls, binding) in &def.classes {
        let Some(contract) = cat.classes.get(cls) else { continue };
        let channels = e.model.channels.get(cls).copied().unwrap_or(binding.channels);
        for ch in 1..=channels {
            // the last update of a channel has the results of every poll group
            let Some(u) = updates.iter().rfind(|u| &u.class == cls && u.ch == ch) else {
                problems.push(format!("{cls} ch{ch}: no status after a poll"));
                continue;
            };
            for (key, spec) in &contract.status {
                match u.status.get(key) {
                    None | Some(Value::Null) if spec.required => problems.push(format!("{cls} ch{ch} {key}: required, but null (reply example missing?)")),
                    Some(v) if !v.is_null() && !spec.kind.accepts(&spec.values, v) => {
                        problems.push(format!("{cls} ch{ch} {key}: {v} is not a {:?}", spec.kind))
                    }
                    _ => {}
                }
            }
            for (key, want) in ex.expect.get(cls).and_then(|c| c.get(&ch)).into_iter().flatten() {
                let got = u.status.get(key).cloned().unwrap_or(Value::Null);
                let same = match (num(want), num(&got)) {
                    (Some(a), Some(b)) if want.is_number() => (a - b).abs() <= 1e-9 * a.abs().max(1.0),
                    _ => &got == want,
                };
                if !same {
                    problems.push(format!("{cls} ch{ch} {key}: got {got}, expected {want}"));
                }
            }
        }
        for (op, o) in &binding.operations {
            if o.get.is_some() {
                if let Err(err) = e.get(cls, op, 1) {
                    problems.push(format!("{cls} get {op}: {err}"));
                }
            }
            if o.set.is_some() {
                let Some(v) = sample_value(&def, cls, op) else { continue };
                match e.set(cls, op, 1, &v) {
                    Ok(()) | Err(Error::Refused(_)) => {}
                    Err(err) => problems.push(format!("{cls} set {op} {v}: {err}")),
                }
            }
        }
    }
    problems
}

#[test]
fn definitions_work_with_their_manual_examples() {
    let cat = load_catalog(&repo().join("devices")).unwrap();
    let mut failed = Vec::new();
    let all = examples();
    assert!(!all.is_empty(), "no devices/examples/*.yaml");
    for (file, ex) in &all {
        for p in check(&cat, ex) {
            failed.push(format!("{}: {p}", file.file_name().unwrap().to_string_lossy()));
        }
    }
    assert!(failed.is_empty(), "\n{}", failed.join("\n"));
}

#[test]
fn awaiting_definitions_have_manual_examples() {
    let cat = load_catalog(&repo().join("devices")).unwrap();
    let covered: Vec<String> = examples().into_iter().map(|(_, e)| e.definition).collect();
    // definitions whose every model is still awaiting confirmation must carry examples
    let missing: Vec<&String> = cat
        .devices
        .iter()
        .filter(|(_, (_, d))| d.models.iter().all(|m| m.support == bm_devicedef::Support::Awaiting))
        .map(|(id, _)| id)
        .filter(|id| !covered.contains(id))
        .collect();
    assert!(missing.is_empty(), "definitions without devices/examples/<id>.yaml: {missing:?}");
}

/// The MHO900 definition on the wire: what the guide wants, not what its base sends.
mod mho900 {
    use super::*;
    use std::sync::Mutex;

    type Sent = Arc<Mutex<Vec<String>>>;

    fn open(idn: &'static str) -> (Result<Engine, Error>, Sent, Arc<DeviceDef>) {
        let cat = load_catalog(&repo().join("devices")).unwrap();
        let (_, def) = cat.devices.get("rigol_mho900").expect("definition rigol_mho900");
        let def = Arc::new(def.for_transport(Some("usbtmc")).unwrap());
        let sent = Arc::new(Mutex::new(Vec::new()));
        let log = sent.clone();
        let sim = SimTransport::new(Box::new(move |w: &[u8]| {
            let text = String::from_utf8_lossy(w).trim().to_string();
            let reply = (text == "*IDN?").then(|| idn.as_bytes().to_vec());
            log.lock().unwrap().push(text);
            reply
        }));
        (Engine::open(def.clone(), Box::new(sim)), sent, def)
    }

    #[test]
    fn identifies_every_model_whatever_the_vendor_text() {
        for (idn, model) in [
            ("RIGOL TECHNOLOGIES,MHO934,X,00.01.00", "MHO934"),
            ("RIGOL TECHNOLOGIES,MHO954,X,00.01.00", "MHO954"),
            ("NK TECHNOLOGIES,MHO954,X,00.01.03", "MHO954"),
            ("RIGOL TECHNOLOGIES,MHO984,X,00.01.00", "MHO984"),
        ] {
            assert_eq!(open(idn).0.unwrap_or_else(|e| panic!("{idn}: {e}")).model.name, model);
        }
        assert!(matches!(open("RIGOL TECHNOLOGIES,DHO804,X,00.01.03").0, Err(Error::UnknownModel(_))));
    }

    #[test]
    fn connecting_selects_the_screen_trace_in_12_bits_whatever_window_was_left_set() {
        let (e, sent, _) = open("RIGOL TECHNOLOGIES,MHO954,X,00.01.00");
        let mut e = e.unwrap();
        sent.lock().unwrap().clear();
        e.start_session().unwrap();
        assert_eq!(*sent.lock().unwrap(), [":WAVeform:MODE NORMal", ":WAVeform:FORMat WORD", ":WAVeform:STARt 1", ":WAVeform:STOP 1000"]);
    }

    #[test]
    fn setters_send_what_the_guide_documents() {
        let (e, sent, _) = open("RIGOL TECHNOLOGIES,MHO954,X,00.01.00");
        let mut e = e.unwrap();
        for (op, v, wire) in [
            ("run", "AUTO", ":AUToset"),
            ("impedance", "50", ":CHANnel2:IMPedance FIFTy"),
            ("impedance", "1M", ":CHANnel2:IMPedance OMEG"),
            ("acquire_type", "HRES", ":ACQuire:TYPE HRESolution"),
            ("bwlimit", "250M", ":CHANnel2:BWLimit 250M"),
            ("probe", "15", ":CHANnel2:PROBe 15"),
            ("timebase", "5e-10", ":TIMebase:MAIN:SCALe 5e-10"),
            // 200 uV/div through a 0.001 probe: the guide's smallest scale at its smallest ratio
            ("scale", "2e-7", ":CHANnel2:SCALe 2e-07"),
        ] {
            sent.lock().unwrap().clear();
            e.set("OSC", op, 2, v).unwrap_or_else(|err| panic!("{op} {v}: {err}"));
            assert_eq!(sent.lock().unwrap().last().map(String::as_str), Some(wire), "{op} {v}");
        }
    }

    #[test]
    fn offers_only_the_trigger_sources_it_has_without_a_logic_probe() {
        let (_, _, def) = open("RIGOL TECHNOLOGIES,MHO954,X,00.01.00");
        let (spec, _) = bm_server::opspec::setter(&def, &def.classes["OSC"].operations["trigger_source"]).unwrap();
        assert_eq!(spec.unwrap()["values"], serde_json::json!(["CHAN1", "CHAN2", "CHAN3", "CHAN4"]));
    }

    #[test]
    fn impedance_does_not_count_as_energizing_for_flows() {
        let (_, _, def) = open("RIGOL TECHNOLOGIES,MHO954,X,00.01.00");
        let (_, energizes) = bm_server::opspec::setter(&def, &def.classes["OSC"].operations["impedance"]).unwrap();
        assert!(!energizes);
    }
}
