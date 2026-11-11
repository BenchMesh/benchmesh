# Automation, assistant and MCP - design and contract

Status: implemented in `crates/bm-server/src/automation/` and `crates/bm-server/src/mcp.rs`
(UI: `frontend/src/ui/automation/`). Replaces the Node-RED nodes as the built-in way to automate
a bench; Node-RED keeps working as an external REST client.

## Goals

1. **Flows** - automations a user builds by hand: read instruments, decide (conditions), set
   instruments, wait, loop, sweep, record. Example: *read the DMM; if the voltage is above the
   target raise the load current, else lower it; stop at 2 A*.
2. **Assistant** - the user describes an automation in words and an LLM assembles the flow. The
   user brings their own API key (Anthropic, OpenAI or OpenRouter, or any OpenAI-compatible
   endpoint such as a local Ollama) and pays the provider directly. The result is an ordinary
   flow: reviewed, edited and run like a hand-made one. The LLM never drives instruments.
3. **MCP** - AI clients (Claude Code, Claude Desktop, Cursor, …) connect to the running app and
   read instruments, build flows and (when the user allows it) run them.

One runtime serves all three: the editor, the assistant and MCP all produce **flow documents**
that the same validator checks and the same executor runs.

## Flow document (`benchmesh.flow/v1`, JSON)

```json
{
  "schema": "benchmesh.flow/v1",
  "name": "Hold 5 V with the load",
  "description": "Raises or lowers the load current until the DMM reads 5 V.",
  "params": [
    {"name": "target", "value": 5.0, "unit": "V", "description": "Voltage to hold", "min": 0, "max": 30}
  ],
  "setup":   [ ...steps ],
  "steps":   [ ...steps ],
  "cleanup": [ ...steps ],
  "timeout_s": 600
}
```

- `params` are the settings a user adjusts without touching the steps (form above the flow);
  names are expression variables. Values: number, string or bool. `min`/`max` are checked at run
  start. A run may override values (`params: {"target": 4.8}`).
- `setup` runs first, then `steps`; **`cleanup` always runs** - after success, failure, a
  `stop` step and an abort (abort of cleanup itself is the only way to skip it).
- `timeout_s` (optional) bounds the whole run (cleanup excluded).

### Steps (tagged by `do`)

Instrument address in every instrument step: `device` (config id), `class` (3-letter),
`channel` (default 1).

| `do` | Fields | Meaning |
|---|---|---|
| `set` | address, `op`, `value` *or* `expr` | Setter `op` of the class contract (e.g. PSU `voltage`, ELL `curr`, `input`). Value limits come from the definition. Waits for **confirmation** (below). |
| `read` | address, `field` *or* `op`, `into` | `field`: status key (e.g. DMM `MEAS`, PSU `VOUT`, ELL `VIN`) from the live registry; `op`: a query. Numbers (SI dicts → `val`) become numbers, enums strings. Stored in variable `into`. |
| `wait` | `seconds` *or* `expr` | Pause (abortable). |
| `let` | `var`, `expr` | Assign a variable. |
| `if` | `condition`, `then`, `else?` | Branch. |
| `repeat` | `times` *or* `while` *or* `until`, `steps`, `max_iterations?` (default 10000) | Loop. `while` is tested before each pass, `until` after. Variable `iteration` (1-based) inside. Each pass takes at least 0.1 s. |
| `sweep` | `var`, (`from`,`to`,`step`) *or* `values`, `steps` | For each value (inclusive `to`, float-tolerant) run `steps` with `var` set. |
| `check` | `condition`, `message`, `fail` (`stop` default \| `warn`) | Assertion; `stop` fails the run (cleanup runs). |
| `log` | `message` | Run log line; `{name}` is replaced by a variable's value. |
| `prompt` | `message` | Pause until the user (UI or MCP) continues - "connect the battery". |
| `record` | `action: start`, `name`, `interval_s?` (1), `series: [address + field + label?]` / `action: stop` | Starts/stops a recording (recordings v2, one time domain). A recording the run started is stopped when the run ends. |
| `stop` | `message?` | End the main part successfully (cleanup still runs). |

