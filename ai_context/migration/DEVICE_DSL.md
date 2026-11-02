# BenchMesh Device Definition DSL (`benchmesh.device/v1`)

Status: v1 schema + validator implemented (`crates/bm-devicedef`). The runtime interpreter is
Phase 3. This document is the contract between definition authors (humans or an LLM working
from a PDF manual) and the interpreter.

## Why a DSL

Every instrument quirk found in Phase 1 (no terminators, fixed-width replies, setters that
never reply, remote-mode-only queries, mode-gated setpoints, settling garbage, binary blocks)
is expressed as **data**, interpreted by one generic Rust driver. Adding an instrument needs
no Rust and no Python. The format is plain YAML validated by a JSON Schema generated from
the Rust types, so an LLM can be constrained to it and its mistakes fail loudly
(`deny_unknown_fields` everywhere).

## Three contract layers

| Layer | File | Owns |
|---|---|---|
| Class schema | `devices/classes/<CLS>.yaml` (`benchmesh.class/v1`) | What the UI/API rely on: status keys (+type, required), operations (+value type, required directions) |
| Device definition | `devices/<id>.yaml` (`benchmesh.device/v1`) | How one device family talks: link, framing, commands, parse, mapping onto classes |
| Interpreter | Rust (Phase 3) | Generic execution of definitions |

JSON Schemas: `devices/schema/{device,class}.schema.json` (generated, drift-tested).

## Tooling

```bash
cargo run -q -p bm-devicedef -- validate devices      # semantic validation, exit 1 on errors
cargo run -q -p bm-devicedef -- schema device|class   # print JSON Schema
cargo test -p bm-devicedef                            # incl. capture coverage test
```

`cargo test` also checks that every command the Python service sent in `captures/` can be
produced by its definition (type-aware: enum params only match their wire values). Deliberate
differences are listed with a reason in `NOT_REPRODUCED` (`crates/bm-devicedef/tests/definitions.rs`).

## Device definition reference

### Header
| Key | Meaning |
|---|---|
| `schema` | `benchmesh.device/v1` |
| `id` | definition id = config `driver` key |
| `vendor`, `family`, `version` | descriptive; version is semver |
| `enabled` | default `true`; `false` hides it from the config UI |
| `sources` | provenance: `{doc, pages}` manuals, `{capture}` sessions, `{note}` |
| `models` | `{name, match (regex on identify reply), support, note, features{CLS: {...}}, channels{CLS: n}}`; `support`: `tested` (maintainers' bench) · `confirmed` (a user's first-contact check passed) · `awaiting` (default: from the manual, not yet seen live); per-model overrides merge over the class binding |
| `identify` | command whose reply is matched against `models` on connect |

### `extends` (shared baselines)
`extends: <base-id>` merges the definition over `devices/bases/<base-id>.yaml`, a partial
definition (any keys, may itself `extends` another base; no `id`/`models`). Mappings merge key
by key (override one field of a base command and keep the rest), lists and scalars replace,
and `null` removes an inherited key (`commands: {beep: null}` for a model without a beeper).
The merged result is validated like any definition. Bases hold what families share, e.g. a
SCPI-99 PSU baseline; definitions then carry only identity, models and quirks.

### `link` (exactly one)
- `serial: {baud, format: 8N1, flow: none|rts_cts|xon_xoff, dtr/rts: untouched|low|high, settle_ms, modem_control: best_effort|required}`
  `best_effort` = DTR/RTS failures do not abort (pseudo-terminals, odd adapters).
- `usbtmc: {vid, pid}`
- `tcp: {port}` - raw SCPI socket (config `transport: tcpip`, `host[:port]`)
- `vxi11: {device: inst0}` - VXI-11 / LXI over ONC RPC (portmapper 111; config `transport: vxi11`, `host`); message-based like USB-TMC

`link` may list several of these at once, each with its own `framing`; the config entry's `transport` picks one (without one: serial, then USB-TMC - a LAN link needs a host).

### `framing`
- `tx.terminator`: appended to every command (may be `""`).
- `rx.mode`:

