//! Recordings v2 (contract: `ai_context/migration/RECORDINGS.md`): the `/recordings` API,
//! the `/ws/recordings` stream and ONE sampler for all recordings.
//!
//! The sampler ticks on whole seconds of the wall clock (`t` = multiple of 1000 ms); a
//! recording samples at `t` iff it is `recording` and `(t / 1000) % interval_s == 0`, so all
//! recordings share one time domain. Values come from the live registry (no extra instrument
//! traffic); a value older than [`MAX_AGE`] or of a disconnected device is a gap, never a
//! repeated stale value. One tick is written in one transaction.
//!
//! An environmental sensor (`environment.rs`) adds its readings to every running or paused
//! recording while it reports, also to one already running when it appears; those series are
//! `locked` (cannot be removed).

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::ws::{Message, WebSocket, WebSocketUpgrade};
use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use bm_devicedef::ValueKind;
use bm_store::{now_ms, Recording, Series, Signal, State as RecState, Store, StoreError};
use serde_json::{json, Map, Value};
use tokio::sync::broadcast;

use crate::config::str_field;
use crate::devices::Devices;
use crate::environment::{self, Environment};
use crate::{detail, S};

/// Registry values older than this are not recorded.
pub const MAX_AGE: Duration = Duration::from_secs(5);

