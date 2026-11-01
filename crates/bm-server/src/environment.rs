//! Environmental readings of the lab (temperature, humidity, atmospheric pressure) from a sensor
//! a distribution adds (`Extension::environment`; the open-source build has none).
//!
//! While a sensor reports, every recording that is running or paused gets one series per reading
//! it reports (`recordings.rs`), on the same time domain as the instruments; the series cannot be
//! removed (`locked`). Without a sensor nothing is added. `GET /environment` returns the latest
//! readings for the UI.

use std::time::Duration;

use axum::extract::State;
use axum::Json;
use bm_store::now_ms;
use serde_json::{json, Value};

use crate::S;

/// The signal device id of environmental series (a config entry may not use it).
pub const DEVICE_ID: &str = "environment";
/// Their signal class and channel.
pub const CLASS: &str = "ENV";
pub const CHANNEL: i64 = 1;

/// A reading older than this is not shown or recorded (the recordings' `MAX_AGE`).
pub const MAX_AGE: Duration = Duration::from_secs(5);

/// The readings an environmental sensor can report: field, label, unit.
pub const FIELDS: [(&str, &str, &str); 3] = [("TEMP", "Temperature", "°C"), ("HUM", "Humidity", "%RH"), ("PRES", "Pressure", "hPa")];

/// One reading of the sensor.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Reading {
    /// One of [`FIELDS`]' fields.
    pub field: &'static str,
    pub value: f64,
    /// When it was measured (ms since the epoch).
    pub at: i64,
}

/// What the sensor reports right now.
#[derive(Debug, Clone, PartialEq)]
pub struct Snapshot {
    /// What reports, e.g. "BenchMesh Sensors (BME280)".
    pub source: String,
    pub readings: Vec<Reading>,
}

impl Snapshot {
    /// The readings measured within [`MAX_AGE`] before `now` (ms), with a finite value of a known field.
    pub fn fresh(&self, now: i64) -> Vec<Reading> {
        let max = MAX_AGE.as_millis() as i64;
        self.readings.iter().copied().filter(|r| field(r.field).is_some() && r.value.is_finite() && now - r.at <= max).collect()
    }
}

/// A source of environmental readings.
pub trait Environment: Send + Sync {
    /// The latest readings; `None` when no sensor is selected or attached.
    fn snapshot(&self) -> Option<Snapshot>;
}

/// Label and unit of a field.
pub fn field(f: &str) -> Option<(&'static str, &'static str)> {
    FIELDS.iter().find(|(k, _, _)| *k == f).map(|(_, l, u)| (*l, *u))
}

/// Series label of a field, e.g. "Environment · Temperature".
pub fn label(f: &str) -> String {
    format!("Environment · {}", field(f).map_or(f, |(l, _)| l))
}

/// The fresh readings of `env` at `now`, or none (no source, no sensor, nothing fresh).
pub fn fresh(env: Option<&dyn Environment>, now: i64) -> Option<(String, Vec<Reading>)> {
    let snap = env?.snapshot()?;
    let r = snap.fresh(now);
    (!r.is_empty()).then_some((snap.source, r))
}

/// `GET /environment`: `{present, source, readings: [{field, label, unit, value, at}]}`.
pub async fn get_environment(State(s): State<S>) -> Json<Value> {
    Json(snapshot_json(s.extensions.environment().as_deref(), now_ms()))
}

pub fn snapshot_json(env: Option<&dyn Environment>, now: i64) -> Value {
    match fresh(env, now) {
        None => json!({"present": false, "source": null, "readings": []}),
        Some((source, readings)) => json!({
            "present": true,
            "source": source,
            "readings": readings.iter().map(|r| {
                let (label, unit) = field(r.field).unwrap_or(("", ""));
                json!({"field": r.field, "label": label, "unit": unit, "value": r.value, "at": r.at})
            }).collect::<Vec<_>>(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed(Option<Snapshot>);
    impl Environment for Fixed {
        fn snapshot(&self) -> Option<Snapshot> {
            self.0.clone()
        }
    }

    fn snap(at: i64) -> Snapshot {
        Snapshot {
            source: "BenchMesh Sensors (BME280)".into(),
            readings: vec![
                Reading { field: "TEMP", value: 23.4, at },
                Reading { field: "HUM", value: f64::NAN, at },
                Reading { field: "PRES", value: 1013.25, at: at - 6000 },
                Reading { field: "WIND", value: 1.0, at },
            ],
        }
    }

    #[test]
    fn only_fresh_finite_known_readings_count() {
        let r = snap(10_000).fresh(10_500);
        assert_eq!(r, vec![Reading { field: "TEMP", value: 23.4, at: 10_000 }]);
    }

    #[test]
    fn json_says_present_only_with_a_fresh_reading() {
        assert_eq!(snapshot_json(None, 0)["present"], false);
        assert_eq!(snapshot_json(Some(&Fixed(None)), 0)["present"], false);
        assert_eq!(snapshot_json(Some(&Fixed(Some(snap(0)))), 60_000)["present"], false, "stale");
        let v = snapshot_json(Some(&Fixed(Some(snap(10_000)))), 10_000);
        assert_eq!(v["present"], true);
        assert_eq!(v["source"], "BenchMesh Sensors (BME280)");
        assert_eq!(v["readings"][0], json!({"field": "TEMP", "label": "Temperature", "unit": "°C", "value": 23.4, "at": 10_000}));
        assert_eq!(v["readings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn labels_name_the_reading() {
        assert_eq!(label("PRES"), "Environment · Pressure");
        assert_eq!(field("HUM"), Some(("Humidity", "%RH")));
        assert_eq!(field("X"), None);
    }
}
