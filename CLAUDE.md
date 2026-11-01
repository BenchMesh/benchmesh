# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

# Key Philosophy Documents (reference these when making design decisions):

- See @ai_context/IMPLEMENTATION_PHILOSOPHY.md for core development principles
- See @ai_context/MODULAR_DESIGN_PHILOSOPHY.md for architecture patterns


## Project Overview

BenchMesh is one cockpit for lab instruments: power supplies, multimeters, electronic loads,
signal generators, oscilloscopes, LCR meters and spectrum analyzers, connected over USB, serial or
LAN, controlled from one window, recorded on one shared timeline and automated with flows. It is
a **Rust** server with a **React** UI, shipped as a **Tauri** desktop app for Windows, macOS and
Linux. Working branch: `develop`. Repository: `BenchMesh/benchmesh-app` (public, MIT; remote `origin`; the
former `MarkoVcode/BenchMesh` is remote `legacy`). Public site: https://benchmesh.online (repo `../BenchMeshOnline`).

Instruments are described by **declarative device definitions** (`devices/*.yaml`, shared parts
in `devices/bases/`, spec `ai_context/migration/DEVICE_DSL.md`, class contracts
`ai_context/migration/CLASS_CONTRACT.md`) interpreted by one generic driver: there is no
per-device code. Design history and decisions: `ai_context/RUST_TAURI_MIGRATION_PLAN.md`. The
Python/Electron/Node-RED system it replaced is kept only in git (tag `legacy-python`).

## Architecture

| Crate (`crates/`) | Role |
|---|---|
| `bm-devicedef` | DSL types (source of truth), validator, JSON Schema, `bm-devicedef validate` |
| `bm-driver` | interpreter: framing, parsing, CEL, SI dicts, engine, per-device actor, sim transport |
| `bm-transport` | serial (`serialport`), LAN (raw SCPI socket, VXI-11), USBTMC (Linux kernel driver; Windows/macOS: own USBTMC protocol over `nusb`, or an installed VISA loaded at run time), port discovery (Linux sysfs, Windows SetupAPI, macOS IORegistry) for the port lists, in-process port lock; opens the port an entry names (`ai_context/migration/IDENTITY.md`) |
| `bm-server` | axum REST API, WebSockets, recordings v2 (one whole-second sampler from the registry, `/ws/recordings`; an environmental sensor a distribution adds - `environment.rs`, `GET /environment` - is recorded in every recording as locked series, a series per sensor (channel) and reading, in the label and unit the sensor gives it; a unit change ends the series and starts one in the new unit), metrics, OpenAPI, virtual SCPI instruments (`scpi/`), UI; `benchmesh-server` binary |
| `bm-store` | recordings database v2 (SQLite/WAL `~/.benchmesh/recordings.db`, 0600 with its journal: recordings, series, samples; contract `ai_context/migration/RECORDINGS.md`) |
| `benchmesh-extensions` | what a distribution adds to the app: stub here (adds nothing), replaced by the official build (`ai_context/OPEN_CORE.md`; not a default member) |
| `bm-probe` | bench tool: `verify` (read-only) / `get` / `set` / `screenshot` against real instruments |
| `benchmesh-app` | Tauri desktop app: embedded `bm-server` + webview (not a default member) |

```bash
cargo test                                   # all default crates (incl. capture replay tests)
cargo clippy --all-targets -- -D warnings
cargo run -q -p bm-devicedef -- validate devices
cargo run -p bm-server -- --config captures/bench-config.yaml        # API + UI on :57666
cargo run -p bm-server -- --config captures/bench-config.yaml --simulate captures/20261016-203514  # no hardware
./target/debug/bm-probe --config captures/bench-config.yaml verify    # bench check (read-only)
cargo tauri dev  /  cargo tauri build        # desktop app (run in crates/benchmesh-app)
cargo clean                                  # target/ keeps every rebuild's artifacts: clean now and then
```

Debug builds carry line tables only, dependencies no debug info (`[profile.dev]` in `Cargo.toml`): keeps `target/` small.


**Open source and the official distribution** (contract `ai_context/OPEN_CORE.md`): this repository is the whole
application and builds without anything else (`cargo tauri build`: unsigned, no updater; `GET /version`
`distribution: community`; the title bar shows "v<version> community" and no version appears anywhere else - only the
official build shows its version in Feedback and Instruments → About). The official installers are built by the private
repository `BenchMesh/services` (local `../benchmesh-services`): it overlays its extensions on a checkout of a tag
(stub crate `crates/benchmesh-extensions` -> its implementation, `frontend/src/extensions/index.ts`, a Tauri
capability and config), signs (updater key, its CI secrets; losing it means installed apps can no longer update) and
publishes the release here. App updates (desktop modal and headless Updates) and future online-service integrations
live only there. Extension points: `bm_server::extension` (routes + OpenAPI, `System::hand_over` for headless
stop-and-run), `benchmesh_extensions::{DISTRIBUTION, server, desktop}`, `frontend/src/extensions/contract.ts`
(`root`, `systemGroups`, `aboutGroups`, `onlineGroups`, `sensorsGroups` (Instruments → **Sensors**, shown only with groups; `ui/sensors/`), `topBarStatus`, `recordingViews`, `titleBadges`, `titleReadouts` (right of the title bar, before Arrange), `panelActions`). Never put private code, keys, release workflows or release notes in this repository.