pub struct Recordings {
    pub store: Arc<Store>,
    devices: Arc<Devices>,
    /// The lab's environmental sensor, if the distribution has one.
    environment: Option<Arc<dyn Environment>>,
    /// WebSocket messages (JSON text) for every `/ws/recordings` client.
    tx: broadcast::Sender<String>,
    sampler: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Recordings {
    /// Stops recordings left running by a previous run and starts the sampler (must be
    /// called inside a tokio runtime).
    pub fn new(store: Arc<Store>, devices: Arc<Devices>, environment: Option<Arc<dyn Environment>>) -> Arc<Self> {
        match store.close_orphans() {
            Ok(0) => {}
            Ok(n) => eprintln!("stopped {n} recording(s) left running by a previous run"),
            Err(e) => eprintln!("recordings: {e}"),
        }
        let (tx, _) = broadcast::channel(1024);
        let rec = Arc::new(Recordings { store, devices, environment, tx, sampler: Mutex::new(None) });
        let task = tokio::spawn(sampler(Arc::downgrade(&rec)));
        *rec.sampler.lock().unwrap() = Some(task);
        rec
    }

    /// Stop the sampler (server shutdown). Recordings stay `recording`; the next start
    /// stops them at their last sample.
    pub fn shutdown(&self) {
        if let Some(t) = self.sampler.lock().unwrap().take() {
            t.abort();
        }
    }

    fn publish(&self, msg: Value) {
        let _ = self.tx.send(msg.to_string());
    }

    /// One sampler tick at `t` (ms, multiple of 1000).
    async fn tick(&self, t: i64) {
        let env = environment::fresh(self.environment.as_deref(), t).map(|(_, r)| r).unwrap_or_default();
        self.add_environment(&env, None).await;
        let store = self.store.clone();
        let due = match tokio::task::spawn_blocking(move || store.due_series(t)).await {
            Ok(Ok(d)) => d,
            Ok(Err(e)) => return eprintln!("recordings: tick {t}: {e}"),
            Err(_) => return,
        };
        if due.is_empty() {
            return;
        }
        let mut values: BTreeMap<i64, Map<String, Value>> = BTreeMap::new();
        let mut samples = Vec::new();
        {
            let reg = self.devices.registry.lock().unwrap();
            for d in &due {
                let s = &d.signal;
                let v = if is_environment(s) {
                    env.iter().find(|r| r.channel == s.channel && r.field == s.field).map(|r| r.value)
                } else {
                    u8::try_from(s.channel)
                    .ok()
                    .and_then(|ch| reg.fresh_value(&s.device_id, &s.class, ch, &s.field, MAX_AGE))
                        .and_then(numeric)
                };
                if let Some(v) = v {
                    samples.push((d.series_id, v));
                }
                values.entry(d.recording_id).or_default().insert(d.series_id.to_string(), json!(v));
            }
        }
        let store = self.store.clone();
        match tokio::task::spawn_blocking(move || store.insert_samples(t, &samples)).await {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => eprintln!("recordings: tick {t}: {e}"),
            Err(_) => return,
        }
        for (rid, v) in values {
            self.publish(json!({"type": "samples", "recording_id": rid, "t": t, "values": v}));
        }
    }

    /// Start a recording with these series (`device_id`, `class`, `channel`, `field`,
    /// `label?` objects) - for automation runs. Announced like an API-created one.
    pub async fn start_with(&self, name: &str, interval_s: i64, series: &[Map<String, Value>]) -> Result<i64, String> {
        let mut signals = Vec::new();
        for b in series {
            let (signal, default_label) = self.validate_signal(b).map_err(|e| e.1)?;
            let label = b.get("label").and_then(|v| v.as_str()).filter(|l| !l.trim().is_empty()).map(str::to_string).unwrap_or(default_label);
            signals.push((signal, label));
        }
        let store = self.store.clone();
        let name = name.to_string();
        let id = tokio::task::spawn_blocking(move || -> Result<i64, StoreError> {
            let t = now_ms();
            let r = store.create_recording(&name, "", interval_s, t)?;
            for (signal, label) in &signals {
                store.add_series(r.id, signal, label, "", t)?;
            }
            Ok(r.id)
        })
        .await
        .map_err(|e| e.to_string())?
        .map_err(|e| e.to_string())?;
        self.add_environment_now(id).await;
        self.announce_id(id).await;
        Ok(id)
    }

    /// Adds the readings the sensor reports now to a new recording (before its first tick).
    async fn add_environment_now(&self, id: i64) {
        if let Some((_, env)) = environment::fresh(self.environment.as_deref(), now_ms()) {
            self.add_environment(&env, Some(id)).await;
        }
    }

    /// Gives every running or paused recording (or only `only`) a series for each of these
    /// readings it lacks - ending its series of the same sensor (channel) and quantity when the
    /// field, label or unit changed (°C to °F, relative humidity to dew point, another chip at
    /// the address) - and announces the recordings changed.
    async fn add_environment(&self, env: &[environment::Reading], only: Option<i64>) {
        if env.is_empty() {
            return;
        }
        let env = env.to_vec();
        let store = self.store.clone();
        let changed = tokio::task::spawn_blocking(move || -> Result<Vec<i64>, StoreError> {
            let t = now_ms();
            let mut changed = Vec::new();
            for r in store.list_recordings()? {
                if r.state == RecState::Stopped || only.is_some_and(|id| id != r.id) {
                    continue;
                }
                let series = store.series_of(r.id)?;
                let mut added = false;
                for e in &env {
                    let label = environment::label(e);
                    let current = series.iter().find(|s| s.removed_at.is_none() && is_environment(&s.signal) && s.signal.channel == e.channel && environment::quantity(&s.signal.field) == environment::quantity(e.field));
                    match current {
                        Some(s) if s.signal.field == e.field && s.unit == e.unit && s.label == label => continue,
                        Some(s) => {
                            store.end_series(r.id, s.id, t)?;
                        }
                        None => {}
                    }
                    let signal = Signal { device_id: environment::DEVICE_ID.into(), class: environment::CLASS.into(), channel: e.channel, field: e.field.to_string() };
                    store.add_series(r.id, &signal, &label, e.unit, t)?;
                    added = true;
                }
                if added {
                    changed.push(r.id);
                }
            }
            Ok(changed)
        })
        .await;
        match changed {
            Ok(Ok(ids)) => {
                for id in ids {
                    self.announce_id(id).await;
                }
            }
            Ok(Err(e)) => eprintln!("recordings: environmental readings: {e}"),
            Err(_) => {}
        }
    }

    /// Stop a recording (automation runs); a recording already stopped is fine.
    pub async fn stop_id(&self, id: i64) -> Result<(), String> {
        let store = self.store.clone();
        let r = tokio::task::spawn_blocking(move || store.stop(id, now_ms())).await.map_err(|e| e.to_string())?;
        match r {
            Ok(_) | Err(StoreError::Conflict(_)) => {
                self.announce_id(id).await;
                Ok(())
            }
            Err(e) => Err(e.to_string()),
        }
    }

    async fn announce_id(&self, id: i64) {
        let store = self.store.clone();
        if let Ok(Ok(v)) = tokio::task::spawn_blocking(move || store.recording(id).and_then(|r| recording_json(&store, &r))).await {
            self.publish(json!({"type": "recording", "recording": v}));
        }
    }

    /// Validate a series request against the configuration and the class contract.
    /// Returns the signal and the default label.
    fn validate_signal(&self, b: &Map<String, Value>) -> Result<(Signal, String), ApiError> {
        let s = |k: &str| b.get(k).and_then(|v| v.as_str()).map(str::to_string).ok_or_else(|| invalid(format!("`{k}` (string) is required")));
        let device_id = s("device_id")?;
        let class = s("class")?;
        let field = s("field")?;
        let channel = b.get("channel").and_then(|v| v.as_i64()).ok_or_else(|| invalid("`channel` (integer) is required"))?;
        let (def, _, entry) = self.devices.lookup(&device_id).ok_or_else(|| invalid(format!("unknown device {device_id:?}")))?;
        let binding = def.classes.get(&class).ok_or_else(|| invalid(format!("device {device_id} has no class {class:?}")))?;
        let channels = self.devices.model_of(&device_id, &def, &entry).and_then(|m| m.channels.get(&class).copied()).unwrap_or(binding.channels);
        if !(1..=channels as i64).contains(&channel) {
            return Err(invalid(format!("{device_id} {class} has channels 1-{channels}, not {channel}")));
        }
        let numeric_field = self
            .devices
            .catalog
            .classes
            .get(&class)
            .and_then(|c| c.status.get(&field))
            .is_some_and(|f| matches!(f.kind, ValueKind::Si | ValueKind::Number | ValueKind::Integer));
        if !numeric_field {
            return Err(invalid(format!("{field:?} is not a numeric status key of class {class}")));
        }
        let name = str_field(&entry, "name").unwrap_or(&device_id).to_string();
        let label = default_label(&name, channels, channel, &field);
        Ok((Signal { device_id, class, channel, field }, label))
    }
}

/// A series of the environmental sensor (added by the server, cannot be removed).
fn is_environment(s: &Signal) -> bool {
    s.device_id == environment::DEVICE_ID
}

/// Label of a series added without one. Multi-channel classes name the channel: two scope
/// `FREQ` columns must be distinguishable in an export.
fn default_label(device_name: &str, channels: u8, channel: i64, field: &str) -> String {
    if channels > 1 { format!("{device_name} · CH{channel} · {field}") } else { format!("{device_name} · {field}") }
}

/// Registry value → sample: SI dict → its `val`; number → itself; anything else → none.
fn numeric(v: &Value) -> Option<f64> {
    let n = match v {
        Value::Number(n) => n.as_f64(),
        Value::Object(o) if o.contains_key("si") => match o.get("val") {
            Some(Value::String(s)) => s.trim().parse().ok(),
            Some(Value::Number(n)) => n.as_f64(),
            _ => None,
        },
        _ => None,
    };
    n.filter(|x| x.is_finite())
}

/// Sleep until the next whole second of the wall clock, tick, repeat. Ends when the
/// recordings are dropped.
async fn sampler(rec: Weak<Recordings>) {
    let mut last = 0i64;
    loop {
        let now = now_ms();
        let next = ((now.div_euclid(1000) + 1) * 1000).max(last + 1000);
        tokio::time::sleep(Duration::from_millis((next - now).max(0) as u64)).await;
        let Some(rec) = rec.upgrade() else { return };
        rec.tick(next).await;
        last = next;
    }
}

// ---- JSON ----------------------------------------------------------------------------

fn series_json(s: &Series) -> Value {
    json!({"id": s.id, "device_id": s.signal.device_id, "class": s.signal.class, "channel": s.signal.channel,
           "field": s.signal.field, "label": s.label, "unit": s.unit, "added_at": s.added_at, "removed_at": s.removed_at,
           "locked": is_environment(&s.signal)})
}

fn recording_json(store: &Store, r: &Recording) -> Result<Value, StoreError> {
    let series = store.series_of(r.id)?;
    let (samples, last_t) = store.sample_stats(r.id)?;
    Ok(json!({
        "id": r.id, "name": r.name, "description": r.description, "interval_s": r.interval_s, "state": r.state.as_str(),
        "started_at": r.started_at, "stopped_at": r.stopped_at, "paused_at": r.paused_at,
        "samples": samples, "last_t": last_t, "series": series.iter().map(series_json).collect::<Vec<_>>(),
    }))
}

fn all_recordings_json(store: &Store) -> Result<Value, StoreError> {
    let list = store.list_recordings()?;
    Ok(Value::Array(list.iter().map(|r| recording_json(store, r)).collect::<Result<Vec<_>, _>>()?))
}

// ---- errors --------------------------------------------------------------------------

pub struct ApiError(StatusCode, String);

fn invalid(m: impl Into<String>) -> ApiError {
    ApiError(StatusCode::UNPROCESSABLE_ENTITY, m.into())
}

impl From<StoreError> for ApiError {
    fn from(e: StoreError) -> Self {
        let code = match &e {
            StoreError::DuplicateName(_) | StoreError::Conflict(_) => StatusCode::CONFLICT,
            StoreError::NotFound(_) => StatusCode::NOT_FOUND,
            StoreError::Db(m) => {
                eprintln!("recordings: {m}");
                StatusCode::INTERNAL_SERVER_ERROR
            }
        };
        ApiError(code, e.to_string())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        detail(self.0, self.1)
    }
}

/// Run store work off the async workers.
async fn db<T: Send + 'static>(s: &S, f: impl FnOnce(&Store) -> Result<T, StoreError> + Send + 'static) -> Result<T, ApiError> {
    let store = s.recordings.store.clone();
    match tokio::task::spawn_blocking(move || f(&store)).await {
        Ok(r) => Ok(r?),
        Err(e) => Err(ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string())),
    }
}

