# Recordings - design and API contract (v2, 2026-10-17)

Purpose (the reason BenchMesh exists): put readings from all instruments on **one common time
domain**. Several recordings run in parallel; readings are added to / removed from a running
recording at any time; recordings may run for hours.

## Model

- **Recording**: name (unique), description, `interval_s` (whole seconds, 1-3600), state
  `recording` | `paused` | `stopped`, started/stopped times. Created empty.
- **Series**: one reading in one recording - a *signal* `{device_id, class, channel, field}` where
  `field` is a numeric status key of the class contract (`si`, `number` or `integer` type, e.g. PSU
  `VOUT`, DMM `MEAS`, ELL `VIN`, AWG `FREQ`, OSC `VPP`). Has `label` and `unit` (supplied by the
  client, which knows how it displays the reading), `added_at`, `removed_at` (null while active).
  The same signal may be in several recordings (stored per recording). Removing a series keeps its
  data; adding the same signal again creates a new series.
- **Sample**: `(series_id, t, v)` - `t` integer milliseconds since the Unix epoch (UTC), `v` a
  finite f64.

## Common time domain (the sampler)

- One sampler for all recordings, ticking on whole seconds: tick times `t` are multiples of 1000 ms.
- A recording samples at tick `t` iff its state is `recording` and `(t / 1000) % interval_s == 0`.
  So all recordings share timestamps (a 1 s and a 5 s recording coincide every 5 s).
- Values come from the **live registry** (the status the devices are polled for anyway): no extra
  instrument traffic, however many recordings. SI dict → its `val` as f64; number → f64; anything
  else (null, string, bool, list) → no sample.
- Freshness: if the registry has had no status update for that device/class/channel for more than
  5 s (or the device is disconnected), the series gets **no sample** at that tick (a gap, never a
  repeated stale value).
- All samples of one tick are written in one transaction. SQLite in WAL mode.
- On server start, recordings still `recording`/`paused` from a previous run are set `stopped`
  (`stopped_at` = last sample time, else start time).

## Environmental readings

A distribution may add an environmental sensor (`Extension::environment`, `crates/bm-server/src/environment.rs`;
the official build: BenchMesh Sensors I2C, sensor chips on a CH341A USB-I2C adapter). While it reports (a reading
newer than 5 s), every recording that is `recording` or `paused` has one series per reading it reports - per
sensor and quantity, so two temperature sensors give two columns - signal `{device_id: "environment", class: "ENV",
channel: <the sensor, 1 or more; the official build: its I2C address>, field: TEMP | HUM | DEW | AH | PRES}`
(temperature, relative humidity, dew point, absolute humidity, pressure), with the label and unit the sensor gives
the reading (its units setting: e.g. `Environment · Temperature (SHT4x 0x44)` in `°C` / `°F` / `K`,
`Environment · Dew point (SHT4x 0x44)` in `°F`, `Pressure (BMP280 0x76)` in `hPa` / `inHg` / ...) - added when the recording is created and, for a sensor that appears later, at the next tick.
When a reading's unit or label changes (the user picks other units, another chip at the address; relative and
absolute humidity count as one reading, the dew point is a reading of its own), its series ends and a new series in the new
unit starts, so every series has one unit (the CSV has both columns, e.g. `Environment · Temperature (SHT4x 0x44) [°C]` and
`[°F]`). Without a sensor nothing is added. These series carry `"locked": true` (every other
series `false`): `DELETE` answers 409, and the UI offers no removal. The sampler takes their values from the sensor
with the same freshness rule; the CSV export has their columns like any other. `environment` is not a valid config
entry id. `GET /environment` → `{present, source, readings: [{field, channel, label, unit, value, at}]}`. A sensor that stops
reporting leaves its series in place (gaps) until it reports again.

## Storage (`bm-store`, `~/.benchmesh/recordings.db`; simulation: in memory)

The database and its `-wal` / `-shm` files are private to the user (0600 on Unix, set at every
open): measurements can be confidential.

```sql
recordings(id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE, description TEXT,
           interval_s INTEGER NOT NULL, state TEXT NOT NULL,
           started_at INTEGER NOT NULL, stopped_at INTEGER, paused_at INTEGER)   -- ms
series(id INTEGER PRIMARY KEY, recording_id INTEGER NOT NULL REFERENCES recordings(id) ON DELETE CASCADE,
       device_id TEXT NOT NULL, class TEXT NOT NULL, channel INTEGER NOT NULL, field TEXT NOT NULL,
       label TEXT NOT NULL, unit TEXT NOT NULL, added_at INTEGER NOT NULL, removed_at INTEGER)
samples(series_id INTEGER NOT NULL REFERENCES series(id) ON DELETE CASCADE,
        t INTEGER NOT NULL, v REAL NOT NULL, PRIMARY KEY (series_id, t)) WITHOUT ROWID
```
The v1 tables (`recording_series`, `data_points`, Python-compatible) are dropped: they hold no data
on the bench machine and the Python backend is being removed.

