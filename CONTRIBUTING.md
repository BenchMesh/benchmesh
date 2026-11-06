# Contributing to BenchMesh

Thank you for your interest in BenchMesh. Bug reports, instrument definitions and code are all welcome.

## Reporting a bug

1. Check the [issue tracker](https://github.com/BenchMesh/benchmesh/issues) for an existing report and make sure you run the latest release.
2. Open the instrument's **console** (console icon in the panel header), reproduce the problem and press **Copy**: it gives the instrument, model, identify reply, connection and app version together with what was sent and received. Paste that into the issue.
3. Describe what you did, what you expected and what happened, and name your operating system.

## Adding or fixing an instrument

Instruments are described by device definitions (`devices/*.yaml`), not code. The format is
specified in [`ai_context/migration/DEVICE_DSL.md`](ai_context/migration/DEVICE_DSL.md); the
existing definitions are the best examples. A new model of a family usually needs only a few
lines in that family's file.

```bash
cargo run -q -p bm-devicedef -- validate devices      # must report 0 errors
```

Say in the pull request whether the definition was tested on real hardware or written from the
programming manual (with the manual's name and pages).

## Development setup

You need Rust (stable), Node.js 20 and, for the desktop app, the
[Tauri prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```bash
git clone https://github.com/BenchMesh/benchmesh.git && cd benchmesh
(cd frontend && npm ci && npm run build)
cargo run -p bm-server -- --config captures/bench-config.yaml --simulate captures/20261016-203514
# UI on http://localhost:57666/ui with a simulated bench, no hardware needed
```

The desktop app: `cargo tauri dev` in `crates/benchmesh-app` (`cargo tauri build` for installers:
unsigned, without updates - see `ai_context/OPEN_CORE.md` for what the official build adds).

## Before opening a pull request

```bash
cargo clippy --all-targets -- -D warnings
cargo test
(cd frontend && npm run lint && npx vitest run && npm run build)
```

- Work on a branch from `develop` and open the pull request against `develop`.
- Cover new behaviour with tests and update the documentation it touches (CLAUDE.md describes the architecture).
- Text uses plain hyphens, never em or en dashes (a test checks every file).
- Adding or removing a dependency: update [THIRD-PARTY-NOTICES.md](THIRD-PARTY-NOTICES.md).

## License

By contributing, you agree that your contributions are licensed under the project's [MIT license](LICENSE).
