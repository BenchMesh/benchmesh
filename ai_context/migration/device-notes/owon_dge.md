# OWON DGE2070 (driver `owon_dge`) - device notes

Dual-channel arbitrary waveform generator over **USBTMC** → class **AWG**, 2 channels.
Sources: Python driver `benchmesh-serial-service/src/benchmesh_service/drivers/owon_dge/{driver.py,manifest.json}`,
transport `transport/usbtmc.py` + `transport/utils.py`; captures `captures/20261016-201609` (read-only),
`captures/20261016-203514` (setters), `captures/20261016-204256-owon-dge2070-query_screenshot` (failed screenshot);
device id `owon-dge2070`, link `/dev/usbtmc2`.

**Citations:** `PM pN` = `DGE2000_3000_SCPI_Protocol.pdf` **PDF page** N (printed page = N−4);
`UM pN` = `DGE2000_Arbitrary_Waveform_Generator_User_Manual.pdf` PDF page N (printed = N−5).
Manuals: `/home/marek/project/BenchMeshDOC/owon_dge/doc/`. **Rule: where capture and manual disagree, the capture wins.**

---

## 1. Identity

| Item | Value |
|---|---|
| Probe | `*IDN?` (PM p13) |
| Manual format | `<Manufacturer>,<Model>,<Serial Number>,<Firmware Level>`; example is copy-pasted from another product: `Factory,XDG3202,1837001,SCPI:99.0 FV:V1.2.0` (PM p13) |
| Observed | `OWON,DGE2070,25130086,SCPI:99.0 FV:V1.0.4.7.0\n` (46 B, 8 identical replies across sessions) |
| Parse | split `,` → vendor, model, serial, firmware; 4th field = `SCPI:<ver> FV:<fw>` (space separated) |
| Match patterns | manifest: `OWON,DGE2070`, `OWON,DGE2035`, `OWON,DGE3032`, `OWON,DGE3062`, `OWON,DGE3031`, `OWON,DGE3061` → regex `^OWON,DGE(20(35|70)|30(3|6)[12]),` |
| Channels | DGE2035 / DGE2070: **2** (UM p45 "Channel 2"). DGE3032/3062/3031/3061: **not covered by either PDF** (despite the "3000" file name). Naming suggests last digit = channel count (30x2 → 2, 30x1 → 1) - **unverified**; manifest gives every model 2 via `DEFAULT` |
| Model differences (2035 vs 2070) | bandwidth 35/70 MHz, 125/300 MSa/s, sine max 35/70 MHz, square/pulse 15/20 MHz, ramp 1/2 MHz, arb 10 MHz (UM p45); pulse period min 67/50 ns (UM p47) |
| `*RST` | factory reset (PM p14) - state-changing, never automatic |
| `SYSTem:VERSion?` | `Vx.x.x` (PM p37-38) - not captured |

## 2. Link

| Item | Manual | Observed / Python |
|---|---|---|
| Physical | rear USB Device port; **Utility → USB Dev must be set to "USB TMC"** (factory default is "PC", the proprietary Waveform-Editor protocol) (UM p39, p41, p43) | Linux kernel `usbtmc` → `/dev/usbtmc2` (user config wrongly says `usbtmc0`) |
| TX terminator | not specified (PM p5 only says SCPI syntax) | Python sends `\r` (manifest `seol`) → accepted. USBTMC delimits by EOM bit, so a terminator should be unnecessary; `\n` untested (Q2) |
| RX framing | `#0…<LF><&EOI>` mentioned for blocks (PM p5) | **one USBTMC message per read, text ends `\n`**. Manifest `reol: "\r"` is wrong but harmless (Python strips `\r\n` and returns the first line) |
| Reply latency (exchange) | - | IDN 9-113 ms; `OUTPut1:STATe?` 2.7-112 ms (median 52); `OUTPut2:STATe?` sent right after: 10-90 ms (median 23); `SHAPe?` 45-115 ms; `SYMMetry?` 11-17 ms. First query of a burst is spread uniformly over ~0-110 ms (device service tick?), back-to-back queries ≈ 15-25 ms |
| Setters | no reply documented | **never reply** (4/4 `OUTPut{1,2}:STATe ON/OFF` → `no_reply`). Reading after a setter issues a USBTMC IN request that the device never answers → timeout / desync. DSL `reply: none` |
| Post-setter gap | - | not measured on DGE (Tenma needed ≥ 50 ms). Read-back after `ON` showed `1\n` on the next poll (≈1 s later) |
| Timeouts | - | kernel default 5 s (`USBTMC_IOCTL_GET/SET_TIMEOUT`). Python's `read()` swallows `OSError` and returns `b''` → errno (ETIMEDOUT, EOVERFLOW) was never visible. Rust: per-command timeout via `SET_TIMEOUT`, ~500 ms for text queries, ~10 s for screenshot; surface errno |
| Remote/local | error `-201 "Invalid while in local"` exists (PM p37, p39) | setters worked without any remote command → no `preconditions:` needed |

