//! Environmental readings of the lab (temperature, humidity, atmospheric pressure) from a sensor
//! a distribution adds (`Extension::environment`; the open-source build has none).
//!
//! While a sensor reports, every recording that is running or paused gets one series per reading
//! it reports (`recordings.rs`) - per sensor chip and quantity, the chip being the series'
//! channel -, on the same time domain as the instruments, with the reading's label and unit (the
//! units the sensor's settings choose); the series cannot be removed (`locked`). When a reading's
//! unit or label changes, its series ends and a new one starts.
//! Without a sensor nothing is added. `GET /environment` returns the latest readings for the UI.

use std::time::Duration;

use axum::extract::State;
use axum::Json;
use bm_store::now_ms;
use serde_json::{json, Value};

use crate::S;

/// The signal device id of environmental series (a config entry may not use it).
pub const DEVICE_ID: &str = "environment";
/// Their signal class (the channel: which sensor, see [`Reading::channel`]).
pub const CLASS: &str = "ENV";

/// A reading older than this is not shown or recorded (the recordings' `MAX_AGE`).
pub const MAX_AGE: Duration = Duration::from_secs(5);

/// The readings an environmental sensor can report: temperature, relative humidity, dew point,
/// absolute humidity, pressure.
pub const FIELDS: [&str; 5] = ["TEMP", "HUM", "DEW", "AH", "PRES"];

/// One reading of the sensor.
#[derive(Debug, Clone, PartialEq)]
pub struct Reading {
    /// One of [`FIELDS`].
    pub field: &'static str,
    /// Which sensor measured it (1 or more, stable for the sensor: the official build uses its
    /// I2C address); each sensor's readings are their own series.
    pub channel: i64,
    /// e.g. "Temperature (SHT4x 0x44)", "Dew point (BME280 0x76)"
    pub label: String,
    /// e.g. "°C", "°F", "%RH", "g/m³", "hPa", "inHg"
    pub unit: &'static str,
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
    /// The readings measured within [`MAX_AGE`] before `now` (ms), with a finite value of a known
    /// field and a label and unit.
    pub fn fresh(&self, now: i64) -> Vec<Reading> {
        let max = MAX_AGE.as_millis() as i64;
        self.readings
            .iter()
            .filter(|r| FIELDS.contains(&r.field) && r.channel >= 1 && !r.label.is_empty() && !r.unit.is_empty() && r.value.is_finite() && now - r.at <= max)
            .cloned()
            .collect()
    }
}

/// A source of environmental readings.
pub trait Environment: Send + Sync {
    /// The latest readings; `None` when no sensor is selected or attached.
    fn snapshot(&self) -> Option<Snapshot>;
}

/// What a field measures: relative and absolute humidity are one quantity given in different
/// ways (a recording has one series of it at a time); the dew point is a reading of its own, next
/// to either.
pub fn quantity(field: &str) -> &str {
    match field {
        "HUM" | "AH" => "HUM",
        f => f,
    }
}

/// Series label of a reading, e.g. "Environment · Temperature".
pub fn label(r: &Reading) -> String {
    format!("Environment · {}", r.label)
}

/// The fresh readings of `env` at `now`, or none (no source, no sensor, nothing fresh).
pub fn fresh(env: Option<&dyn Environment>, now: i64) -> Option<(String, Vec<Reading>)> {
    let snap = env?.snapshot()?;
    let r = snap.fresh(now);
    (!r.is_empty()).then_some((snap.source, r))
}

/// `GET /environment`: `{present, source, readings: [{field, channel, label, unit, value, at}]}`.
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
                json!({"field": r.field, "channel": r.channel, "label": r.label, "unit": r.unit, "value": r.value, "at": r.at})
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

    fn r(field: &'static str, label: &str, unit: &'static str, value: f64, at: i64) -> Reading {
        Reading { field, channel: 0x44, label: label.into(), unit, value, at }
    }

    fn snap(at: i64) -> Snapshot {
        Snapshot {
            source: "BenchMesh Sensors (BME280)".into(),
            readings: vec![
                r("TEMP", "Temperature", "°F", 74.1, at),
                r("HUM", "Humidity", "%RH", f64::NAN, at),
                r("PRES", "Pressure", "hPa", 1013.25, at - 6000),
                r("WIND", "Wind", "m/s", 1.0, at),
                r("DEW", "Dew point", "", 9.0, at),
                Reading { channel: 0, ..r("TEMP", "Temperature", "°C", 20.0, at) },
            ],
        }
    }

    #[test]
    fn only_fresh_finite_known_readings_with_a_unit_count() {
        let r = snap(10_000).fresh(10_500);
        assert_eq!(r, vec![super::Reading { field: "TEMP", channel: 0x44, label: "Temperature".into(), unit: "°F", value: 74.1, at: 10_000 }]);
    }

    #[test]
    fn json_says_present_only_with_a_fresh_reading() {
        assert_eq!(snapshot_json(None, 0)["present"], false);
        assert_eq!(snapshot_json(Some(&Fixed(None)), 0)["present"], false);
        assert_eq!(snapshot_json(Some(&Fixed(Some(snap(0)))), 60_000)["present"], false, "stale");
        let v = snapshot_json(Some(&Fixed(Some(snap(10_000)))), 10_000);
        assert_eq!(v["present"], true);
        assert_eq!(v["source"], "BenchMesh Sensors (BME280)");
        assert_eq!(v["readings"][0], json!({"field": "TEMP", "channel": 0x44, "label": "Temperature", "unit": "°F", "value": 74.1, "at": 10_000}));
        assert_eq!(v["readings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn labels_name_the_reading() {
        assert_eq!(label(&r("DEW", "Dew point", "°C", 9.0, 0)), "Environment · Dew point");
        assert_eq!((quantity("DEW"), quantity("AH"), quantity("HUM"), quantity("PRES")), ("DEW", "HUM", "HUM", "PRES"));
    }
}
