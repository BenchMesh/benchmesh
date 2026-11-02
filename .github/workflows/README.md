# GitHub Actions workflows

| Workflow | Runs on | What it does |
|---|---|---|
| `ci.yaml` | every push and pull request | Frontend: lint, unit tests (including the no-dash check of the whole repository), build. Rust: clippy, tests (including the device definitions against the wire captures), definition validation. Port discovery built and tested on Windows (macOS off while the Mac builds are off); there also the server is linted and its headless system code tested (start with the computer, restart, shut down). Desktop app crates (`benchmesh-app`, the `benchmesh-extensions` stub): clippy and unit tests (with the WebKitGTK packages), on x64 and 64-bit ARM. |
| `e2e-tests.yml` | pushes and pull requests to `develop`, manual | Playwright end-to-end tests of the UI against a preview build. |

## Releases

Official installers are not built here: the release process lives in the private
`BenchMesh/services` repository, which builds this repository at a version tag together with the
official extensions, signs the installers and publishes them in this repository's Releases
(`ai_context/OPEN_CORE.md`). To build installers yourself: `cargo tauri build` in
`crates/benchmesh-app` (unsigned, without updates).