DSL: `link.usbtmc`, `tx.terminator: "\n"` once Q2 confirms (else `"\r"`), `rx.mode: message` (EOM) + strip trailing `\n`,
setters `reply: none`, text `rx.max_bytes: 4096`.

## 3. Command table

Channel `{ch}` ∈ 1,2. `SOURce{ch}` prefix optional (omitted → CH1, PM p17). Short forms allowed (PM p6). Booleans accept `ON|OFF|<NR1>`; boolean queries return `<NR1>` (`0`/`1`). Most numeric setters accept `MINimum|MAXimum`; queries accept `? MINimum|MAXimum` to read limits.

| Function | Set syntax | Query → reply | Params / range / units | Captured | Cite |
|---|---|---|---|---|---|
| Output on/off | `OUTPut{ch}:STATe ON\|OFF` | `OUTPut{ch}:STATe?` → `0`/`1` | bool | `0\n` ×928, `1\n` ×3 (after ON); setters no reply | PM p14 |
| Load impedance | `OUTPut{ch}:IMPedance <ohms>\|INFinity\|MIN\|MAX` | `…IMPedance?` → `<NR3>` ohms; `9.9E+37` = INF (High Z) | 1 Ω-10 kΩ, 1 Ω res, unit `OHM`; INF = >10 kΩ | - | PM p13-14, UM p38 |
| Shape | `SOURce{ch}:FUNCtion:SHAPe SINusoid\|SQUare\|PULSe\|RAMP\|PRNoise` | `…SHAPe?` → `SIN\|SQU\|PUL\|RAMP\|PRN` | no ARB/DC token documented | `SIN\n` (both ch) | PM p26 |
| Frequency | `SOURce{ch}:FREQuency[:FIXed] <f>[Hz\|kHz\|MHz]` | `…FREQuency?` → `<frequency>` (format undocumented) | 1 µHz-70 MHz (shape/model dependent, UM p45); res 1 µHz; changing shape may clamp freq | - | PM p23-24 |
| Amplitude | `SOURce{ch}:VOLTage[:AMPLitude] <a>[mVpp\|Vpp]` | `…VOLTage?` → `<amplitude>` | 1 mVpp-10 Vpp @50 Ω, 2 mVpp-20 Vpp High Z (≤10 MHz; 4/8 Vpp above) (UM p46); res 1 mVpp/4 digits | - | PM p35 |
| Offset | `SOURce{ch}:VOLTage:OFFSet <v>[mV\|V]` | `…VOLTage:OFFSet?` → `<voltage>` | ±(5 Vpk − Vpp/2) @50 Ω, ±(10 Vpk − Vpp/2) High Z (≤10 MHz) (UM p46) | - | PM p34-35 |
| Phase | `SOURce{ch}:PHASe[:ADJust] <p>[RAD\|DEG]` | `…PHASe?` → value **in RAD** | 0-2π rad / 0-360°; default unit RAD; n/a for DC & noise | - | PM p27-28 |
| Ramp symmetry | `SOURce{ch}:FUNCtion:RAMP:SYMMetry <NR2>[PCT]` | `…SYMMetry?` → `<NR2>` | 0.0-100.0 % | `50.0\n` (both ch, even while shape = SIN) | PM p26 |
| Pulse duty | `SOURce{ch}:PULSe:DCYCle <NR2>[PCT]` | `…DCYCle?` → `<percent>` | limited by min width (≥24 ns) & period; coupled to WIDTh | report #13 (DGE2070, shape SQU / SIN): every pulse query answers a value and queues `-102,"Syntax error"` - the definition (1.1.1) reads them only while SHAPE = PUL; behaviour in PUL not yet observed | PM p30-31, UM p47 |
| Pulse width / edges | `SOURce{ch}:PULSe:WIDTh\|TRANsition[:LEADing]\|TRANsition:TRAiling <t>[ns\|us\|ms\|s]` | `…?` → `<seconds>` | width ≥ 24 ns, edges ≥ 15 ns (UM p47) | - | PM p31-32 |
| Square duty | **none** (square is fixed 50 %; no SQU:DCYC command) | - | - | - | PM p26 |
| Sync output / channel couple / tracking | **no SCPI command, not in UM** (only panel "Align Phase", UM p42) | - | - | - | - |
| Modulation (summary) | `SOURce{ch}:MODE:STATe` (modulation master enable); `AM[:DEPTh]` 0-100 % `PCT`; `FM[:DEViation]` Hz; `PM[:DEViation]` RAD/DEG 0-π / 0-180°; each with `:INTernal:FREQuency` 2 mHz-100 kHz, `:INTernal:FUNCtion SINusoid\|SUQare\|RAMP\|PRNoise` (→ `SIN,SUQ,RAMP,PRN`), `:SOURce INT\|EXT`, `:STATe` bool. **FSK has no SCPI** (panel only, UM p37) | `<NR1>` / enum / number | carrier: sine/square/ramp/arb (not DC) | - | PM p15-17, p20-22, p27-30 |
| Sweep (summary) | `SOURce{ch}:SWEep:STATe` bool, `:SPACing LIN\|LOG`, `:TIME` 1 ms-500 s, `:SOURce TIM\|MAN\|EXT`; `FREQuency:STARt/STOP` or `CENTer/SPAN` | enums `LIN\|LOG`, `TIM\|MAN\|EXT` | - | - | PM p23-25, p32-34 |
| Burst (summary) | `SOURce{ch}:BURSt:STATe` bool, `:MODE TRIG\|GAT`, `:NCYCles` 1-500 000\|INF (`9.9E+37`), `:INTernal:PERiod` ns-s, `:SOURce TIM\|MAN\|EXT`, `:GATE:POLarity NORM\|INV` | enums as listed | UM spec says 1-60 000 cycles, internal/manual only | - | PM p17-20, UM p49 |
| Arbitrary upload (summary) | `DATA[:DATA] EMEMory,<IEEE block>` (alias `TRACe`) → edit memory | `DATA? EMEMory` → IEEE block | sample format/byte order **undocumented**; arb length 2-8 K points, 14-bit (UM p47); built-in arbs are 8192 pts (UM p23). No command documented to select ARB shape, a built-in arb, or to store/output the uploaded wave | - | PM p38, UM p22-23, p47 |
| Screenshot | - | `HCOPy:SDUMp:DATA?` → IEEE 488.2 definite block of a BMP | see §5 | `#6391734BM…` | PM p12 |
| Screenshot to USB stick | `HCOPy:SDUMp[:IMMediate]` | - | writes `Model/IMAGE/n.BMP` | - | PM p12-13 |
| Error queue | - | `SYSTem:ERRor[:NEXT]?` → `<NR1>,"<string>"` e.g. `-201,"Invalid while in local"` | codes 0, −101, −102, −108, −201; FIFO 64 | - | PM p36-37, p39 |
| Misc system | `SYSTem:BEEPer:STATe`, `SYSTem:BEEPer`, `SYSTem:LANguage`, `DISPlay:BRIGhtness` 0-100, `DISPlay:SAVer[:STATe]/:DELay/:IMMediate` | `<NR1>` / enum | - | - | PM p10-11, p35-37 |
| Counter | `COUNter:COUPling AC\|DC`, `:HFR`, `:SENSitivity LOW\|MIDDle\|HIGH`; `COUNter:FREQ?\|PERiod?\|DUTYcycle?\|PULSewidth?` → `<NR3>` | - | UM lists no counter input on DGE2000 → probably unsupported (Q12) | - | PM p7-9 |

