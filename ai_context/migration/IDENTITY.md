# USB ports, USBTMC per OS, and `match`

An instrument is configured by the port it is plugged into: a config entry names its `port`
(serial) or `device` (USBTMC); the Instruments drawer lists what the OS reports
(`/serial-ports`, `/usbtmc-devices`, `bm_transport::discover`) to pick from. Works the same on
Linux, Windows and macOS without root/admin rights and without udev rules.

**`match` (finding an instrument by who it is) and Detect are in the official build only**
(benchmesh-services `docs/IDENTITY.md`, extension point `bm_server::extension::Finder`; UI
`instrumentsSections`, `identityField`). This build keeps an entry's `match` (an entry written by
the official build) but does not follow it: it opens the `port`/`device` the entry names, and an
entry with `match` and no path is refused ("serial device needs `port` (this build does not find
instruments by `match`)").

## Code

| Piece | Where |
|---|---|
| port enumeration (`Port`, `UsbInfo`, `ports()`) | `crates/bm-transport/src/discover/` (`linux.rs`, `windows.rs`, `macos.rs`) |
| in-process port lock | `crates/bm-transport/src/lock.rs` |
| connect attempts (`Attempt`, an optional identify check before the session) | `crates/bm-driver/src/actor.rs`, `Engine::open` / `start_session`; `bm_transport::attempts` |
| the finder of a distribution (`match`) | `crates/bm-server/src/extension.rs` `Finder`, `devices.rs` `hardware_connectors` / `hardware_presence` |
| what the OS reports | `cargo run -p bm-transport --example ports` |

## USBTMC instruments per OS

| OS | How BenchMesh talks to them | Port path | One-time setup |
|---|---|---|---|
| Linux | kernel `usbtmc` driver | `/dev/usbtmcN` | access to `/dev/usbtmc*` (generic rule, installer) |
| Windows | own USBTMC protocol over `nusb` | `usb:1ab1:044d:DHO8A272907001` | the instrument needs the **WinUSB** driver: **Install driver** in the Instruments drawer (one UAC prompt), unless … |
| Windows with a VISA (NI-VISA, Keysight IO Libraries, Rigol UltraSigma) | that VISA, loaded at run time | `USB0::0x1AB1::0x044D::DHO8A272907001::INSTR` | none - the VISA's driver stays |
| macOS | own USBTMC protocol over `nusb` | `usb:…` | none |

Discovery picks the right path per instrument (on Windows: WinUSB-bound → `usb:`, else the VISA
resource with the same vid/pid/serial). On Windows
USBTMC instruments are listed from the device tree (compatible id `USB\Class_FE&SubClass_03`), so
an instrument **without any driver** is listed too (`nusb` cannot see those): `/usbtmc-devices`
gives it a `note` with the instructions above, and opening it fails with them.

**Install driver** (Windows, `POST /usbtmc-devices/driver {device}`, `bm_transport::winusb`):
offered for every instrument `/usbtmc-devices` marks `driver_installable` (a note and a `usb:`
path: no WinUSB and no VISA reaches it). The server starts its own executable again elevated
(`runas` → one UAC prompt) with `--install-winusb <path> <result file>`; both binaries hand that
run to `winusb::helper_main()` first thing in `main`. The helper opens the instrument's device
node (the USBTMC interface node on a composite device), makes it a *Universal Serial Bus
devices* class device if it has no or another class, builds the class driver list from the
in-box `%WINDIR%\INF\winusb.inf` only (`DI_ENUMSINGLEINF`, `DI_FLAGSEX_ALLOWEXCLUDEDDRVS`) and
installs its *WinUsb Device* driver with `DiInstallDevice` - what Device Manager's "Let me pick"
does. Nothing is shipped, downloaded or self-signed. A declined prompt → 403. The instrument
re-enumerates, the UI re-reads the lists after 2 s, and configured instruments reconnect on
their own. Undo: Device Manager → Uninstall device (delete the driver). Replacing a vendor
driver this way hides the instrument from that vendor's software.
A USB device that is not an instrument (a distribution's sensor adapter) is named by its ids only,
`usb:1a86:5512` (`winusb::install_usb_id`, `usb_id_path`; no instrument path has that form):
the helper binds the first attached device node with those ids (`discover::windows_usb_service`
tells whether one is attached and which driver it has).
(First Windows test, 2026-10-18: neither the Rigol DHO800 nor the OWON DGE2070 was listed -
most likely because neither had a driver; not yet confirmed.)

The USBTMC protocol (`usbtmc_proto.rs`) discards replies whose bTag belongs to an earlier
request (a late answer after a timeout) and sends INITIATE_ABORT_BULK_IN after a timeout, so a
slow reply can no longer leave an instrument "one reply behind".