fn object(body: &Bytes) -> Result<Map<String, Value>, ApiError> {
    match serde_json::from_slice::<Value>(body) {
        Ok(Value::Object(m)) => Ok(m),
        Ok(_) => Err(invalid("body must be a JSON object")),
        Err(e) => Err(invalid(format!("invalid JSON: {e}"))),
    }
}

fn rec_id(id: &str) -> Result<i64, ApiError> {
    id.parse().map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("Recording {id} not found")))
}

/// Recording name: 1-100 characters after trimming.
fn valid_name(v: &Value) -> Result<String, ApiError> {
    let n = v.as_str().map(str::trim).ok_or_else(|| invalid("`name` must be a string"))?;
    if n.is_empty() || n.chars().count() > 100 {
        return Err(invalid("`name` must be 1-100 characters"));
    }
    Ok(n.to_string())
}

fn opt_string(b: &Map<String, Value>, k: &str) -> Result<Option<String>, ApiError> {
    match b.get(k) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(invalid(format!("`{k}` must be a string"))),
    }
}

// ---- routes --------------------------------------------------------------------------

pub fn routes() -> Router<S> {
    Router::new()
        .route("/recordings", get(list).post(create))
        .route("/recordings/{id}", get(one).patch(update).delete(remove))
        .route("/recordings/{id}/series", post(add_series))
        .route("/recordings/{id}/series/{sid}", delete(remove_series))
        .route("/recordings/{id}/pause", post(pause))
        .route("/recordings/{id}/resume", post(resume))
        .route("/recordings/{id}/stop", post(stop))
        .route("/recordings/{id}/data", get(data))
        .route("/recordings/{id}/export", get(export))
        .route("/ws/recordings", get(ws))
}