## 4. Proposed AWG class status contract

Today: driver emits only `{"OUTPUT":"ON"|"OFF"}` per channel; UI `GenericAWG` renders placeholders (CLASS_CONTRACT.md §AWG).
Proposal (registry `AWG.status_ch{N}`, SI dicts as in CLASS_CONTRACT §0):

| Key | Type | Source query | Tier |
|---|---|---|---|
| `OUTPUT` | `"ON"\|"OFF"` | `OUTPut{ch}:STATe?` (`1`→ON, `0`→OFF, anything else = error, **not** OFF) | fast |
| `SHAPE` | enum `SIN\|SQU\|PUL\|RAMP\|PRN` (+ `ARB` if Q5 shows a token) | `SOUR{ch}:FUNC:SHAP?` | slow |
| `FREQ` | SI dict, Hz | `SOUR{ch}:FREQ?` (omit for PRN) | slow |
| `AMPL` | SI dict, Vpp | `SOUR{ch}:VOLT?` | slow |
| `OFFS` | SI dict, V | `SOUR{ch}:VOLT:OFFS?` | slow |
| `PHASE` | SI dict, ° (convert from rad) | `SOUR{ch}:PHAS?` (omit for PRN) | slow |
| `LOAD` | SI dict Ω, or `"HIGHZ"` when ≥ 9.9E+37 | `OUTP{ch}:IMP?` | slow |
| `SYMM` | number % | `SOUR{ch}:FUNC:RAMP:SYMM?` - only when `SHAPE=RAMP` | slow, conditional |
| `DUTY` / `WIDTH` | % / SI s | `SOUR{ch}:PULS:DCYC?`, `:WIDT?` - only when `SHAPE=PUL` | slow, conditional |
| `MODE` | `"CW"\|"MOD"\|"SWEEP"\|"BURST"` | `SOUR{ch}:MODE:STAT?`, `SWE:STAT?`, `BURS:STAT?` (+ AM/FM/PM `STAT?` to name the modulation) | slow, optional |

