//! Integration tests over the real `devices/` catalog and the Phase 1 wire captures.

use std::collections::BTreeMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};

use bm_devicedef::template::command_wire_regex;
use bm_devicedef::*;

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").canonicalize().unwrap()
}

fn catalog() -> Catalog {
    load_catalog(&repo().join("devices")).unwrap_or_else(|e| panic!("catalog failed to load:\n{}", e.join("\n")))
}

fn errors(issues: &[Issue]) -> Vec<String> {
    issues.iter().filter(|i| i.severity == Severity::Error).map(|i| i.to_string()).collect()
}

#[test]
fn all_definitions_and_classes_are_valid() {
    let cat = catalog();
    assert!(cat.classes.len() >= 5, "class schemas missing");
    assert!(cat.devices.len() >= 5, "device definitions missing");
    let issues = validate_catalog(&cat);
    let errs: Vec<String> =
        issues.iter().filter(|(_, i)| i.severity == Severity::Error).map(|(f, i)| format!("{f}: {i}")).collect();
    assert!(errs.is_empty(), "validation errors:\n{}", errs.join("\n"));
}

#[test]
fn json_schemas_generate() {
    let d = device_json_schema();
    assert!(d["properties"]["commands"].is_object());
    let c = class_json_schema();
    assert!(c["properties"]["operations"].is_object());
}

/// The committed schemas (used by editors and for LLM structured output) must match the types.
#[test]
fn committed_json_schemas_are_up_to_date() {
    for (file, schema) in [("device.schema.json", device_json_schema()), ("class.schema.json", class_json_schema())] {
        let path = repo().join("devices/schema").join(file);
        let committed = fs::read_to_string(&path).unwrap_or_default();
        let generated = serde_json::to_string_pretty(&schema).unwrap() + "\n";
        assert!(
            committed == generated,
            "{} is stale; regenerate with: cargo run -q -p bm-devicedef -- schema {} > devices/schema/{file}",
            path.display(),
            file.split('.').next().unwrap()
        );
    }
}

/// Apply a YAML edit to the Tenma definition and return the validation errors.
fn tenma_errors_after(edit: impl FnOnce(&mut serde_yaml_ng::Value)) -> Vec<String> {
    let cat = catalog();
    let text = fs::read_to_string(repo().join("devices/tenma_72.yaml")).unwrap();
    let mut v: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    edit(&mut v);
    let text = serde_yaml_ng::to_string(&v).unwrap();
    match from_yaml::<DeviceDef>(&text) {
        Err(e) => vec![format!("parse: {e}")],
        Ok(d) => errors(&validate_device(&d, &cat.classes)),
    }
}

fn at<'a>(v: &'a mut serde_yaml_ng::Value, path: &[&str]) -> &'a mut serde_yaml_ng::Value {
    path.iter().fold(v, |v, k| v.get_mut(*k).unwrap_or_else(|| panic!("no key {k}")))
}

fn assert_rejected(errs: Vec<String>, needle: &str) {
    assert!(errs.iter().any(|e| e.contains(needle)), "expected an error containing `{needle}`, got: {errs:#?}");
}

#[test]
fn validator_rejects_broken_definitions() {
    assert!(tenma_errors_after(|_| {}).is_empty(), "baseline must be valid");

    assert_rejected(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "poll"])[0]["read"].as_sequence_mut().unwrap().push("nope".into());
        }),
        "unknown command `nope`",
    );
    assert_rejected(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "poll"])[0]["read"].as_sequence_mut().unwrap().push("output".into());
        }),
        "has no reply",
    );
    assert_rejected(
        tenma_errors_after(|v| *at(v, &["commands", "vout", "send"]) = "VOUT{chan}?".into()),
        "placeholder `{chan}`",
    );
    assert_rejected(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "status"]).as_mapping_mut().unwrap().remove("OUTPUT");
        }),
        "required status key `OUTPUT` missing",
    );
    assert_rejected(
        tenma_errors_after(|v| *at(v, &["classes", "PSU", "status", "CV"]) = "statuz.out".into()),
        "unknown variable `statuz`",
    );
    assert_rejected(
        tenma_errors_after(|v| *at(v, &["classes", "PSU", "status", "CV"]) = "status.out &&".into()),
        "CEL parse error",
    );
    assert_rejected(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "operations"]).as_mapping_mut().unwrap().remove("voltage");
        }),
        "required operation `voltage`",
    );
    assert_rejected(tenma_errors_after(|v| *at(v, &["framing", "rx", "mode"]) = "fixed".into()), "rx mode `fixed`");
    assert_rejected(tenma_errors_after(|v| at(v, &["models"])[0]["match"] = "([".into()), "invalid regex");
    assert_rejected(
        tenma_errors_after(|v| *at(v, &["commands", "vset", "params", "v", "format"]) = "%x".into()),
        "unsupported format",
    );
    assert_rejected(tenma_errors_after(|v| at(v, &["commands", "vout"])["colour"] = "red".into()), "unknown field `colour`");
    assert_rejected(tenma_errors_after(|v| *at(v, &["identify"]) = "output".into()), "identify command must have a reply");
    // a reply parsed to a number for an operation the contract wants as text (OSC probe "1" vs 1.0)
    assert_rejected(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "operations", "output"])["get"] = "vset_q".into();
        }),
        "parsed to a number",
    );
    assert!(
        tenma_errors_after(|v| {
            at(v, &["classes", "PSU", "operations", "output"])["get"] = "vset_q".into();
            at(v, &["commands", "vset_q", "parse"]).as_sequence_mut().unwrap().push("text".into());
        })
        .iter()
        .all(|e| !e.contains("parsed to a number")),
        "ending the parse with `text` is accepted"
    );
}