Expressions (`expr`, `condition`) are **CEL** (the definitions' expression language; it always
terminates). Variables: params, `read`/`let`/`sweep` results, `iteration`, `elapsed` (seconds
since run start). Functions: CEL built-ins, `num(x)`, `abs(x)`, `min(a,b)`, `max(a,b)`,
`round(x, n)`.

### Validation (before saving and before running)

Errors carry a path (`steps[2].then[0]`) and a message. Checked: schema, step fields, CEL
compiles, variables are defined before use (params, `into`, `var`, `iteration` inside loops),
device exists in the config, class on that device, channel range, setter/query/status key exists
for that class on that device, literal values against the setter's value spec (enum values,
min/max), `repeat` has a bound. A device that is configured but offline is a warning, not an
error.

## Execution (`POST /automation/run`)

- **One run at a time** (a second run → 409).
- **Arming:** a run whose `set` steps can energise (setter `energizes` in the definition, and the
  value is not the safe value) needs `"arm": true`. Without it → 409
  `{"detail", "energizes": [{"path","device","class","channel","op"}]}`; the UI shows that list
  and asks once.
- **Confirmation of `set`:** after sending, the executor waits (max 6 s, like the UI) until the
  instrument reports the value: the op's query if it has one, else the status key named like the
  op in upper case (`output` → `OUTPUT`). Numbers are equal within half a unit of the value
  spec's last decimal (or 1e-6 relative). Not confirmed → the step fails. Nothing to read back →
  logged as "sent (no read-back)".
- **Fresh reads:** a `read` of a status field waits (max 5 s) for a status update newer than the
  run's last `set` - a reading always reflects the change before it. Stale or offline → step
  fails.
- **Safe state:** when a run fails or is aborted, after `cleanup` every energising op the run
  switched on is set to its safe value (`OFF` / `false`). A successful run leaves instruments as
  its cleanup left them.
- **Abort:** `POST /automation/run/abort` (UI: red button in the top bar while a run is active,
  key `Escape` in the run view; MCP `abort_run`). Waits are interrupted immediately; an
  instrument exchange in progress finishes first.
- **Log:** every step start/result, set value, read value, warning and error is a run event
  `{seq, t (ms, wall clock), level: info|warn|error, path, msg}` (ring of 5000). `t` is in the
  recordings' time domain, so a run log lines up with recorded series.

Run state: `idle` (no run yet), `running`, `waiting` (prompt), `cleanup`, `finished`, `failed`,
`aborted`.

## REST API