**Cross-site guard:** the loopback listener (127.0.0.1:57666, the app window) has no
authentication, so `crates/bm-server/src/guard.rs` refuses what a foreign web page could make the
browser send: a `Host` that is not a loopback name (DNS rebinding), and any request whose `Origin`
is neither the server's own nor a loopback origin (form posts, `fetch`, WebSocket upgrades). There
is no CORS layer: the UI is same-origin. UI files are sent `no-store`, never 304 (`lib.rs` `fresh_ui_request`): different builds share the
address and the webview cache, and a stale index.html asks for assets the running build lacks (white window). Nothing unauthenticated listens on the network.

**Remote access** (contract and threat model `ai_context/REMOTE_ACCESS.md`, `crates/bm-server/src/remote/`,
UI `frontend/src/ui/remote/`): off by default; switched on in Instruments → Remote access with a password, the server
opens a second listener, HTTPS (rustls, self-signed certificate in `<data dir>/remote-tls/`, its
fingerprint shown in the app) on every interface, port 57667, accepting only local-network
addresses. Browsers sign in (`/remote/login`, PBKDF2 hash in `<data dir>/remote.json`, rate
limited) and get a `__Host-` session cookie; `remote::gate` checks Host/Origin, the session and the
route's scope (`remote/scope.rs`: unclassified routes are `admin` = this computer only; remote
sessions hold every scope except `admin`: remote access itself, MCP, assistant keys, the Windows
driver install). An optional **view-only password** (`PUT|DELETE /remote/view-password`) signs in
sessions of role `view`: the gate refuses everything but GET/HEAD (and sign-out) with 403, the UI
shows **Remote - view only**, panel bodies `inert`, no park or new recording (`useViewOnly`). The app window and remote browsers work at the same time; the top bar shows
**Remote: N** with **Disconnect all and switch off**; ending a session closes its WebSockets.
Headless: `benchmesh-server remote-password` (stdin). Future online accounts map onto the same
principals/scopes (see the contract).

**Headless** (contract `ai_context/HEADLESS.md`): `benchmesh --headless` (the desktop app's executable, every OS)
and `benchmesh-server` run without a window, used through remote access; `--remote-password` (or
`BENCHMESH_REMOTE_PASSWORD`) switches it on and the hash is kept, `--port`, `--data-dir`, `--config`, `--simulate`;
anything else is refused (exit 2, `bm_server::headless::check_args`). Shared runner
`crates/bm-server/src/headless.rs`, the app's part `crates/benchmesh-app/src/headless.rs`. Instruments → **System**
(`frontend/src/ui/system/`, API `/system/**`, scope `system`, `crates/bm-server/src/system/`, resources via `sysinfo`):
host, OS, cores, uptime, temperature, load, processor / memory / disk meters and BenchMesh's own use for everyone;
headless only **Restart** (same command line; Unix `exec` in place) and **Shut down** (exit 0), two presses, refused
while a flow runs, and **Start with this computer** (`system/autostart.rs`: systemd user service + linger command on
Linux, LaunchAgent on macOS, Run key on Windows; never the password; not for a simulation); extensions add groups.
Instruments → **About** (`about/AboutTab.tsx`): build, version (official only), docs and releases, groups a distribution
adds (the official build: **Updates**); Instruments → **Online** (`online/OnlineTab.tsx`) only when a distribution adds
groups (the official build: BenchMesh account and activity). A group may stop BenchMesh (`onStopping`): the tab waits
and reloads (`system/comeback.tsx`). Official releases include Linux ARM64 (Raspberry Pi 4/5); CI tests the desktop app
on ARM and the server's system code on Windows and macOS.

**Automation, assistant, MCP** (contract `ai_context/AUTOMATION.md`): flows are JSON documents
(`benchmesh.flow/v1`: params, setup / steps / cleanup-always-runs; steps set, read, wait, let,
if, repeat, sweep, check, log, prompt, record, stop; CEL expressions) validated against the
bench's vocabulary and run by one executor (`crates/bm-server/src/automation/`): one run at a
time, energising steps need `arm`, every set confirmed by read-back within 6 s, status reads
newer than the last set, abort interrupts waits, a failed/aborted run switches off what it
switched on. Flows live in `<data dir>/flows/`, settings (assistant provider/model/API keys, MCP
access) in `<data dir>/automation.json` (0600). The assistant (`assist.rs`) sends the user's
description + vocabulary to the user's own provider (Anthropic / OpenAI / OpenRouter /
OpenAI-compatible) and validates the returned flow (up to two self-corrections); it never runs
it. `POST /mcp` (`mcp.rs`) is an MCP server (Streamable HTTP, JSON responses; access off / read
(default) / control); `benchmesh --mcp` and `benchmesh-server mcp` are stdio bridges to it.