/// Wire commands the Python service sent that the definitions deliberately do not produce.
/// (definition id, exact wire text, reason)
const NOT_REPRODUCED: &[(&str, &str, &str)] = &[
    ("owon_spm", "MEASure:ALL?\r", "ambiguous 2-field reply; MEAS:ALL:INFO? covers V/I/P"),
    ("owon_spm", "VOLTage:DC:RANGe AUTO\r", "invalid SCPI sent by Python; definition uses :RANGe:AUTO ON"),
    ("owon_spm", "RESistance:RANGe 2000.0\r", "Python API coerced `2E3` to float; definition sends the manual token 2E3"),
    ("owon_xdm", "SYSTem:REMote\r", "remote/local is not part of the DMM class; XDM queries work in local"),
    ("owon_xdm", "SYSTem:LOCal\r", "remote/local is not part of the DMM class; XDM queries work in local"),
];

/// TX events of a gzipped capture log (cheap substring filter before JSON parsing).
fn tx_events(path: &Path) -> Vec<serde_json::Value> {
    let f = fs::File::open(path).unwrap();
    BufReader::new(flate2::read::GzDecoder::new(f))
        .lines()
        .map(|l| l.unwrap())
        .filter(|l| l.contains("\"ev\":\"tx\""))
        .map(|l| serde_json::from_str(&l).unwrap())
        .collect()
}

#[test]
fn captured_wire_traffic_is_covered_by_definitions() {
    let cat = catalog();
    let captures = repo().join("captures");
    let mut unmatched: BTreeMap<(String, String), usize> = BTreeMap::new();
    let mut checked = 0usize;
    let mut sessions = 0usize;
    for entry in fs::read_dir(&captures).unwrap() {
        let dir = entry.unwrap().path();
        let session = dir.join("session.json");
        // Direct-call captures have result.json (with the device) instead of session.json.
        let devices: Vec<serde_json::Value> = if session.exists() {
            let s: serde_json::Value = serde_json::from_str(&fs::read_to_string(&session).unwrap()).unwrap();
            s["config"]["devices"].as_array().cloned().unwrap_or_default()
        } else if dir.join("result.json").exists() {
            let r: serde_json::Value = serde_json::from_str(&fs::read_to_string(dir.join("result.json")).unwrap()).unwrap();
            vec![r["device"].clone()]
        } else {
            continue;
        };
        sessions += 1;
        for dev in devices {
            let (id, driver) = (dev["id"].as_str().unwrap(), dev["driver"].as_str().unwrap());
            let log = dir.join(format!("{id}.jsonl.gz"));
            if !log.exists() {
                continue;
            }
            let (_, def) = cat.devices.get(driver).unwrap_or_else(|| panic!("no definition for driver {driver}"));
            let regexes: Vec<regex::Regex> =
                def.commands.values().map(|c| command_wire_regex(c, &def.framing.tx.terminator).unwrap()).collect();
            for ev in tx_events(&log) {
                let bytes: Vec<u8> = (0..ev["hex"].as_str().unwrap().len())
                    .step_by(2)
                    .map(|i| u8::from_str_radix(&ev["hex"].as_str().unwrap()[i..i + 2], 16).unwrap())
                    .collect();
                let wire: String = bytes.iter().map(|&b| b as char).collect();
                checked += 1;
                let allowed = NOT_REPRODUCED.iter().any(|(d, w, _)| *d == driver && *w == wire);
                if !allowed && !regexes.iter().any(|r| r.is_match(&wire)) {
                    *unmatched.entry((driver.to_string(), wire.escape_debug().to_string())).or_default() += 1;
                }
            }
        }
    }
    assert!(sessions >= 3 && checked > 1000, "too little capture data: {sessions} sessions, {checked} commands");
    assert!(
        unmatched.is_empty(),
        "captured commands no definition command can produce:\n{}",
        unmatched.iter().map(|((d, w), n)| format!("  {d}: {w:?} x{n}")).collect::<Vec<_>>().join("\n")
    );
}

/// ai_context/website/instrument-notes.yaml (notes for the public instrument list) names only
/// existing definitions, with known kinds.
#[test]
fn website_notes_refer_to_existing_definitions() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let text = std::fs::read_to_string(root.join("ai_context/website/instrument-notes.yaml")).unwrap();
    let notes: serde_yaml_ng::Value = serde_yaml_ng::from_str(&text).unwrap();
    let cat = bm_devicedef::load_catalog(&root.join("devices")).unwrap();
    let kinds = ["partial_interface", "unsupported", "unverified", "prefer", "known_issue"];
    for (id, list) in notes["families"].as_mapping().unwrap() {
        let id = id.as_str().unwrap();
        assert!(cat.devices.contains_key(id), "instrument-notes.yaml: unknown definition `{id}`");
        for n in list.as_sequence().unwrap() {
            let k = n["kind"].as_str().unwrap_or_default();
            assert!(kinds.contains(&k), "{id}: unknown note kind `{k}`");
            assert!(n["text"].as_str().is_some_and(|t| !t.is_empty()), "{id}: note without text");
        }
    }
}
