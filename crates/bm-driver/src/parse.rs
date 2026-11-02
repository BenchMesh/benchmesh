//! Reply parse pipeline (`commands.<name>.parse`).

use bm_devicedef::{Command, OnEmpty, ParseStep, SampleFormat};
use regex::Regex;
use serde_json::{json, Map, Value};

use crate::framing::preview;
use crate::Error;

/// Working value: raw bytes until a step needs text or a number.
enum Cur {
    Bytes(Vec<u8>),
    Val(Value),
}

impl Cur {
    fn text(self, step: &str) -> Result<String, Error> {
        match self {
            // latin-1: every byte maps to one char, nothing is lost
            Cur::Bytes(b) => Ok(b.iter().map(|&c| c as char).collect()),
            Cur::Val(Value::String(s)) => Ok(s),
            Cur::Val(Value::Number(n)) => Ok(n.to_string()),
            Cur::Val(v) => Err(Error::Parse(format!("`{step}` needs text, got {v}"))),
        }
    }
}

/// Lenient numeric parse shared with the CEL `num()` function: trims, accepts `+1.0E-04`,
/// `05.00`, `+0.0000E-308`; rejects tokens like `OL`.
pub fn lenient_number(s: &str) -> Option<f64> {
    let t = s.trim();
    let n: f64 = t.parse().ok()?;
    n.is_finite().then_some(n)
}

/// Parse one framed reply with the command's pipeline.
pub fn parse_reply(cmd: &Command, reply: &[u8]) -> Result<Value, Error> {
    let binary = cmd.modbus.is_some() || matches!(cmd.parse.first(), Some(ParseStep::Byte | ParseStep::Samples(_)));
    if reply.iter().all(|b| b.is_ascii_whitespace()) && !binary {
        return match cmd.on_empty {
            OnEmpty::Null => Ok(Value::Null),
            OnEmpty::Error => Err(Error::Parse("empty reply".into())),
        };
    }
    if reply.is_empty() {
        return Err(Error::Parse("empty reply".into()));
    }
    let mut cur = Cur::Bytes(reply.to_vec());
    for step in &cmd.parse {
        if let Cur::Val(Value::Null) = cur {
            break;
        }
        cur = apply(step, cur).map_err(|e| match e {
            Error::Parse(m) => Error::Parse(format!("{m} (reply {:?})", preview(reply))),
            other => other,
        })?;
    }
    Ok(match cur {
        Cur::Bytes(b) => Value::String(b.iter().map(|&c| c as char).collect()),
        Cur::Val(v) => v,
    })
}