**Connectors** (Instruments → Connectors, `frontend/src/ui/connectors/`): **virtual SCPI instruments**
(contract `ai_context/SCPI.md`, `crates/bm-server/src/scpi/`): off by default (access off / read /
control, `<data dir>/scpi.json`); every configured instrument gets a raw SCPI socket on
`127.0.0.1` (`TCPIP0::127.0.0.1::<port>::SOCKET`, ports from `base_port` 5025, kept per instrument)
so MATLAB, LabVIEW, PyVISA and TestStand use it like a LAN instrument whatever its own link. Dialect:
`*IDN?`, `[CLS:][CHn:]<op>?` / `<op> <value>` (the definition's operations), `FETCh:<field>?`
(latest polled status), `SYST:ERR?`, `SYST:HELP:HEAD?`; a failed query answers an empty line.
A connection that sends an HTTP request line or header is closed (a web page cannot smuggle
commands); setters for an instrument the running flow uses answer -221; `GET|PUT /scpi` are admin.
The **Python package** `benchmesh` (PyPI, zero dependencies; separate repository, local copy
`../benchmesh-python`) wraps the REST API: read / set / status / watch, recordings to pandas, flows,
remote access with certificate pinning.

**Website (BenchMeshOnline, `../BenchMeshOnline`):** the public instrument database is exported
from the definitions (`cargo run -p bm-devicedef -- export devices --notes
ai_context/website/instrument-notes.yaml`, schema `benchmesh.instruments/v1`, no wire details)
by the site's `npm run sync`, which also copies the UI kit. Community status (contract: the website's
`docs/APP_INTEGRATION.md` §1, `benchmesh.status/v1`, `crates/bm-server/src/community.rs`): a build carries
the status document of its day (`crates/bm-server/community-status.json`, refreshed by
`scripts/community-status.sh` before a release); at start the server asks once, without credentials, for
what changed since (`GET {BENCHMESH_ONLINE_URL or https://benchmesh.online}/api/v1/status?since=<its
generated>`; `BENCHMESH_ONLINE=off` disables it, simulations stay offline) and merges the models over it;
`issues` come only from that answer. `/catalog` and `/instruments` merge it per model (`community`; an
`awaiting` model users confirmed counts as `confirmed`; `issue`: a short mark, never longer than **Issue reported**
(investigating), **Fix available** / **Fix coming** when the fix is newer than the app, published or not per
`/api/v1/update/latest.json`, asked only then; hover, focus or a press shows its card - state, reports, since, what to
do - `bench/issue.ts`, `bench/IssueMark.tsx`): Instruments → Supported shows it with the confirming versions, the
panel header shows the issue. Offline: the snapshot, no issues. Linking,
problem reports and "It works" confirmations are official-build only (benchmesh-services).

**Parking** (contract `ai_context/PARKING.md`): `POST|DELETE /devices/{id}/park` sets `parked: true` in
the config entry; the device thread closes the link (after `on_disconnect`), sends nothing and only checks
presence every 5 s (`bm_transport::presence`: port enumeration / TCP connect; for a serial link that proves only the port - `presence_of: port`, shown amber - since a USB-serial interface powered by the computer stays attached with the instrument off), requests answer 409, a
parked instrument is never opened at start. Refused while the running flow uses the instrument; a run
using a parked one is refused. UI: park icon before Fold, parked panels wait in the parking strip left of
the rack (`bench/ParkingStrip.tsx`, `bench/parking.ts`) and return to their place when unparked.

**Instruments are set up by hand:** a config entry names its `port` / `device` / `host`, picked in the Instruments
drawer from what the OS reports. Finding instruments automatically - Detect and an entry's `match` (its identity,
followed on whatever port it is plugged into) - is official-build only (extension point `bm_server::extension::Finder`,
UI `instrumentsSections` / `identityField`): this build keeps `match` in an entry without following it and opens the
port the entry names; an entry with `match` and no path is refused.
On Windows a USBTMC instrument without a usable driver gets **Install driver** (Instruments drawer):
the server re-runs its executable elevated (`--install-winusb`, `bm_transport::winusb`) to bind
Microsoft's in-box WinUSB driver - the one admin prompt Windows requires. A port the OS refuses is explained (`bm_transport::access`,
message ending " - see <help URL>"): Linux "Permission denied" (`/dev/usbtmcN` belongs to root, serial ports to
`dialout`/`uucp`) and macOS 15+ "No route to host" to a LAN instrument (Local Network permission; the app bundle's
`crates/benchmesh-app/Info.plist` gives the prompt's reason); a serial port another program holds (busy, Windows
"Access is denied") is named as in use by another program, on every OS. The panel shows the fix (`bench/AccessFix.tsx`):
a command with Copy (Linux), **Open Local Network settings** (macOS desktop app), and "How to fix" to
https://benchmesh.online/docs/linux#usb-permissions / #serial-permissions / docs/macos#local-network (same commands in
the website's `src/lib/linux.ts`, section id in `src/lib/macos.ts`). Ports and USBTMC per OS:
`ai_context/migration/IDENTITY.md`; `cargo run -p bm-transport --example ports` lists what the OS reports.

Bench/hardware notes and wire captures: `captures/` (golden transcripts replayed by the tests),
`scripts/bench_setters.sh`, `scripts/bench_scope_setters.sh`, `ai_context/migration/device-notes/`.

## Instrument API

- `GET /instruments`, `/instruments/{CLS}`, `/instruments/{CLS}/{id}` (features), `/instruments/{CLS}/{id}/methods`
- `GET /instruments/{CLS}/{id}/status`: the latest polled status per channel with its update time
  (ms), nothing sent to the instrument (what the bench shows; used by the Python package).