async fn list(State(s): State<S>) -> Result<Json<Value>, ApiError> {
    Ok(Json(db(&s, all_recordings_json).await?))
}

async fn one(State(s): State<S>, Path(id): Path<String>) -> Result<Json<Value>, ApiError> {
    let id = rec_id(&id)?;
    Ok(Json(db(&s, move |st| recording_json(st, &st.recording(id)?)).await?))
}

/// Announce a changed recording on the WebSocket; returns its JSON.
async fn announce(s: &S, id: i64) -> Result<Value, ApiError> {
    let v = db(s, move |st| recording_json(st, &st.recording(id)?)).await?;
    s.recordings.publish(json!({"type": "recording", "recording": v}));
    Ok(v)
}

/// Respond with the changed recording (and announce it).
async fn changed(s: &S, id: i64, code: StatusCode) -> Result<Response, ApiError> {
    Ok((code, Json(announce(s, id).await?)).into_response())
}

async fn create(State(s): State<S>, body: Bytes) -> Result<Response, ApiError> {
    let b = object(&body)?;
    let name = valid_name(b.get("name").unwrap_or(&Value::Null))?;
    let description = opt_string(&b, "description")?.unwrap_or_default();
    let interval = b
        .get("interval_s")
        .and_then(|v| v.as_i64())
        .filter(|i| (1..=3600).contains(i))
        .ok_or_else(|| invalid("`interval_s` must be an integer 1-3600"))?;
    let r = db(&s, move |st| st.create_recording(&name, &description, interval, now_ms())).await?;
    s.recordings.add_environment_now(r.id).await;
    changed(&s, r.id, StatusCode::CREATED).await
}