fn apply(step: &ParseStep, cur: Cur) -> Result<Cur, Error> {
    let s = |v: String| Ok(Cur::Val(Value::String(v)));
    match step {
        ParseStep::Trim => s(cur.text("trim")?.trim().to_string()),
        ParseStep::Unquote => {
            let t = cur.text("unquote")?;
            let u = t.strip_prefix('"').and_then(|x| x.strip_suffix('"')).unwrap_or(&t);
            s(u.to_string())
        }
        ParseStep::Lower => s(cur.text("lower")?.to_lowercase()),
        ParseStep::Upper => s(cur.text("upper")?.to_uppercase()),
        ParseStep::Number => {
            let t = cur.text("number")?;
            let n = lenient_number(&t).ok_or_else(|| Error::Parse(format!("not a number: {t:?}")))?;
            Ok(Cur::Val(json!(n)))
        }
        ParseStep::Text => {
            let n = match cur {
                Cur::Val(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                other => {
                    let t = other.text("text")?;
                    lenient_number(&t).ok_or_else(|| Error::Parse(format!("text: not a number: {t:?}")))?
                }
            };
            // 12 significant digits, as `scale`: no binary floating point noise in the text
            let x: f64 = format!("{n:.11e}").parse().unwrap_or(n);
            s(format!("{x}"))
        }
        ParseStep::Scale(k) => {
            let n = match cur {
                Cur::Val(Value::Number(n)) => n.as_f64().unwrap_or(0.0),
                other => {
                    let t = other.text("scale")?;
                    lenient_number(&t).ok_or_else(|| Error::Parse(format!("scale: not a number: {t:?}")))?
                }
            };
            // 127 × 0.1 is 12.700000000000001 in binary floating point: keep 12 significant digits
            let x: f64 = format!("{:.11e}", n * k).parse().unwrap_or(n * k);
            Ok(Cur::Val(json!(x)))
        }
        ParseStep::Numbers => Ok(Cur::Val(Value::Array(numbers(&cur.text("numbers")?)?))),
        ParseStep::Byte => match cur {
            Cur::Bytes(b) => Ok(Cur::Val(json!(b[0]))),
            Cur::Val(Value::String(t)) => {
                Ok(Cur::Val(json!(t.chars().next().map(|c| c as u32).ok_or_else(|| Error::Parse("byte: empty".into()))?)))
            }
            Cur::Val(v) => Err(Error::Parse(format!("byte: unexpected {v}"))),
        },
        ParseStep::Split { sep, index } => {
            let t = cur.text("split")?;
            let part = t.split(sep.as_str()).nth(*index).ok_or_else(|| Error::Parse(format!("split: no field {index} in {t:?}")))?;
            s(part.to_string())
        }
        ParseStep::Fields { sep, names } => {
            let t = cur.text("fields")?;
            let parts: Vec<&str> = t.split(sep.as_str()).collect();
            if parts.len() < names.len() {
                return Err(Error::Parse(format!("fields: expected {} fields, got {} in {t:?}", names.len(), parts.len())));
            }
            let m: Map<String, Value> = names.iter().zip(parts).map(|(k, v)| (k.clone(), json!(v))).collect();
            Ok(Cur::Val(Value::Object(m)))
        }
        ParseStep::Regex { pattern, group } => {
            let t = cur.text("regex")?;
            let re = Regex::new(pattern).map_err(|e| Error::Parse(e.to_string()))?;
            let c = re.captures(&t).ok_or_else(|| Error::Parse(format!("regex {pattern:?} does not match {t:?}")))?;
            s(c.get(*group).map(|m| m.as_str().to_string()).unwrap_or_default())
        }
        ParseStep::Replace { from, to } => s(cur.text("replace")?.replace(from.as_str(), to)),
        ParseStep::Map(m) => {
            let t = cur.text("map")?;
            match m.get(&t).or_else(|| m.get("_")) {
                Some(v) => s(v.replace("{raw}", &t)),
                None => Err(Error::Parse(format!("map: unexpected value {t:?}"))),
            }
        }
        ParseStep::Bits(spec) => {
            let n = match cur {
                Cur::Val(Value::Number(n)) => n
                    .as_u64()
                    .or_else(|| n.as_f64().filter(|f| *f >= 0.0 && f.fract() == 0.0).map(|f| f as u64))
                    .ok_or_else(|| Error::Parse("bits: not an unsigned integer".into()))?,
                // decimal, or hex with 0x (Siglent `SYST:STAT?` → `0x0224`)
                Cur::Val(Value::String(t)) => {
                    let t = t.trim();
                    match t.strip_prefix("0x").or_else(|| t.strip_prefix("0X")) {
                        Some(h) => u64::from_str_radix(h, 16).ok(),
                        None => t.parse().ok(),
                    }
                    .ok_or_else(|| Error::Parse(format!("bits: not an unsigned integer: {t:?}")))?
                }
                Cur::Bytes(b) => b[0] as u64,
                Cur::Val(v) => return Err(Error::Parse(format!("bits: unexpected {v}"))),
            };
            let re = Regex::new(r"^(\d+)(?:\.\.(\d+))?$").unwrap();
            let mut out = Map::new();
            for (name, range) in spec {
                let c = re.captures(range).ok_or_else(|| Error::Parse(format!("bits: bad range {range}")))?;
                let lo: u32 = c[1].parse().unwrap();
                match c.get(2) {
                    None => out.insert(name.clone(), json!((n >> lo) & 1 == 1)),
                    Some(hi) => {
                        let hi: u32 = hi.as_str().parse().unwrap();
                        let width = hi - lo + 1;
                        out.insert(name.clone(), json!((n >> lo) & ((1u64 << width) - 1)))
                    }
                };
            }
            Ok(Cur::Val(Value::Object(out)))
        }
        ParseStep::Samples(format) => {
            let Cur::Bytes(b) = cur else { return Err(Error::Parse("samples: needs the raw reply (first step)".into())) };
            Ok(Cur::Val(Value::Array(samples(&b, *format)?)))
        }
        ParseStep::Word(field) => {
            let Cur::Bytes(b) = cur else { return Err(Error::Parse("word: needs register data (first step)".into())) };
            Ok(Cur::Val(crate::modbus::decode(&b, field)?))
        }
        ParseStep::Registers(fields) => {
            let Cur::Bytes(b) = cur else { return Err(Error::Parse("registers: needs register data (first step)".into())) };
            let mut out = Map::new();
            for (name, field) in fields {
                out.insert(name.clone(), crate::modbus::decode(&b, field)?);
            }
            Ok(Cur::Val(Value::Object(out)))
        }
        ParseStep::NullIf(values) => {
            let t = cur.text("null_if")?;
            Ok(if values.contains(&t) { Cur::Val(Value::Null) } else { Cur::Val(Value::String(t)) })
        }
    }
}

fn numbers(t: &str) -> Result<Vec<Value>, Error> {
    let mut t = t.trim();
    if let Some(rest) = t.strip_prefix('#') {
        // IEEE 488.2 header `#<n><n digits>` before an ASCII list
        let n = rest.chars().next().and_then(|c| c.to_digit(10)).unwrap_or(0) as usize;
        t = rest.get(1 + n..).ok_or_else(|| Error::Parse("numbers: truncated block header".into()))?;
    }
    t.split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|f| !f.is_empty())
        .map(|f| lenient_number(f).map(|n| json!(n)).ok_or_else(|| Error::Parse(format!("numbers: not a number: {f:?}"))))
        .collect()
}