| mode | reply ends when | notes |
|---|---|---|
| `terminator` | `rx.terminator` seen | `skip_leading` chars dropped before the reply starts; `strip_trailing` removed |
| `fixed` | `reply: {bytes: N}` received | every replying command must declare `bytes` (or `block`) |
| `idle` | `idle_ms` silence after the first byte, or `bytes: N` reached | good for terminator-less devices with fixed-width replies plus a variable one |
| `message` | transport delivers one message (USBTMC EOM) | |

- `rx.timeout_ms`: max wait for a complete reply. `rx.drain_idle_ms`: after a timeout or malformed reply, discard input until idle this long. **Never let a late reply land on the next command.**
- `timing.min_gap_ms`: gap between exchanges. `timing.gap_after_write_ms`: gap after a `reply: none` command (e.g. TENMA loses setters followed within 20 ms).

### Modbus RTU
`framing.modbus: {address: 1}` makes the definition a Modbus RTU slave. A command then carries
`modbus` instead of `send`: `{fn: read_holding|read_input|read_coils|read_discrete, register, count}`
or `{fn: write_single|write_multiple|write_coil, register, value: <param>, encoding: u16|i16|u32|i32|f32, swap}`.
The engine builds the frame with CRC16, frames the reply by its function's length, checks slave,
function and CRC, and turns exception replies into refusals (`modbus exception 2: illegal data
address`). A write's value is its param after validation and `scale` (`{type: number, max: 60,
scale: 100}` → centivolts); enum wire values give the integer (`{ON: "1", OFF: "0"}`). Reads return
register data, decoded with `{word: {at, type, scale, swap}}` (one value) or `{registers: {name:
{at, type, scale}}}` (map), coils with `bits`. Words are big-endian; `swap` puts the low word of a
32-bit value first. A register used as identify (model id) identifies as its number's text.
Manual examples describe a Modbus instrument as a register map (`modbus: {holding: {0: 60062}}`),
answered by a simulated slave that applies writes. Example: `devices/bases/riden_rd60xx.yaml`.
Not yet: a per-entry slave address in config.yaml, Modbus TCP.

### `session`
`on_connect` / `on_disconnect`: steps run after identify / before close (e.g. OEL `SYST:REM` / `SYST:LOC`).

### `state`
Client-side variables for values the device cannot report: `{type, initial, description}`.
Updated by command `effects`. Referenced as `{state.x}` in templates and `state.x` in CEL.

### `commands.<name>`
| Key | Meaning |
|---|---|
| `send` | template: `{ch}` channel, `{param}`, `{state.var}`; `{{`/`}}` literal braces |
| `params.<p>` | `{type: number\|integer\|bool\|string\|enum, min, max, format (%.Nf\|%d\|%g), unit, values (enum: API value → wire text), pattern (string regex), from (CEL → computed param)}` |
| `reply` | `line` (default; rx framing) · `none` (never read) · `{bytes: N}` · `block` (IEEE 488.2 `#<n><len><data>`) |
| `parse` | pipeline, see below |
| `on_empty` | `error` (default) or `"null"` (empty reply means "no value", e.g. OEL in local mode) |
| `cache_ms`, `invalidates` | result caching and invalidation |
| `effects` | `{state_var: template}` after success; placeholders get the caller's API values |
| `before` | steps that must run first (only after `requires` passed: a refused command sends nothing) |
| `requires` | `{expr: CEL, message}` refuse (4xx with message) unless true. Preferred over hidden side effects (OEL: setpoint only in the matching mode) |
| `safety` | `query` (read-only, the only kind allowed in polls) · `configure` · `energize` (can switch outputs/inputs on; future PDF-generated definitions never auto-run these) |
| `timeout_ms`, `max_bytes` | per-command overrides (block replies) |
| `gap_after_ms` | per-command gap override (XDM: `CONF:TEMP` stalls ~2.8 s) |
| `settle_ms` | polled readings reported as null for this long afterwards (XDM: stale mantissa after a mode change) |
| `description`, `source` | documentation + manual page citation |