async fn update(State(s): State<S>, Path(id): Path<String>, body: Bytes) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    let b = object(&body)?;
    let name = match b.get("name") {
        None | Some(Value::Null) => None,
        Some(v) => Some(valid_name(v)?),
    };
    let description = opt_string(&b, "description")?;
    db(&s, move |st| st.update_recording(id, name.as_deref(), description.as_deref())).await?;
    changed(&s, id, StatusCode::OK).await
}

async fn add_series(State(s): State<S>, Path(id): Path<String>, body: Bytes) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    db(&s, move |st| st.recording(id)).await?;
    let b = object(&body)?;
    let (signal, default_label) = s.recordings.validate_signal(&b)?;
    let label = opt_string(&b, "label")?.filter(|l| !l.trim().is_empty()).unwrap_or(default_label);
    let unit = opt_string(&b, "unit")?.unwrap_or_default();
    let series = db(&s, move |st| st.add_series(id, &signal, &label, &unit, now_ms())).await?;
    announce(&s, id).await?;
    Ok((StatusCode::CREATED, Json(series_json(&series))).into_response())
}

async fn remove_series(State(s): State<S>, Path((id, sid)): Path<(String, String)>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    let sid: i64 = sid.parse().map_err(|_| ApiError(StatusCode::NOT_FOUND, format!("Series {sid} not found")))?;
    let series = db(&s, move |st| st.series_of(id)).await?;
    if series.iter().any(|x| x.id == sid && is_environment(&x.signal)) {
        return Err(ApiError(StatusCode::CONFLICT, "Environmental readings stay in every recording".into()));
    }
    db(&s, move |st| st.end_series(id, sid, now_ms())).await?;
    announce(&s, id).await?;
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn pause(State(s): State<S>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    db(&s, move |st| st.pause(id, now_ms())).await?;
    changed(&s, id, StatusCode::OK).await
}

async fn resume(State(s): State<S>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    db(&s, move |st| st.resume(id)).await?;
    changed(&s, id, StatusCode::OK).await
}

async fn stop(State(s): State<S>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    db(&s, move |st| st.stop(id, now_ms())).await?;
    changed(&s, id, StatusCode::OK).await
}

async fn remove(State(s): State<S>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    db(&s, move |st| st.delete_recording(id)).await?;
    s.recordings.publish(json!({"type": "deleted", "recording_id": id}));
    Ok(StatusCode::NO_CONTENT.into_response())
}

async fn data(State(s): State<S>, Path(id): Path<String>, Query(q): Query<HashMap<String, String>>) -> Result<Json<Value>, ApiError> {
    let id = rec_id(&id)?;
    let num = |k: &str| -> Result<Option<i64>, ApiError> {
        q.get(k).map(|v| v.trim().parse::<i64>().map_err(|_| invalid(format!("`{k}` must be an integer")))).transpose()
    };
    let (from, to) = (num("from")?, num("to")?);
    let max_points = num("max_points")?.unwrap_or(2000);
    if !(10..=20_000).contains(&max_points) {
        return Err(invalid("`max_points` must be 10-20000"));
    }
    if let (Some(f), Some(t)) = (from, to) {
        if f > t {
            return Err(invalid("`from` must not be after `to`"));
        }
    }
    let v = db(&s, move |st| {
        let r = st.recording(id)?;
        let from = from.unwrap_or(r.started_at);
        let to = to.unwrap_or_else(|| r.stopped_at.unwrap_or_else(now_ms));
        let mut series = Vec::new();
        for x in st.series_of(id)? {
            let p = st.range(x.id, from, to, max_points as usize)?;
            series.push(json!({"id": x.id, "t": p.t, "v": p.v}));
        }
        Ok(json!({"recording_id": id, "from": from, "to": to, "series": series}))
    })
    .await?;
    Ok(Json(v))
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn utc(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .map(|d| d.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
        .unwrap_or_default()
}

async fn export(State(s): State<S>, Path(id): Path<String>) -> Result<Response, ApiError> {
    let id = rec_id(&id)?;
    let (r, (series, rows)) = db(&s, move |st| Ok((st.recording(id)?, st.export_rows(id)?))).await?;
    let mut out = String::from("time_utc,time_ms");
    for x in &series {
        let col = if x.unit.is_empty() { x.label.clone() } else { format!("{} [{}]", x.label, x.unit) };
        out.push(',');
        out.push_str(&csv_field(&col));
    }
    out.push_str("\r\n");
    for (t, vals) in rows {
        out.push_str(&format!("{},{t}", utc(t)));
        for v in vals {
            out.push(',');
            if let Some(v) = v {
                out.push_str(&v.to_string());
            }
        }
        out.push_str("\r\n");
    }
    let filename = format!("{}.csv", r.name.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-' && c != '.', "_"));
    Ok((
        [(header::CONTENT_TYPE, "text/csv; charset=utf-8".to_string()),
         (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{filename}\""))],
        out,
    )
        .into_response())
}

// ---- WebSocket -----------------------------------------------------------------------

async fn ws(State(s): State<S>, p: crate::remote::Principal, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |socket| live(s, p, socket))
}

async fn snapshot(s: &S) -> Option<String> {
    db(s, all_recordings_json).await.ok().map(|list| json!({"type": "snapshot", "recordings": list}).to_string())
}

async fn live(s: S, p: crate::remote::Principal, mut socket: WebSocket) {
    let mut rx = s.recordings.tx.subscribe(); // before the snapshot: no change is missed
    let Some(first) = snapshot(&s).await else { return };
    if socket.send(Message::Text(first.into())).await.is_err() {
        return;
    }
    loop {
        tokio::select! {
            _ = p.revoked() => {
                let _ = socket.send(Message::Close(None)).await;
                return;
            }
            msg = socket.recv() => match msg {
                Some(Ok(Message::Close(_))) | None | Some(Err(_)) => return,
                _ => {}
            },
            m = rx.recv() => {
                let text = match m {
                    Ok(t) => t,
                    // too slow a client: resynchronise with a fresh snapshot
                    Err(broadcast::error::RecvError::Lagged(_)) => match snapshot(&s).await { Some(t) => t, None => return },
                    Err(broadcast::error::RecvError::Closed) => return,
                };
                if socket.send(Message::Text(text.into())).await.is_err() {
                    return;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn default_labels_name_the_channel_of_multi_channel_classes() {
        assert_eq!(default_label("Tenma", 1, 1, "VOUT"), "Tenma · VOUT");
        assert_eq!(default_label("Rigol DHO804", 4, 2, "FREQ"), "Rigol DHO804 · CH2 · FREQ");
    }

    use super::*;

    #[test]
    fn numeric_values_from_the_registry() {
        let si = json!({"si": {"number": "4.94"}, "sci": "4.94000E+00", "val": "+4.9400E+00"});
        assert_eq!(numeric(&si), Some(4.94));
        assert_eq!(numeric(&json!(12)), Some(12.0));
        assert_eq!(numeric(&json!(0.5)), Some(0.5));
        for v in [json!(null), json!("ON"), json!(true), json!([1, 2]), json!({"si": {}, "val": "OL"})] {
            assert_eq!(numeric(&v), None, "{v}");
        }
    }

    #[test]
    fn csv_fields_and_times() {
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("say \"hi\""), "\"say \"\"hi\"\"\"");
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(utc(1_792_236_034_000), "2026-10-17T11:20:34.000Z");
    }
}