- `GET /instruments/{CLS}/{id}/{ch}/{op}` runs the definition's **query** `op`; `POST
  /instruments/{CLS}/{id}/{ch}/{op}/{value}` runs its **setter**. Only operations the definition
  declares for that class can be called, a GET never runs a setter and a POST never runs a query;
  values are validated against the setter's value spec before anything is sent.
- `/methods` lists every operation with its HTTP verb; `POST` entries carry the setter's value
  spec from the definition, e.g. `"value": {"type": "number", "min": 0, "max": 60, "format": "%.2f", "unit": "V"}`
  or `{"type": "enum", "values": ["OFF", "ON"]}`, plus `"energizes": true` for operations that
  can switch an output/input on (the UI requires two presses for those). Implemented in
  `crates/bm-server/src/opspec.rs` (also used by `/ai/context` and the automation vocabulary).
- `/openapi.json` / `/openapi.yaml` describe every route (`crates/bm-server/src/openapi.rs`; a
  test fails when a route is missing; extensions `x-websocket` and `x-no-try: <reason>` for what
  cannot be sent from the UI); `/docs` opens the UI's API reference (Instruments → **API**, the drawer wide while it is open,
  `frontend/src/ui/apidocs/`: endpoints by tag, filter, parameters with instrument suggestions,
  JSON body, Send - two presses for anything but GET - with status, time and response, curl
  line, first message of a WebSocket; pure model `openapi.ts`).

## Configuration and user data

User data lives in `~/.benchmesh/` (`BENCHMESH_DATA_DIR`): `config.yaml` (the instruments),
`recordings.db`, `flows/`, `automation.json`, `scpi.json`. A simulation (`--simulate <capture>`) keeps
recordings, flows and settings in memory. `config.yaml`:

```yaml
version: 1
devices:
  - {id: tenma-72-2540, name: Tenma, driver: tenma_72, model: 72-2540, transport: serial, port: /dev/ttyACM0, baud: 9600, serial: 8N1}
  - {id: rigol-dho804, name: Rigol DHO804, driver: rigol_dho800, model: DHO804, transport: usbtmc, device: /dev/usbtmc0}