- **Minimal set** per channel: OUTPUT, SHAPE, FREQ, AMPL, OFFS → **10 queries** for both channels; +PHASE, LOAD → 14.
- **Cycle time** (back-to-back ≈ 15-25 ms, first query up to ~110 ms): 10 queries ≈ 0.25-0.35 s, 14 ≈ 0.35-0.45 s, worst case (every query 110 ms) ≈ 1.5 s.
- **Recommendation:** fast tier OUTPUT ×2 every 1 s (≈ 50-130 ms); slow tier every 5 s, plus cache invalidation on any `set_*` for that channel (shape change also invalidates FREQ, since the device may clamp it - PM p23). Front-panel edits are only seen by the slow tier.
- Ops (partial names): `output/{ON|OFF}`, `shape/{SIN|SQU|PUL|RAMP|PRN}`, `frequency/{Hz}`, `amplitude/{Vpp}`, `offset/{V}`, `phase/{deg}` (send with `DEG` unit), `load/{ohms|INF}`, `symmetry/{pct}`, `duty/{pct}`; GET `screenshot` (binary, §5). Features: `channels`, `channel_colors`, per-model `limits` (freq max per shape, from UM p45 or `? MAX` queries).

## 5. Binary block (screenshot)

**Reply format (manual + capture):** IEEE 488.2 definite block `#<n><len><data>` (PM p5, p12). Observed header `#6391734` (8 B), payload = BMP of **391 734 B**:
`BM`, file size `0x0005FA36` = 391 734, pixel offset 54, BITMAPINFOHEADER (40), **480 × 272** (positive height → bottom-up), 1 plane, **24 bpp**, BI_RGB. Rows 480·3 = 1440 B (no padding) × 272 = 391 680 + 54 = 391 734 ✓. Matches the 480×272 LCD (UM p49). Whole USBTMC message = 391 742 B (+ possibly a trailing `\n`, Q6). Manual's example length 377 512 belongs to another model.

**What went wrong (capture `20261016-204256`):**
1. `HCOPy:SDUMp:DATA?\r`, Python sleeps 2 s, then `read(12)` → `#6391734BM6\xfa` (33 ms - data was ready).
2. `read(65536)` → only **52 B** (rest of the BMP headers), then 15× `read(65536)` → `b''` in 0.2 ms each (errors swallowed by `UsbTmcTransport.read`).
3. Retry: re-sends the query, `read(12)` → `' \x18( \x18( …'` = **stranded pixel data** (BGR 0x20,0x18,0x28) from the first image; next attempt → nothing. Afterwards every read (even `*IDN?`) fails: `wrong bTag`, then `EOVERFLOW` (−75). `USBTMC_IOCTL_CLEAR` did not recover; needs replug.
- Interpretation: the Linux driver turns each `read(count)` into a `REQUEST_DEV_DEP_MSG_IN` with TransferSize ≈ count. The DGE apparently **ignores TransferSize and streams the whole message**; after the short read the kernel considers the transfer finished, so the remaining ~391 KB sit in the bulk-IN pipe and are later parsed as USBTMC headers (bTag mismatch / overflow). Retrying the query while data is stranded makes it worse.

