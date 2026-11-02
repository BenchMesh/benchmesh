# Open source and the official distribution

BenchMesh is open source (MIT): this repository, `BenchMesh/benchmesh-app`, is the whole
application, and anyone can build, run, change and redistribute it. The **official
distribution** - the installers on benchmesh.online and in this repository's Releases - is
built by the BenchMesh project from this repository plus a private repository,
`BenchMesh/services`, which adds what only an official build has, and holds the release process.

## What lives where

| | This repository (public) | `BenchMesh/services` (private) |
|---|---|---|
| Application | everything: server, drivers, device definitions, UI, desktop app, headless, automation, MCP, remote access | only extensions through the points below |
| App updates | - (a build from source is updated by building it again) | desktop updater (Tauri plugin) and headless updates (Instruments - About - Updates), the update addresses |
| Online services | the public, read-only community status from benchmesh.online and its issue flags (`community.rs`) | integrations that need a BenchMesh account: linking, sending problem reports and following tickets, "It works" confirmations |
| Instruments and troubleshooting | instruments set up by hand: the port lists, opening the port an entry names, explanations of a port the OS refuses (permissions, Local Network, a port another program holds), Windows Install driver, the instrument console; an entry's `match` is kept, not followed | finding instruments automatically: Detect and following an entry's `match` (its identity, on whatever port it is plugged into); troubleshooting: Report a problem and its diagnostic (read of every operation, poll history, consistency checks, connection search, findings with suggested fixes) |
| Build and release | local builds: `cargo build`, `cargo tauri build` (unsigned installers, no updater) | CI that builds, signs and publishes the official installers; the updater signing key; release notes |

Rules:

1. The open-source app is complete on its own. Nothing here is disabled or crippled to make the
   official build look better, and nothing here needs the private repository to build or to pass
   its tests.
2. No private code, keys, secrets, release workflows or release notes in this repository.
3. Extensions only use the extension points below. A new need means a new, small, documented
   extension point here first - it is public, so other distributions can use it too.
4. A build of this repository calls itself `community`; the official build `official`.
   `GET /version` `distribution`. Only the official build is versioned: it shows its version in
   the title bar, the Feedback drawer (with Copy version and where releases are) and Instruments -
   System. A community build shows "v<version> community" in the title bar (the last release it was
   built from) and no version anywhere else (`frontend/src/ui/about/version.ts`); the instrument
   console's Copy / Save evidence names it "(community build)".

## Extension points

| Where | Contract | In this repository |
|---|---|---|
| Server | `bm_server::extension` (`Extension`: name, routes, their OpenAPI description, optionally a `Finder` that follows an entry's `match` - checks it, the ports to try, presence; optionally an `Environment` (`bm_server::environment`): the lab's temperature / humidity / pressure, recorded in every running or paused recording while it reports (locked series of device `environment`, class `ENV`) and served by `GET /environment`; `started(state)`: called once the server is set up, on its runtime, for background work with its state - the official build follows the website's issue channel; `Host`: data dir, headless; `Extensions` in `ServerOptions`); `System::hand_over` stops a headless process and runs the extension's step before it exits or restarts (`AfterStop`) | `benchmesh-server` passes none |
| Desktop app | crate `benchmesh-extensions`: `DISTRIBUTION`, `server(&Host)`, `desktop(builder, stop_server)` | the stub crate `crates/benchmesh-extensions`: adds nothing |
| UI | `frontend/src/extensions/contract.ts`: `root` (rendered once above the bench, e.g. a dialog), `systemGroups` / `aboutGroups` / `onlineGroups` / `sensorsGroups` (groups at the end of Instruments - System / About / of Instruments - Online / Sensors, the last two shown only when a distribution adds groups; the official build's Sensors: Environmental sensors; each with `viewOnly` and `onStopping`, `GroupProps`), `topBarStatus` (statuses after API and Readings in the top bar, drawn with `ui/bench/StatusPill`; the official build: **Linked** / **Not linked**, opening Online), `recordingViews` (other ways to draw a recording next to the chart, in the recorder dock and the Recordings drawer: a **Chart \| <view>** switch appears only when one is added; the view gets the chart's data, `RecordingViewProps`), `titleReadouts` (readouts on the right of the title bar, before Arrange, in `.bench-title__readouts`; the official build: the lab readout - temperature, humidity, pressure and the time), `titleBadges` (small notices after the version in the title bar, e.g. "Update available", drawn as `.bench-title__badge`; `open(tab)` opens Instruments - System / About / Online, `OpenTabProps`), `panelActions` (buttons in every instrument panel's header before Park, given the instrument's id, the panel's class, its name and whether it is online; not while arranging or in a view-only session; the official build: **Report a problem**), `instrumentsSections` (sections at the top of Instruments - configuration, given the saved entries and told when one changed; the official build: **Detect**), `identityField` (shows an entry's `match` in the Instruments form instead of its port and lets the form keep it without one; `null`: the form shows the port and says the identity is not followed) | `frontend/src/extensions/index.ts`: empty lists (no view switch), `identityField: null` |

Extension routes are served behind the same cross-site guard and remote gate as every other
route, and their remote scope follows their path (`remote/scope.rs`: an unclassified path is
local-only).

## How the official build is made

The services repository checks out this repository at a release tag into a separate directory,
copies its `overlay/` over it - replacing `crates/benchmesh-extensions/` and
`frontend/src/extensions/index.ts`, adding its UI files, a Tauri capability and a Tauri config
(`--config`) with the updater settings - then tests, builds and signs the installers for every
platform and publishes them as a draft release in this repository. Its CI also composes against
`develop` of this repository, so a change to a contract here shows up there at once.

Releasing: bump the version here (`Cargo.toml`, `crates/benchmesh-app/tauri.conf.json`) on
`develop`, tag `vX.Y.Z`, push the tag; the release itself is started in the services repository.

Official builds look for updates only at `https://benchmesh.online/api/v1/update/latest.json`:
the website serves the signed `latest.json` of the latest published release here from its cache,
which this repository's `release` webhook refreshes when a release is published (hourly otherwise).

## Building your own

```bash
(cd frontend && npm ci && npm run build)
cd crates/benchmesh-app && cargo tauri build      # installers in target/release/bundle, unsigned
```

Your build is a community build: it never updates itself; build it again for a new version.
