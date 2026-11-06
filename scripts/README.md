# Scripts

Development tools; none of them is shipped with the app.

| Script | What it does |
|---|---|
| `bench_setters.sh` | Live setter check of the device definitions on the bench with `bm-probe`: every block sets a value, verifies it by read-back and restores what it found. Safe only with nothing connected to any instrument output or input. |
| `bench_scope_setters.sh [config] [device-id]` | The same for every RIGOL DHO800/DHO900 setter (configuration only, nothing energises). |
| `mho900_fake.py [--port N] [--idn TEXT]` | A strict fake RIGOL MHO954 on a loopback socket, written from its programming guide: runs the real server, `bm-probe` and panel without the instrument (config `transport: tcpip`, `host: 127.0.0.1:<port>`) and prints every command the guide does not document. |
| `libcheck/` | Cross-checks the definitions against the protocol tests of open-source instrument libraries (PyMeasure, InstrumentKit, QCoDeS); see its README. |
| `ui-freeze-capture.sh` | Run while the desktop app's window is frozen: which process/thread waits on what, whether the API answers, and GNOME's "frame counter" journal lines (with sudo also gdb backtraces). |
| `frame-sync/check.sh [app]` | Linux: runs a build in a nested X server under a window manager that never confirms frames (what GNOME 42 sometimes does) and tells whether the window keeps painting (`crates/benchmesh-app/src/frame_sync.rs`). Local only: needs Xephyr, gcc, libX11, ImageMagick. |
| `community-status.sh` | Refreshes the community status snapshot a release carries (`crates/bm-server/community-status.json`) from the website (`BENCHMESH_ONLINE_URL` for another site); run before tagging a release. Needs curl and jq. |
| `logo/build.mjs` | Writes the logo files, the favicon and the desktop app icons from `frontend/src/ui/kit/brand.ts` (needs Inkscape and the Tauri CLI). |

Build `bm-probe` first for the bench scripts: `cargo build -p bm-probe`.