fn samples(b: &[u8], format: SampleFormat) -> Result<Vec<Value>, Error> {
    use SampleFormat::*;
    let width = if matches!(format, U8 | I8) { 1 } else { 2 };
    if b.len() % width != 0 {
        return Err(Error::Parse(format!("samples: {} bytes is not a multiple of {width}", b.len())));
    }
    Ok(b.chunks_exact(width)
        .map(|c| match format {
            U8 => json!(c[0]),
            I8 => json!(c[0] as i8),
            U16le => json!(u16::from_le_bytes([c[0], c[1]])),
            I16le => json!(i16::from_le_bytes([c[0], c[1]])),
            U16be => json!(u16::from_be_bytes([c[0], c[1]])),
            I16be => json!(i16::from_be_bytes([c[0], c[1]])),
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmd(yaml: &str) -> Command {
        bm_devicedef::from_yaml(yaml).unwrap()
    }

    #[test]
    fn number_and_trim() {
        let c = cmd("send: x\nparse: [trim, number]");
        assert_eq!(parse_reply(&c, b"0.988 ").unwrap(), json!(0.988));
        assert_eq!(parse_reply(&c, b"+0.0000E-308").unwrap(), json!(0.0));
        assert!(parse_reply(&c, b"OL").is_err());
    }

    #[test]
    fn text_writes_a_number_as_its_shortest_decimal() {
        let c = cmd("send: x\nparse: [trim, number, text]");
        assert_eq!(parse_reply(&c, b"1.000000e+01\n").unwrap(), json!("10"));
        assert_eq!(parse_reply(&c, b"1").unwrap(), json!("1"));
        assert_eq!(parse_reply(&c, b"1.000000e-01").unwrap(), json!("0.1"));
        assert_eq!(parse_reply(&c, b"0.001").unwrap(), json!("0.001"));
        let c = cmd("send: x\nparse: [trim, text]");
        assert_eq!(parse_reply(&c, b"+1.0E+03").unwrap(), json!("1000"), "from text too");
        assert!(parse_reply(&c, b"10X").is_err());
    }

    #[test]
    fn map_can_pass_unlisted_values_through() {
        let c = cmd("send: x\nparse: [trim, number, {map: {'10.0': '10', _: '{raw}'}}]");
        assert_eq!(parse_reply(&c, b"+1.00000000E+01").unwrap(), json!("10"));
        assert_eq!(parse_reply(&c, b"+2.00000000E+00").unwrap(), json!("2.0"));
    }

    #[test]
    fn hex_and_scaled_numbers() {
        let c = cmd("send: x\nparse: [trim, {bits: {ch1_cv: '0', out1: '4'}}]");
        assert_eq!(parse_reply(&c, b"0x0011\n").unwrap(), json!({"ch1_cv": true, "out1": true}));
        let c = cmd("send: x\nparse: [trim, number, {bits: {b2: '2'}}]");
        assert_eq!(parse_reply(&c, b"4").unwrap(), json!({"b2": true}), "bits after number (a float)");
        let c = cmd("send: x\nparse: [trim, {scale: 0.1}]");
        assert_eq!(parse_reply(&c, b"127").unwrap(), json!(12.7));
    }

    #[test]
    fn ascii_number_lists() {
        let c = cmd("send: x\nparse: [numbers]");
        // Siglent SSA: trailing comma; tinySA: one value per line; Rigol: IEEE header first
        assert_eq!(parse_reply(&c, b"-80.10,-79.9,1.5E+01,").unwrap(), json!([-80.1, -79.9, 15.0]));
        assert_eq!(parse_reply(&c, b"-9.1e+01\r\n-8.9e+01\r\n").unwrap(), json!([-91.0, -89.0]));
        assert_eq!(parse_reply(&c, b"#9000000011-1.0, -2.0").unwrap(), json!([-1.0, -2.0]));
        assert!(parse_reply(&c, b"1,x,3").is_err());
    }

    #[test]
    fn fields_and_null_if() {
        let c = cmd("send: x\nparse: [trim, {fields: {sep: ' ', names: [func, value]}}]");
        assert_eq!(parse_reply(&c, b"RES OL").unwrap(), json!({"func": "RES", "value": "OL"}));
        let c = cmd("send: x\nparse: [trim, {split: {sep: ' ', index: 1}}, {null_if: [OL]}, number]");
        assert_eq!(parse_reply(&c, b"RES OL").unwrap(), Value::Null);
        assert_eq!(parse_reply(&c, b"VOLT:DC +1.0000E-04").unwrap(), json!(1e-4));
    }

    #[test]
    fn bits_from_binary_byte() {
        let c = cmd("send: x\nreply: {bytes: 1}\nparse: [byte, {bits: {cv: '0', tracking: '2..3', out: '6'}}]");
        assert_eq!(parse_reply(&c, &[0x51]).unwrap(), json!({"cv": true, "tracking": 0, "out": true}));
        assert_eq!(parse_reply(&c, &[0x10]).unwrap(), json!({"cv": false, "tracking": 0, "out": false}));
    }

    #[test]
    fn binary_samples() {
        let c = cmd("send: x\nreply: block\nparse: [{samples: u8}]");
        assert_eq!(parse_reply(&c, &[0, 127, 255]).unwrap(), json!([0, 127, 255]));
        let c = cmd("send: x\nreply: block\nparse: [{samples: i16le}]");
        assert_eq!(parse_reply(&c, &[0x01, 0x00, 0xff, 0xff]).unwrap(), json!([1, -1]));
        let c = cmd("send: x\nreply: block\nparse: [{samples: u16be}]");
        assert_eq!(parse_reply(&c, &[0x01, 0x00]).unwrap(), json!([256]));
        // a byte that looks like whitespace is still a sample, and an odd tail is an error
        let c = cmd("send: x\nreply: block\nparse: [{samples: u8}]");
        assert_eq!(parse_reply(&c, &[32, 10]).unwrap(), json!([32, 10]));
        let c = cmd("send: x\nreply: block\nparse: [{samples: u16le}]");
        assert!(parse_reply(&c, &[1, 2, 3]).is_err());
    }

    #[test]
    fn unquote_map_and_empty() {
        let c = cmd("send: x\nparse: [trim, unquote, {map: {'VOLT AC': VOLT_AC}}]");
        assert_eq!(parse_reply(&c, b"\"VOLT AC\"").unwrap(), json!("VOLT_AC"));
        assert!(parse_reply(&c, b"\"FOO\"").is_err());
        assert!(parse_reply(&c, b"").is_err());
        let c = cmd("send: x\non_empty: 'null'\nparse: [trim]");
        assert_eq!(parse_reply(&c, b"").unwrap(), Value::Null);
    }
}