Enum params map API values to whole wire fragments, so one command can cover a family of
commands (`send: "{on}"`, `values: {ON: SYST:REM, OFF: SYST:LOC}`). Computed params (`from`)
are evaluated over cached query results at send time, e.g. SPM range commands depend on the
active function (`from: meas_func`).

### Parse steps
Unit steps are strings, others single-key maps:
`trim`, `unquote`, `lower`, `upper`, `number` (f64, accepts `+1.0E-04` and `+0.0000E-308`),
`text` (a number as its shortest decimal text: `1.000000e+01` -> `"10"`, `0.1` -> `"0.1"`; for an
operation the class contract wants as text, written like the setter's values - a status that
needs the number reads it back with `num()`),
`byte` (first byte as integer), `{split: {sep, index}}`, `{fields: {sep, names}}` (→ map),
`{regex: {pattern, group}}`, `{replace: {from, to}}`, `{map: {raw: value}}` (unmapped = error
unless `_` key), `{bits: {name: "N" | "A..B"}}` (→ map of bool/int), `{null_if: [raw...]}`,
`{samples: u8|i8|u16le|i16le|u16be|i16be}` (binary payload → list of integers; first step, e.g. an
oscilloscope's `reply: block` waveform). A `block` reply without parse steps is returned as bytes
(images); with steps it is data.

### `classes.<CLS>` (binding)
| Key | Meaning |
|---|---|
| `channels`, `ui_component`, `features` | registry/UI metadata (features = the UI dictionary served by `GET /instruments/{class}/{id}`). The bench UI uses one panel per class and ignores `ui_component` |
| `poll` | list of groups `{every_ms, read: [query commands]}`; slower groups' values persist between refreshes; per channel when the command or one of its `before` steps uses `{ch}` (e.g. `:WAV:DATA?` after `:WAV:SOUR CHAN{ch}`). A successful set makes all of the class's groups due at once, so slow groups can be slow without delaying feedback |
| `status.<KEY>` | CEL expression over poll results (by command name), `state`, `ch`. Shorthand string = raw value; `{expr, as: si}` = SI value dict |
| `operations.<op>` | `{get: cmd \| [steps], set: cmd \| [steps], as: raw\|si\|bytes}`; POST value binds to the single caller param (or `{value}` in step args). A step `{cmd, args, only: [values]}` runs only for those caller values (e.g. an LCR's series/parallel command, absent for R-X). `GET .../methods` reports each setter's value spec (type, min/max, format, unit, enum values) and `energizes` |

Class contracts (`devices/classes/*.yaml`) type status keys and operations with `number`, `integer`,
`bool`, `string`, `enum`, `si`, `bytes` (files) or `list` (numeric arrays, e.g. waveform samples).

### CEL environment (interpreter contract)
- Variables: poll command names (parsed results; maps for `fields`/`bits`), `state`, `ch`.
- Custom functions the interpreter must provide: **`num(x)`** lenient numeric parse →
  double, or null for non-numeric tokens such as `OL`.
- A status value of `null` means "unknown right now" (settling, local mode, overload).

## Validation rules (implemented)
Schema id; regexes compile (models, params, parse); template placeholders ↔ params; enum
params have values; number formats; rx-mode consistency; identify/session/before/poll/
operation references exist; polls only use replying, `query`-safety, param-free commands;
CEL expressions parse and reference only known variables; status keys and operation `as`
match the class contract types; an operation the contract wants as text (string, enum) is not
read with a parse that ends in a number (`number`, `scale`, `byte`: end it with `text` or map the
values); required class status keys and operation directions are
bound; `effects` target declared state; `requires` messages present; unused commands warn.

## Authoring checklist (human or LLM)
1. Identity: `*IDN?`-style command + per-model regex; mark `support: tested` only after a bench test (`confirmed` after a user's check).
2. Framing from observation, not the manual: watch the exchange in the instrument console (or `bm-probe`) and read what the instrument really sends.
3. Every setter: `reply: none` unless the device really answers; set `safety`.
4. Numeric setters: explicit `format` with the decimals the device accepts.
5. Preconditions as `requires`, not hidden mode switches.
6. Map to the class contract; run `validate`; add the capture session to `sources`.