**Recommended strategy (kernel `/dev/usbtmcN`):**
1. Write `HCOPy:SDUMp:DATA?` (no sleep).
2. `USBTMC_IOCTL_SET_TIMEOUT` → 10 000 ms for this command only (generation time unknown; < 2 s observed).
3. **One `read()` with a buffer ≥ the whole message** (e.g. 1 MiB) so TransferSize covers it; the kernel loops over bulk packets until EOM/short packet. Never split a message into header + chunks.
4. Validate: `#`, digit n, `len`, received ≥ 2+n+len; BMP `BM` and its size field == len; optional dims check. On mismatch → error, **no automatic retry**.
5. Restore the normal timeout.
- DSL: `reply: {ieee488_block: {max_bytes: 1048576, timeout_ms: 10000}}`. **Drop** the plan's "2 s pre-read delay, chunking, retries, partial accept" options - they were workarounds for the split-read bug.
- **Desync handling:** any errno on read (EOVERFLOW, EIO, ETIMEDOUT after a reply was expected) or a malformed reply ⇒ mark link dirty, block further commands, then recovery ladder: (a) `USBTMC_IOCTL_ABORT_BULK_IN` (kernel drains bulk-IN until a short packet) + `USBTMC_IOCTL_CLEAR_IN_HALT`; (b) `USBTMC_IOCTL_CLEAR`; (c) `USBDEVFS_RESET` on `/dev/bus/usb/BBB/DDD` (needs a udev rule granting the user access, not root); (d) close/reopen; verify with `*IDN?`. Only (b) was tried so far.
- **nusb path (later):** own USBTMC framing; must tolerate a device that sends more than TransferSize: keep reading bulk-IN until the header's TransferSize is satisfied and EOM set, and drain to a short packet on abort.

## 6. Discrepancies (manual vs capture vs Python)

| # | Topic | Manual | Capture | Python |
|---|---|---|---|---|
| 1 | IDN example | `Factory,XDG3202,…` (PM p13) | `OWON,DGE2070,25130086,SCPI:99.0 FV:V1.0.4.7.0` | patterns `OWON,DGE…` ✓ |
| 2 | RX terminator | unspecified | `\n` | manifest `reol "\r"` (harmless on USBTMC) |
| 3 | Screenshot size | example 377 512 (PM p12) | 391 734 (480×272×24bpp BMP) | docstring cites 377 512 |
| 4 | Screenshot read | one block | device streams whole message | 2 s sleep + 12 B + 64 KiB split reads + re-query retries + partial accept → wedges device; also API passes channel as `save_path` (plan bug list) |
| 5 | Error visibility | - | reads fail fast with errno | `read()` returns `b''` on any `OSError`; `query_output` maps anything ≠ `1` (incl. empty/timeout) to `"OFF"` → failures masked |
| 6 | Models | PDFs cover DGE2035/2070 only | DGE2070 | manifest lists DGE30xx with 2 ch (30x1 likely 1 ch) |
| 7 | Shapes | SHAPe has no ARB/DC (PM p26) but UM has Arbitrary key + DC built-in (UM p22-23); PM/PM:STATe mention "arbitrary carrier" (PM p30) | `SIN` only | queries SHAPe raw |
| 8 | Modulation waveform token | `SUQare` → `SUQ` (PM p16, p21, p29) vs `SQUare`/`SQU` in SHAPe - likely typo | - | - |
| 9 | Block example | `#42000` vs text "`#21000`, 2 digits" (PM p38) | - | - |
| 10 | Burst cycles / trigger | PM: 1-500 000, sources TIM/MAN/EXT, gated mode (PM p18-19); UM spec: 1-60 000, internal/manual only, I/O = USB only (UM p49) | - | - |
| 11 | Default load | PM: "default 50 Ω" (PM p13); UM factory defaults: CH1/CH2 **High Z** (UM p41) | - | - |
| 12 | FSK | in UM (p36-37, p48) | - | no SCPI command exists |
| 13 | Counter | `COUNter:*` documented (PM p7-9) | - | UM shows no counter input on DGE2000 |
| 14 | Phase unit | query returns RAD (PM p27) | - | - (UI will want degrees) |
| 15 | Poll content | - | 2 queries/2 s, only OUTPUT | UI shows placeholders; no shape/freq/ampl |