```

`driver` names the definition (`devices/<driver>.yaml`); `parked: true` keeps an instrument disconnected
(see Parking); transports are `serial` (`port`, `baud`,
`serial`), `usbtmc` (`device`) and `tcpip` (raw SCPI socket) / `vxi11` (`host`, optionally
`:port`). A USB entry may also carry `match`, which only the official build follows (see Instruments are set
up by hand); the UI's Instruments drawer writes these entries (`crates/bm-server/src/config.rs`).

## Adding an instrument

1. Find the family's definition in `devices/` (or a base in `devices/bases/`) and add the model, or
   write a new definition following `DEVICE_DSL.md` and the class contract.
2. `cargo run -q -p bm-devicedef -- validate devices` (0 errors).
3. With hardware: `bm-probe verify` (read-only), then `get` / `set` checks; record a capture for
   the replay tests when the definition is confirmed. Without hardware: mark it from the manual
   (speculative coverage, `ai_context/migration/coverage/`).
4. The UI needs nothing: class panels read features and value specs from the API.

## Instrument UI (bench)

The UI is a **bench**: one panel per instrument class (`PSU`, `DMM`, `ELL`, `AWG`, `OSC`, `LCR`, `SAL`; any other
class gets a read-only generic panel), laid out in a responsive rack. Every instrument of a class
uses the same panel; manufacturer differences (limits, steps, ranges, modes, optional controls)
come only from the API - features (`GET /instruments/{CLS}/{id}`) and setter value specs
(`GET /instruments/{CLS}/{id}/methods`, from the device definition). There is no per-device UI
code; the definitions' `ui_component` is not used by the UI.

| Path (`frontend/src/ui/`) | Role |
|---|---|
| `kit/` | instrument component kit with three skins in `kit.css` - **Reflective LCD** (default), LED segment, Cockpit glass - switched in Instruments → Miscellaneous (`drawers/SkinSetting.tsx`, `kit/skin.tsx`, stored in `localStorage` `benchmesh:skin`; canvases read colour tokens at draw time). Components: segment readout, digit-wise setpoint editor, arm-to-enable switch (two presses for anything that `energizes`), micro switch, annunciators, rotary dial, knob, 1-2-5 stepper, bargraph, sparkline, waveform preview, oscilloscope screen, spectrum screen, action button (momentary actions confirmed by a reported change). Fonts are bundled (B612, B612 Mono, Chakra Petch) - the desktop app runs offline |
| `bench/profiles.ts` | features + methods → what a class panel shows (limits, decimals, modes, ranges, optional controls) |
| `bench/panels/` | class panels (`PsuPanel`, `DmmPanel`, `EllPanel` - modes without a setpoint, Short (two presses); `AwgChannel` - frequency or period, amplitude as Vpp / Vrms / dBm / high-low levels, phase, ramp symmetry, load, pulse duty/width/edges, mode CW / MOD (AM/FM/PM: modulating wave, frequency, depth or deviation) / SWEEP (start, stop, time, spacing, trigger) / BURST (N cycles or infinite, period, trigger), each control only when the definition binds it: conversions in `bench/awgMath.ts`, values the instrument does not report tagged CALC; `OscPanel` - one screen for all channels, `LcrPanel` - primary/secondary readouts, function keys L/C/R/Z/DCR → secondary → SER/PAR, frequency/level, range/speed, open/short correction with a prompt, bias; `SalPanel` - spectrum screen (`kit/SpectrumScreen`: dB graticule, max-hold ghost, marker diamond, click to place), marker readout, annotation bar, soft-key groups FREQ/BW/AMPT/SWEEP/TRACE/MKR/TG; `GenericPanel`) |
| `bench/ParkingStrip.tsx`, `bench/parking.ts` | parked instruments (`ai_context/PARKING.md`): park control in the panel header (asks twice with an output on, disabled while the running flow uses the instrument), slim strip left of the rack with a presence LED before each name, sheet with details and Unpark. Status lights are `bench/Led.tsx`: a rectangular LED one capital letter tall (`1cap` of the text's font, bottom on its baseline; logical sizes, so it turns with vertical text), tooltip with the state - also before the class name in every panel header (connected green / offline red with why, `offline.ts` `linkLed`) |
| `bench/layout.ts`, `Rack.tsx` | panel order / width (S, M, L, W) / hidden / channel layout of multi-channel panels (side by side or stacked), persisted in `localStorage` (`benchmesh:bench-layout`); columns follow the window width (300 px units) |
| `bench/BenchLayout.tsx`, `bench/TitleBar.tsx` | shell: a fixed head of two rows (only the content below it, `.bench-body`, scrolls) - the **title bar** (brand, version on its baseline, a distribution's badges after it (`extensions.titleBadges`), a distribution's readouts on the right (`extensions.titleReadouts`, the official build: lab environment and clock), **Arrange** as an app control, then on Windows/Linux the window controls minimize / maximize / close; the bar drags the window, a double press maximizes) and the top bar (statuses on the left - **API** (the server answers this window), **Readings** (the registry stream is current), a distribution's (`extensions.topBarStatus`, the official build: **Linked** / **Not linked**), each a `StatusPill` with its explanation as tooltip, texts in `bench/status.ts`, sentence case - then run / Remote; drawer buttons on the right: Instruments, Recording, Automation, Feedback) - **Arrange** mode (drag, ◀ ▶, width, channels ⇆ / ⇅, hide), drawers (Instruments with its tabs - Log and API among them -, Recordings, Automation, Feedback; they open below the head, which stays usable: a menu button switches the open drawer). The desktop app's main window has no OS title bar on Windows and Linux (`decorations(false)` in `benchmesh-app` main.rs, permissions `capabilities/main-window.json`); macOS keeps its native title bar and the UI shows no window controls there (`desktop.ts` `hasOwnWindowControls`) |
| `drawers/` | Instruments drawer tabs: configuration (sections a distribution adds on top: `extensions.instrumentsSections`, the official build's Detect), Supported, Miscellaneous, **Metrics** (`MetricsView.tsx` + `metrics.ts`: per instrument link busy, exchanges/s, API latency from `/ws/metrics`, judged against limits), **System** (`system/`: resources; restart, shut down, start with the computer when headless), **Sensors** (a distribution's only, `sensors/`), **Connectors** (`connectors/`: virtual SCPI access, addresses with MATLAB / PyVISA / LabVIEW code, the Python package; this computer only), **Log** (the request log, `LogsPanel.tsx`), **API** (the API reference, `apidocs/`; the drawer widens), **Online** (a distribution's only), Remote access, **About** (`about/AboutTab.tsx`) |
| `recording/` | recordings v2 (`ai_context/migration/RECORDINGS.md`): `RecordingsProvider` (REST + `/ws/recordings`, live sample buffers; main and panel windows), recordable readings (`signal.tsx`: `SignalScope` + `Recordable`, kit `ContextMenu` on right-click / context-menu key, red dot when recorded), recorder dock (bottom of the main window, `benchmesh:recorder-dock`), recordings drawer (history, zoomable chart, rename, CSV export, delete). Saving a file (CSV, flow JSON, console log: `ui/files.ts`) asks where with the system's Save dialog in the desktop app (`benchmesh-app` `save.rs`, command `save_file`; the webview would drop it into Downloads silently) and downloads in a browser. Chart: `chartModel.ts` (one y-axis per series, colours `--kit-series-1..8`), ECharts core loaded lazily in `RecordingChart.tsx`; views a distribution adds (`extensions.recordingViews`) get a Chart / view switch (`views.tsx`) |
| `kit/brand.ts`, `kit/BrandMark.tsx` | the logo ("Readout": seven-segment b, ghost segments, amber decimal point): one geometry, colours on dark / on light; `BrandMark` follows the skin (`--kit-brand-*`) or takes fixed colours (website). `node scripts/logo/build.mjs` writes `docs/static/logo/*.svg`, `frontend/public/favicon.svg` and the app icons (`crates/benchmesh-app/icons`, via Inkscape + `cargo tauri icon`); a test fails when they are stale |
| `about/` | title-bar version (`GET /version`, `version.ts`) and the **Feedback** drawer (`AboutView.tsx`: version and Copy version in the official build only, GitHub issues links, contact email support@benchmesh.online with Copy email). External pages open by name (`links.tsx`): in the desktop app the system browser (or mail app) via the `open_page` command (fixed list, `crates/benchmesh-app/src/links.rs`, capability `links.json`), in a browser a new tab |
| `DisclaimerModal.tsx`, `disclaimer.css` | start screen until accepted (`benchmesh:disclaimer-accepted` when "don't show again" is ticked): centred logo, thanks, the app is in development and needs feedback, contact `support@benchmesh.online`, then the safety points; kit tokens, so every skin. The flag lives in `disclaimer.ts`; Instruments → Miscellaneous **Start screen - Show at start** (`drawers/StartScreenSetting.tsx`) brings it back |
| `automation/` | Automation drawer (top bar **Automation**, contract `ai_context/AUTOMATION.md`): saved flows (new, duplicate, delete, import/export JSON), flow editor (parameter form, Setup / Steps / Cleanup step cards with vocabulary pickers, live validation shown at each step's path, JSON view; pure document model `flowModel.ts`), run panel (arm dialog for energising flows, live log, variables, prompt Continue, Abort / Escape), assistant (user's own LLM key, result loaded as unsaved), settings tab (provider, key - only its hint is ever shown, model, base URL; MCP access + connection snippets). `RunProvider.tsx` polls `/automation/run` for the whole UI: top-bar `▶ flow - state` + red **Stop** while a run is active |

Panel **window controls** (header, top right): fold to the header line (saved with the layout;
double-click the header works too) and **open in its own window**. A detached panel lives at
`/ui/?panel=<instrumentId:CLASS>` (`bench/PanelWindow.tsx`): in the desktop app an undecorated
Tauri window (`panel-*`, opened from the UI via `window.__TAURI__`; permissions in
`crates/benchmesh-app/capabilities/panels.json`) whose header drags it, with fold (shrinks the
window), keep-on-top and close; in a browser a popup. While its window lives it writes a
heartbeat (`bench/detach.ts`) and the bench leaves the panel out (never drawn twice) and lists it
under "In own windows"; closing (or a crash) brings it back. Closing the main window closes the
app. On Linux Wayland the app runs through XWayland (`GDK_BACKEND=x11`) because Wayland does not
allow keep-on-top; `BENCHMESH_NATIVE_WAYLAND=1` opts out. Under X11 the app's windows do not wait for the compositor between
frames (`benchmesh-app` `frame_sync.rs`): GNOME 42 sometimes never confirms a frame and the window's picture then
stopped for good (journal "Frame has assigned frame counter but no frame drawn time"; check `scripts/frame-sync/check.sh`). Run as an AppImage, the app writes its desktop entry
and icon for the user at start (`~/.local/share/applications/benchmesh.desktop`, `benchmesh-app` `desktop_entry.rs`):
docks match the window (class / app id `benchmesh`) to it, otherwise newer GNOME shows a generic icon.

**Instrument console** (panel header, console icon): an optional pane at the bottom of the panel
showing what is sent to (→) and received from (←) the instrument, plus port events (•), from
`GET /devices/{id}/wire?after=<seq>&limit=<n>` (per-device ring of 50 000 entries / 8 MiB, about
19 minutes of a polled oscilloscope and longer for meters, `bm_driver::wirelog`; reply chunks
joined in the UI, which shows the last 5000 lines). Auto / Text / Hex view, pause, clear, **Copy**:
paste-ready evidence with instrument, model, identify reply, link and app version, and **Save**:
the same for everything the server kept, as a text file (`bench/wire.ts`, `bench/WireConsole.tsx`).
Transports add their own events as info lines (`Transport::take_notes`), e.g. a USBTMC recovery:
a write that times out or stalls gets abort bulk-out/in, clear halts and a device clear, then one
more try, else an error telling the user to replug (`bm-transport` usbtmc.rs, usbtmc_proto.rs). Open state per panel in `localStorage` `benchmesh:consoles`.

Every control that changes the instrument waits for **confirmation**: after the request is sent it
pulses discreetly until the instrument's reported value (registry status, or a read-back for
setpoints) equals the requested one, and shows a small red dot if that does not happen within 6 s
(`kit/confirm.ts`). Pass the reported value, never a locally assumed one.

Drawer pages of text and controls use the shared page standard in `bench/bench.css`: content in
`.bench-page` (16 px inset, 14 px rhythm, `.bench-hint` text), boxes in `.bench-group`, rows in
`.bench-group__row` (About and feedback, Metrics, Remote access); `e2e/about-feedback.spec.ts` fails when a
drawer's content comes closer than 8 px to the drawer's sides. Buttons are kit buttons (`bench-btn`, `--primary` for
the main action, `--danger` for removal, `--icon` for a glyph), fields the kit field (`kit-field`, `kit-label`,
`kit-input`); the Instruments form styles its fields with the same tokens.

Adding a class panel: write `bench/panels/<Cls>Panel.tsx` from kit components, a `<cls>Profile()`
in `profiles.ts` (with tests against `bench/__tests__/bench-fixtures.json`, captured from the
simulated bench), and dispatch it in `InstrumentPanel.tsx`.


## Frontend (`frontend/`)

```bash
npm ci
npm run dev            # Vite dev server, proxies the API to :57666 (BENCHMESH_API overrides)
npm run build          # into frontend/dist (served by bm-server under /ui)
npm run lint
npx vitest run         # unit tests (CI)
npm run test:e2e       # Playwright against a preview build with a mocked API (CI)
```

## Environment variables

| Variable | Effect |
|---|---|
| `BENCHMESH_CONFIG` | config file (default `~/.benchmesh/config.yaml`) |
| `BENCHMESH_DATA_DIR` | user data directory (default `~/.benchmesh`) |
| `BENCHMESH_DEVICES` | device definitions directory (default `./devices`, in the app its bundled copy) |
| `BENCHMESH_UI` | UI directory for the app (default its bundled copy; `bm-server --ui`) |
| `BENCHMESH_SIMULATE` | capture to simulate instead of real hardware (`--simulate`) |
| `API_PORT` | `bm-server` loopback port (default 57666; `--port`); other computers use remote access |
| `BENCHMESH_ONLINE` / `BENCHMESH_ONLINE_URL` | `off` disables the community status / its base URL |
| `BENCHMESH_URL` | server address for the MCP stdio bridge |
| `BENCHMESH_REMOTE_PASSWORD` | headless: remote access password at start (as `--remote-password`, not shown in process lists) |
| `BENCHMESH_NATIVE_WAYLAND` | `1`: run the app on native Wayland instead of XWayland |
| `BM_WS_INTERVAL` | registry WebSocket push interval in seconds |

## CI and releases

`.github/workflows/README.md`: `ci.yaml` (frontend lint/tests/build, Rust clippy/tests/definition
validation, transport on Windows and macOS), `e2e-tests.yml`. Releases: bump the version (`Cargo.toml`,
`crates/benchmesh-app/tauri.conf.json`) and refresh the community status snapshot (`scripts/community-status.sh`)
on `develop`, tag `vX.Y.Z`, push; then the release workflow of
`BenchMesh/services` builds the draft release here; publish with `gh release edit vX.Y.Z --draft=false -R BenchMesh/benchmesh-app`.

## Guidelines

- apply TDD principles when adding new features
- always MUST run tests after the code changes
- do not try to maintain "Fallback for legacy" we are developing a new software, there is no legacy we need to maintain
- always MUST cover new development with tests - whatever is added or improved
- always MUST validate if the documentation is still up to date
- always MUST follow single responsibility principle
- differentiate between unit tests and integration tests - integration tests should NOT run in GitHub Actions (reserve for local/staging testing only)
- all new unit tests that are suitable for GitHub Actions execution must be automatically added to the CI workflow
- when you add or remove dependency update THIRD-PARTY-NOTICES.md
- when you create any temporary or tool scripts place them always in scripts folder
- plain text (UI, messages, definitions, docs, release notes) uses "-" only, never em or en dashes; `frontend/src/__tests__/noDashes.test.ts` checks every tracked file
- always verify CI tests localy
- when implementing new feature always try to test on the real working service particularly applies to the server and API (`--simulate` when no hardware is attached)
- do apply defensive programming principle - especially with API implementation the parameters must be validated and resources need to return meaningful response - catch and log 500 responses
- feel free to kill and spinn up the services we are developing in this project but always MUST clean up - do not leave used ports, release all resources

# Claude's Working Philosophy and Memory System

## Critical Operating Principles

- VERY IMPORTANT: Always think through a plan for every ask, and if it is more than a simple request, break it down and use TodoWrite tool to manage a todo list. When this happens, make sure to always ULTRA-THINK as you plan and populate this list.
- VERY IMPORTANT: Always consider if there is an agent available that can help with any given sub-task, they are more specialized tools designed to tackle specific challenges. Your role is to be a general coordinator. Use the Task tool to delegate specific tasks to these agents. Where possible, launch multiple agents in parallel via a single message with multiple tool uses.

<example>
User: "I need to implement a new feature that requires changes to multiple services. [details truncated for example]"
Assistant: "Let me analyze this problem before implementing. I will break it down into smaller tasks and use sub-agents where possible. I will track my plan with a TODO list."
</example>

- VERY IMPORTANT: If user has not provided enough clarity to CONFIDENTLY proceed, ask clarifying questions until you have a solid understanding of the task.

<example>
User: "I want to create a new memory system."
Assistant: "Did you have a specific design or set of requirements in mind for this memory system? Please help me understand what you're envisioning or let me know if you would like me to propose a design or even brainstorm some ideas together. Please consider switching to 'Plan Mode' until we are done (shift+tab to cycle through modes)."
Assistant: Use ExitPlanMode tool when you have finished planning and there are no further clarifying questions you need answered from the user or if they have explicitly indicated they are done planning.
</example>

## Parallel Execution Strategy

**CRITICAL**: Always ask yourself: "What can I do in parallel here?" Send ONE message with MULTIPLE tool calls, not multiple messages with single tool calls.

### When to Parallelize

Parallelize when tasks:
- Don't depend on each other's output
- Perform similar operations on different targets
- Can be delegated to different agents
- Gather independent information

### Common Patterns

#### Multiple File Edits
When fixing the same issue across files (e.g., type errors, import updates):
```
Single message with multiple Edit/MultiEdit calls:
- Edit: Fix type error in src/auth.py
- Edit: Fix type error in src/database.py
- Edit: Fix type error in src/api.py
```

#### Batch Type Error Fixes
When pyright reports multiple type errors:
```
Single message addressing all errors:
- Read: Check current implementation in affected files
- MultiEdit: Fix all type errors in utils.py
- MultiEdit: Fix all type errors in models.py
- Edit: Update type imports in __init__.py
```

#### Information Gathering
Before implementing features:
```
Single message with parallel reads and searches:
- Grep: Search for existing patterns
- Read: Main implementation file
- Read: Test file
- Read: Related configuration
```

#### Multiple Agent Analysis
For comprehensive review:
```
Single message with multiple Task calls:
- Task zen-architect: "Design approach"
- Task bug-hunter: "Identify potential issues"
- Task test-coverage: "Suggest test cases"
```

### Anti-Patterns to Avoid

**Don't do this:**
```
"Let me read the first file"
[Read file1.py]
"Now let me read the second file"  
[Read file2.py]
```

**Do this instead:**
```
"I'll examine these files in parallel"
[Single message: Read file1.py, Read file2.py, Read file3.py]
```

### Remember

- Parallel execution is the default, not an optimization
- Sequential execution needs justification (true dependencies)
- Context is preserved better with parallel operations
- Users prefer comprehensive results over watching sequential progress

### 1. Context Window Management

- **Limited context requires strategic compaction** - Details get summarized and lost
- **Two key solutions:**
  - Use memory system for critical persistent information
  - Use sub-agents to fork context and conserve space
- **Smart memory usage** - Not everything goes in memory, be selective about what's truly critical

### 2. Sub-Agent Delegation Strategy

#### Power of Sub-Agents

- Each sub-agent only returns the parts of their context that are requested or needed
- Fork context for parallel, unbiased work
- Conserve context by delegating and receiving only essential results
- Create specialized agents for reusable, focused purposes

#### When to Use Sub-Agents (HINT: ALWAYS IF POSSIBLE)

- **Analysis tasks** - Let them do deep work and return synthesis
- **Parallel exploration** - Fork for unbiased opinions
- **Complex multi-step work** - Delegate entire workflows
- **Specialized expertise** - Use focused agents over generic capability

### 3. Creating New Sub-Agents

- **Don't hesitate to request new specialized agents**
- Specialized and focused > generalized and generic
- Request that user creates them via user's `/agents` command
- You provide the user with a detailed description
- New agents undergo Claude Code optimization
- Better to have too many specialized tools than struggle with generic ones

### 4. My Role as Orchestrator

- **I am the overseer/manager/orchestrator**
- Delegate EVERYTHING possible to sub-agents
- Focus on what ONLY I can do for the user
- Be the #1 partner, not the worker

### 5. Code-Based Utilities Strategy

- Wrap sub-agent capabilities into code utilities using Claude Code SDK
  - See docs in `ai_context/claude_code/CLAUDE_CODE_SDK.md`
  - See examples in `ai_context/git_collector/CLAUDE_CODE_SDK_PYTHON.md`
- Create "recipes" for dependable workflow execution that are "more code than model"
  - Orchestrates the use of the Claude Code sub-agents for subtasks, using code where more structure is beneficial
  - Reserve use of Claude Code sub-agents for tasks that are hard to codify
- Balance structured data needs with valuable natural language
- Build these progressively as patterns emerge

### 6. Human Engagement Points

- **Clarification** - Ask when truly uncertain about direction
- **Checkpoints** - Surface completed work stages for validation
- **Proxy decisions** - Answer sub-agent questions when possible, escalate when needed
- **Learning stance** - Act as skilled new employee learning "our way"

### 7. Learning and Memory System

#### Current Learning Needs

- Track what I learn from user interactions
- Make learnings visible and actionable
- Consider memory retrieval sub-agent for context-appropriate recall
- Avoid repeated teaching of same concepts
- Become more aligned with user over time

#### Memory Architecture Ideas

- **Working Memory** - Current session critical info
- **Long-term Memory** - Persistent learnings and patterns
- **Retrieval System** - Sub-agent to pull relevant memories per task
- **Learning Log** - Track what's been learned and when

### 8. Continuous Improvement Rhythm

- Regularly mine articles for new ideas
- Run experimental implementations
- Measure and test changes systematically
- Evaluate improvements vs degradations
- Support parallel experimentation in different trees

## Key Metrics for Success

- Becoming the most valuable tool in user's arsenal
- Amplifying user's work effectively
- Acting as true partner and accelerator
- Learning and improving continuously
- Maintaining alignment with user's approach

## Philosophical Anchors

**See `ai_context/IMPLEMENTATION_PHILOSOPHY.md` for complete implelemtation philosophy guide.**
**See `ai_context/MODULAR_DESIGN_PHILOSOPHY.md` for complete modular design philosophy guide.**
- Embrace ruthless simplicity
- Build as bricks and studs
- Trust in emergence over control

## Next Actions

- Design comprehensive knowledge synthesis architecture
- Create specialized planning sub-agent
- Build memory retrieval system
- Establish measurement framework
- Begin continuous learning cycle

## Document Reference Protocol

When working with documents that contain references:

1. **Always check for references/citations** at the end of documents
2. **Re-read source materials** when implementing referenced concepts
3. **Understand the backstory/context** before applying ideas
4. **Track which articles informed which decisions** for learning

This ensures we build on the full depth of ideas, not just their summaries.