| Method | Path | Body / result |
|---|---|---|
| GET | `/automation/vocabulary` | What flows can address: per configured device `id`, `name`, `online`, classes with `channels`, `fields` (status keys with type/description), `operations` (`get`, `set`, `value` spec, `energizes`). |
| GET | `/automation/flows` | `[{id, name, description, updated_at}]` |
| POST | `/automation/flows` | flow → `{id, flow, issues}` (saved even with warnings; errors → 422 `{detail, issues}`) |
| GET/PUT/DELETE | `/automation/flows/{id}` | the stored flow / replace / delete |
| POST | `/automation/validate` | flow → `{ok, issues: [{path, level, message}], energizes: [...]}` |
| POST | `/automation/run` | `{flow_id}` or `{flow}`, `params?`, `arm?` → 202 run status; 409 `{detail, parked}` when the flow uses a parked instrument (`ai_context/PARKING.md`) |
| GET | `/automation/run?after=<seq>` | run status `{id, flow, devices, state, started_at, ended_at, path, vars, error, recording_id, events}` (`devices`: the instruments the flow addresses; they cannot be parked while it runs) |
| POST | `/automation/run/abort`, `/automation/run/continue` | 204 |
| GET/PUT | `/automation/assistant` | `{provider, model, models: {provider: model}, base_url, keys: {anthropic: "sk-…a1b2" \| null, …}, providers}` (`model`, `base_url` are the current provider's; model, base URL and key are stored per provider); PUT `{provider?, model?, base_url?, api_key?}` applies to the (new) provider; `api_key: ""` deletes the key |
| GET | `/automation/assistant/models` | the provider's model list (`[{id, name}]`) |
| POST | `/automation/assist` | `{instruction, flow?}` → `{flow, explanation, issues}` |
| GET/PUT | `/automation/mcp` | `{access: "off"\|"read"\|"control"}` |

Flows are files `<data dir>/flows/<id>.json`; settings `<data dir>/automation.json` (permissions
0600 on Unix: it holds API keys). Without a data dir (simulations, tests) both are kept in memory.

## Assistant

- Server-side: the key never reaches the web page after it is saved (only a hint `sk-…a1b2`).
  The UI calls `POST /automation/assist`; the server calls the provider (rustls, 120 s timeout).
- Providers: `anthropic` (Messages API, tool `submit_flow` with `tool_choice: auto` - current Claude models refuse a forced tool - and the system prompt asking for that tool only), `openai` and `openrouter`
  (Chat Completions, forced function `submit_flow`), `custom` (OpenAI-compatible `base_url`, e.g.
  `http://localhost:11434/v1` for Ollama).
- Prompt: role and rules, the flow format (this section, condensed), the vocabulary of this
  bench (devices, fields, operations with limits and `energizes`), and the current flow when
  refining. Rules given to the model: only address listed instruments and ops; put user-tunable
  numbers in `params` with units; every flow that switches an output/input on must switch it off
  in `cleanup`; bound every loop; prefer reading status fields; ask (in `explanation`) when the
  request is ambiguous instead of guessing a dangerous value.
- Output tool `submit_flow {flow, explanation}`. The server validates the flow; on errors it
  sends them back and lets the model correct itself (max 2 retries). The response always carries
  the final issues; the UI loads the flow into the editor - it is never run automatically.

## MCP server (`POST /mcp`, Streamable HTTP, JSON responses)

JSON-RPC 2.0: `initialize` (protocol versions 2025-11-25, 2025-06-18, 2025-03-26, 2024-11-05),
`notifications/*` (202), `ping`, `tools/list`, `tools/call`. `GET /mcp` → 405 (no server
stream). The cross-site guard applies (Origin must be local, as the MCP spec requires).

Access (`/automation/mcp`, set in the UI; default `read`):

| Tool | Access | |
|---|---|---|
| `bench_overview` | read | instruments, online state, live status, operations with limits |
| `read_instrument` | read | one status field or query |
| `flow_format` | read | the flow document reference with examples |
| `list_flows`, `get_flow`, `validate_flow` | read | |
| `run_status` | read | state and log of the current/last run |
| `list_recordings`, `get_recording` | read | `get_recording` returns at most `max_points` (default 500) rows |
| `set_instrument` | control | one setter, confirmed like a flow `set`; energising needs `confirm_energize: true` |
| `save_flow` | control | validate and save |
| `run_flow` | control | by id or document; energising needs `arm: true` |
| `abort_run`, `continue_run` | control | `abort_run` is allowed at `read` too: stopping is always safe |
| `start_recording`, `stop_recording` | control | |

`access: off` → every call errors with the hint where to enable it.

**stdio bridge** for clients that only start processes (Claude Desktop):
`benchmesh --mcp` (desktop app executable) or `benchmesh-server mcp` forwards JSON-RPC lines
from stdin to `http://127.0.0.1:57666/mcp` (`BENCHMESH_URL` overrides) and writes the replies to
stdout. The app must be running.

## Not in v1

Control blocks (PID / hysteresis as single steps), a flowchart view, scheduled/triggered flows,
parallel runs, dry run on the simulator from the editor, markers in recordings.