## HTTP API (JSON; errors `{"detail": "..."}`)

Recording object:
```json
{"id": 3, "name": "PSU ramp", "description": "", "interval_s": 1, "state": "recording",
 "started_at": 1792234800000, "stopped_at": null, "paused_at": null,
 "samples": 1234, "last_t": 1792236034000,
 "series": [{"id": 11, "device_id": "tenma-72-2540", "class": "PSU", "channel": 1, "field": "VOUT",
             "label": "Tenma · VOUT", "unit": "V", "added_at": 1792234801000, "removed_at": null}]}
```
All times are integer ms since the epoch (UTC).

| Method | Path | Body / query | Result |
|---|---|---|---|
| GET | `/recordings` | | `[Recording]`, newest first |
| POST | `/recordings` | `{name, description?, interval_s}` | 201 Recording (state `recording`, no series). 409 name taken; 422 invalid (name 1-100 chars after trim, interval 1-3600 integer) |
| GET | `/recordings/{id}` | | Recording; 404 |
| PATCH | `/recordings/{id}` | `{name?, description?}` | Recording; 409 name taken |
| POST | `/recordings/{id}/series` | `{device_id, class, channel, field, label?, unit?}` | 201 Series. 404 recording; 409 recording stopped, or the signal is already active in it; 422 unknown device/class/channel or field not a numeric status key of the class contract. Default label `"<device name> · <field>"`, unit `""` |
| DELETE | `/recordings/{id}/series/{sid}` | | 204 (sets `removed_at`; data kept); 404 |
| POST | `/recordings/{id}/pause` · `/resume` · `/stop` | | Recording (idempotent; stop is final) |
| DELETE | `/recordings/{id}` | | 204 (removes its data) |
| GET | `/recordings/{id}/data` | `from`, `to` (ms, optional), `max_points` (default 2000, 10-20000) | `{"recording_id", "from", "to", "series": [{"id", "t": [ms], "v": [num]}]}`. A series with more samples in the range than `max_points` is decimated into `max_points/2` time buckets keeping each bucket's min and max (in time order) - peaks survive |
| GET | `/recordings/{id}/export` | | `text/csv`: header `time_utc,time_ms,<label> [<unit>],…`; one row per distinct `t`; empty cell where a series has no sample |
| WS | `/ws/recordings` | | see below |

The previous `/recordings/start`, `/recordings/{id}/data?offset&limit` (v1) and
`/recordings/ws/{id}` are replaced.

### WebSocket `/ws/recordings`
Server → client JSON messages:
- on connect: `{"type": "snapshot", "recordings": [Recording, …]}` (all recordings, newest first)
- every tick, per recording that sampled: `{"type": "samples", "recording_id": 3, "t": 1792236034000,
  "values": {"11": 4.93, "12": null}}` - every active series of the recording, `null` = no sample
- on any change (create, state, series added/removed, rename): `{"type": "recording", "recording": Recording}`
- on delete: `{"type": "deleted", "recording_id": 3}`

## UI (frontend, `src/ui/recording/`, kit look, all three skins)

- **Recording a reading**: every numeric reading in the class panels (PSU VOUT/IOUT/POUT, DMM main
  reading, ELL V/I/P, AWG frequency/amplitude/offset readbacks, OSC per-channel Vpp/Vavg/Vrms/Freq,
  generic panel readouts) is a *signal*. Right-click (or the context-menu key / Shift+F10 when
  focused) opens a context menu: the active recordings (state recording/paused) with a check mark
  where the signal is recorded - toggling adds/removes the series - and "New recording…" (creates
  one and adds the signal). A recorded reading shows a small red dot.
- **Recorder dock** (bottom of the main window): a slim bar listing active recordings (pulsing REC
  mark, name, elapsed time, series count) and "+ New recording"; expanding shows the selected
  recording's live chart, its series legend (colour, label, latest value, remove ×), Pause/Resume
  and Stop. Height toggles between collapsed bar and an expanded panel; state remembered.
- **Recordings drawer** (top bar "Recording"): history (name, state, start, duration, series,
  samples), open a recording's full chart (zoom/pan, decimated data from `/data`, re-queried on
  zoom), rename/describe, export CSV, delete (with confirmation in the UI).
- **Chart**: time x-axis (local time labels); **one y-axis per series** in the series' colour
  (so 0-5 V and 0-120 V share one chart legibly), alternating left/right; legend toggles series;
  tooltip shows all values at a time; live mode follows the newest data (last 10 min by default,
  "All" button). Series colours from a skin-aware palette (tokens).
- **Other views**: a distribution may add ways to draw a recording (`extensions.recordingViews`,
  `ai_context/OPEN_CORE.md`); the dock and the drawer then show a **Chart | <view>** switch
  (remembered in `benchmesh:recording-view`, `frontend/src/ui/recording/views.tsx`), the view gets
  the chart's series and asks the drawer for finer data of the range it shows (`onRange`). A view
  that fails falls back to the chart. The open-source build adds none.