## 7. Open questions for hardware (RO = read-only query, SC = state-changing)

| # | Question | Kind |
|---|---|---|
| Q1 | Reply formats of `SOUR1:FREQ?`, `SOUR1:VOLT?`, `SOUR1:VOLT:OFFS?`, `SOUR1:PHAS?`, `OUTP1:IMP?`, `SOUR1:PULS:DCYC?` (NR2/NR3? units appended?) | RO |
| Q2 | Is `*IDN?\n` (LF) or no terminator accepted? | RO |
| Q3 | Reply to `SYST:ERR?` when empty (`0,"No error"`?); does an unknown query reply at all or just time out (and leave the link clean)? | RO (unknown query could desync - do after Q6 recovery is proven) |
| Q4 | `SOUR1:FREQ? MAX` / `VOLT? MAX` etc. per shape → feature limits | RO |
| Q5 | `SHAPe?` reply when the panel is in Arbitrary or DC (needs pressing the Arb key on the panel) | SC (panel) |
| Q6 | Screenshot after replug with single 1 MiB read: total bytes (trailing `\n`?), time to first byte without the 2 s sleep, link healthy afterwards (`*IDN?`) | RO but **risk of wedging** - have replug available |
| Q7 | Does `ABORT_BULK_IN` + `CLEAR_IN_HALT` recover a deliberately stranded transfer (avoid replug)? Is `USBDEVFS_RESET` via udev-permitted node sufficient? | RO, risky |
| Q8 | `MODE:STAT?`, `AM/FM/PM:STAT?`, `SWE:STAT?`, `BURS:STAT?`, `SYST:VERS?` replies | RO |
| Q9 | Setter formats: plain numbers vs units (`1kHz`, `500mV`, `1Vpp`, `90DEG`), precision accepted; min gap after a setter before the next command | SC |
| Q10 | Does changing SHAPE clamp FREQ/AMPL (cache invalidation rule)? | SC |
| Q11 | `DATA:DATA EMEMory` sample format (int16? 14-bit? endianness, point count) and how to select/output the uploaded wave | SC |
| Q12 | Do `COUNter:*` queries answer on DGE2070, or hang? | RO, risky (may not reply) |
| Q13 | USB VID:PID of DGE2070 from sysfs (for `bm-probe discover`) | RO (sysfs only) |
| Q14 | IDN + channel count of DGE3000 models (no hardware / docs) | RO (needs device) |
| Q15 | Latency: confirm ~110 ms first-query jitter vs ~20 ms back-to-back with a 14-query burst | RO |

## Bench verification (Rust bm-probe, 2026-10-16, after replug)
- All text queries and both channels verified live: poll ≈100-120 ms (fast group), slow group ≈480 ms first time; GETs 20-65 ms. Unverified reply formats (FREQ, VOLT, OFFS, PHAS) parse fine (e.g. FREQ 1 kHz, AMPL 1 / 7 Vpp).
- **Screenshot is not readable through the Linux `usbtmc` driver.** A single 1 MiB `read()` returns only 52 bytes (first 64-byte packet minus the 12-byte header). Repeated reads (Rust engine) collected 318 292 B, then the kernel logged `Device sent too small first packet: 10 < 12` - the DGE streams the rest of the message as raw bulk packets without valid USBTMC headers, so the kernel misparses pixel data as headers (the collected data is corrupt). A raw Python read probe left the device returning `EOVERFLOW` (kernel `wrong MsgID`, `usb_bulk_msg returned -75`); `ABORT_BULK_IN` fails with -75, `CLEAR`/`CLEAR_*_HALT` succeed but do not recover → physical replug required.
- Plan: userspace USBTMC transport (`nusb`): own header handling, keep reading bulk-in until the IEEE block length is satisfied, drain to a short packet on abort. Needs a udev rule granting the user access to the device node (and kernel-driver detach). Screenshot operation disabled in the definition until then.
