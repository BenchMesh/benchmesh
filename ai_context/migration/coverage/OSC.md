# OSC coverage: digital oscilloscopes

Class contract: `devices/classes/OSC.yaml`. Reference definition: `devices/rigol_dho800.yaml`
(bench-tested on a DHO804, see `device-notes/rigol_dho800.md`). Research date 2026-10-18.

Manuals are outside the repo in `/home/marek/project/BenchMeshDOC/<vendor>_<family>/doc/`
(abbreviated `DOC/` below), each with a `SOURCES.md`. **Page numbers are PDF pages** (the page
index of the PDF file, as `pdftotext`/viewers count them) unless marked "printed". For Keysight
and the Siglent EN11x guides: Keysight printed = PDF; Siglent printed = PDF − 1.

What the DSL can do today (relevant to scopes): text SCPI over serial and USBTMC, `reply: block`
(IEEE 488.2 definite block, **the reply must start with `#`**), `{samples: u8|i8|u16le|i16le|u16be|i16be}`
on a block payload, `fields`/`split`/`regex`/`map`/`bits` on text, per-channel `before:` steps,
`requires`, computed params (`from:` CEL), CEL status expressions. `link.tcp` (raw SCPI socket) is
implemented since 2026-10-18; every scope definition with a LAN socket offers it.

---

## 1. Ranking (hobby / small-lab prevalence)

| # | Family | Price band (new) | Protocol | Waveform transfer | Doc | Confidence | DSL fit |
|---|---|---|---|---|---|---|---|
| 1 | Rigol DS1000Z (DS1054Z, DS1104Z, MSO1000Z, DS1202Z-E) | €300-600 (used €200) | SCPI, USBTMC + LAN | IEEE block, BYTE, Rigol preamble | very good | high | **works today** |
| 2 | Siglent SDS1000X-E (SDS1104X-E, SDS1202X-E …) | €350-600 | legacy Siglent SCPI (`C1:VDIV`, `C1:WF? DAT2`) | prefixed block `C1:WF DAT2,#9…`, i8 | good | medium | **blocked**: block reply with text prefix |
| 3 | Rigol DHO800/DHO900 | €300-900 | SCPI, USBTMC | IEEE block, BYTE | very good | **bench-tested** | works (`rigol_dho800.yaml`) |
| 4 | Siglent SDS800X HD / SDS1000X HD / SDS2000X Plus / SDS2000X HD | €400-2500 | Siglent SCPI tree (`:CHANnel1:SCALe`, `:WAVeform:…`) | IEEE block (i8 / i16le), **binary** 346-byte preamble | very good | medium-high | **mostly works**; scaling from text queries + per-model `code_per_div` (binary preamble needs a struct step) |
| 5 | Rigol DHO1000 / DHO4000 | €700-2000 | SCPI (DHO800 dialect) | IEEE block, BYTE/WORD | very good | high | works (≈ DHO800 definition) |
| 6 | Keysight InfiniiVision 1000X / 1200X (EDUX1052G, DSOX1204G), 2000X, 3000T | €600-6000 | Keysight SCPI (Rigol's ancestor) | IEEE block, BYTE/WORD, 10-field text preamble | excellent | high | **works today** |
| 7 | Rigol MSO5000 / DS2000A | €700-1500 (used) | SCPI (DS1000Z dialect) | IEEE block, BYTE | very good | high | works today |
| 8 | Tektronix TBS1000C / TBS1000B / TBS2000B | €500-1500 (schools) | Tek SCPI (`CH1:SCAle`, `CURVe?`) | IEEE block, RI (i8), `WFMOutpre?` | good | medium-high (TBS1000C) / low (TBS2000B, no manual fetched) | **works today** (`HEADer OFF`) |
| 9 | UNI-T MSO/UPO2000 (UPO2000CS/E), MSO2000X/3000X, UPO1000 | €300-900 | Rigol-like SCPI | IEEE block, BYTE (UPO2000), WORD (MSO2000X), Rigol preamble | fair (translated) | medium | works today (byte order of WORD unconfirmed) |
| 10 | GW Instek GDS-1000B (GDS-1054B/1072B/1102B) | €350-700 | GW SCPI over **USB CDC-ACM (virtual COM)** | `#<n><len>` block, i16be, text settings header | good | medium | works today (serial link) |
| 11 | OWON XDS2000/3000, TAO3000, SDS(-E), HDS200 handheld | €150-600 | OWON SCPI (USB vendor bulk / LAN) | **4-byte length prefix** + i16le, JSON header | poor | low | **blocked**: non-IEEE length prefix, string values with units, unknown transport |
| 12 | Hantek DSO2000 (DSO2C10/2C15/2D10/2D15) | €150-250 | Hantek SCPI subset (USBTMC) | custom `#9…` multi-packet frame, all channels | poor | low | **blocked**: custom framing, no RUN/STOP or measurement commands |
| 5b | Rigol MHO900 (MHO934/954/984) | €1000-2000 | SCPI (DHO dialect) | IEEE block, BYTE/WORD | very good | high | works (`rigol_mho900.yaml`, analog) |
| - | Hantek 6022BE/BL | €60 | raw USB (Cypress FX2, needs firmware upload) | - | none (sigrok) | - | not SCPI; out of scope |
| - | Micsig TO/STO/ATO tablet scopes | €400-1500 | not verified (no manual obtained) | - | none found | - | unknown |
| - | FNIRSI (1013D, DPOX180H, 2C23T, DSO152) | €50-150 | no remote control interface (USB = mass storage / charging) | - | - | - | none |

Price bands are street prices in 2026, indicative only.

---

## 2. Per family

### 2.1 Rigol DS1000Z (DS1054Z / DS1074Z / DS1104Z, MSO1000Z, DS1000Z-E)

**Docs:** `DOC/rigol_ds1000z/doc/DS1000Z_ProgrammingGuide_EN.pdf` (MSO1000Z/DS1000Z, Dec 2015,
260 pp.), `DOC/rigol_ds1000z/doc/DS1000ZE_ProgrammingGuide_EN.pdf` (DS1000Z-E, Apr 2020, 218 pp.).
Pages = PDF (printed `2-N` = PDF N+16). Very good, old.
**Models:** DS1054Z (50 MHz), DS1074Z(-S)(Plus) (70 MHz), DS1104Z(-S)(Plus) (100 MHz), MSO1074Z/1104Z(-S)
(+16 digital), all 4 ch (p4); DS1102Z-E/DS1202Z-E 2 ch 100/200 MHz (E guide p4). V/div 1 mV-10 V at 1X
(p30); offset ±2 V below 500 mV/div, ±100 V above (p27). Timebase 5 ns-50 s, **12 horizontal
divisions** (p173, p23). Memory 12 Mpts (24 Mpts option), split by active channels (p21).
**Interfaces:** USBTMC **`1ab1:04ce`** (p14; Z-E the same), LAN VXI-11/LXI; raw socket 5555
(community/sigrok, not in the guide). Terminator `\n`; blocks `#9…<data>\n`.
**Identify:** `RIGOL TECHNOLOGIES,DS1104Z,DS1ZA…,00.04.04.SP4` (format; regex
`^RIGOL TECHNOLOGIES,(DS1\d{3}Z|MSO1\d{3}Z)`). Some units report `RIGOL TECHNOLOGIES,DS1104Z Plus,…`.

**Commands - same as `rigol_dho800.yaml` except:**
| Item | DS1000Z | Page |
|---|---|---|
| channel | `:CHANnel{ch}:DISPlay/SCALe/OFFSet/COUPling AC\|DC\|GND/PROBe 0.01…1000/BWLimit 20M\|OFF/INVert`; replies like `1.000000e+01` | p25-30 |
| timebase | `:TIMebase[:MAIN]:SCALe/OFFSet` | p172-173 |
| trigger | `:TRIGger:STATus?` TD/WAIT/RUN/AUTO/STOP; `:TRIGger:SWEep AUTO\|NORMal\|SINGle` | p175 |
| edge source | `:TRIGger:EDGe:SOURce CHANnel1-4\|D0-D15\|AC` - **`AC`**, not `ACLine` | p178-179 |
| acquire | `:ACQuire:TYPE NORMal\|AVERages\|PEAK\|HRESolution`, `:ACQuire:SRATe?` | p22-23 |
| run | `:RUN :STOP :SINGle :AUToscale :CLEar` | p18-19 |
| measure | `:MEASure:ITEM? VPP,CHANnel{ch}`; `9.9E37` = invalid (shown for cursors p47; ds1054z library treats it as no value) | p141 |
| waveform | `:WAV:SOUR CHAN{ch}`, `:WAV:MODE NORMal\|MAXimum\|RAW`, `:WAV:FORM BYTE\|WORD\|ASCii`, `:WAV:DATA?`, `:WAV:PRE?` | p233-242 |
| screenshot | `:DISPlay:DATA? ON,OFF,PNG` (color, invert, format); a lone `PNG` lands in the colour slot; default BMP24 = 1 152 054 bytes; strip the trailing `\n` | p79 |

**Waveform:** NORMal mode = **1200 points** (screen; XINC = timebase/100), BYTE → `u8`, WORD has 8 valid
bits (`u16le`). Preamble (p242) = the DHO800 10 fields, e.g. `0,2,6000000,1,1.000000e-09,-3.000000e-03,0,4.132813e-01,0,122`;
NORMal-mode **YREF = 127** (DHO800: 128). `volts = (code − yorig − yref)·yinc` (unchanged). RAW: stop first,
≤ 250 000 points (BYTE) / 125 000 (WORD) / 15 625 (ASCii) per `:WAV:DATA?`, chunk with `:WAV:STAR/STOP`
(p235-237).
**Quirks:** occasional short/truncated blocks (sigrok discards them); slow copy from memory to the
output buffer (sigrok sleeps 100 ms below 15 k points, else 1 s); `*OPC?` between commands helps (EEVblog,
DS1074Z Linux usbtmc timeouts). The python `ds1054z` library stops the scope for RAW reads and uses
`:DISP:DATA? ON,OFF,PNG`.
**Confidence:** high. **DSL fit:** works today - copy `rigol_dho800.yaml`, change edge source `AC`, YREF
comes from the preamble anyway, features (12 div, 1 mV min, 50 s max), screenshot command.

### 2.2 Rigol DHO1000 / DHO4000

**Doc:** `DOC/rigol_dho1000/doc/DHO1000-4000_ProgrammingGuide_EN.pdf` (PGA34101-1110, 453 pp.; identical
copy in `DOC/rigol_dho4000/doc/`). Pages = PDF (printed = PDF − 22). Same tree as the DHO800/900 guide.
**Models:** DHO1072/1074 (70 MHz), DHO1102/1104 (100 MHz), DHO1202/1204 (200 MHz), 2/4 ch + EXT, 2 GSa/s,
12-bit, 50 Mpts (100 Mpts option); DHO4204/4404/4804 (200/400/800 MHz, 4 ch, 4 GSa/s, 250/500 Mpts).
V/div: DHO1000 500 µV-10 V (1 MΩ only); DHO4000 100 µV-10 V at 1 MΩ, 100 µV-1 V at 50 Ω (p98).
Offset DHO1000: ±1 V ≤ 65 mV/div, ±10 V ≤ 270 mV, ±20 V ≤ 2.75 V, ±100 V above (p96-97). Timebase
DHO1000 2 ns (200 MHz) / 5 ns - 1 ks; DHO4000 500 ps - 1 ks.
**Interfaces:** USBTMC vid `1ab1` (**pid not found**; DHO800 is `044d` - match on vid), LAN VXI-11/LXI.
**Same as `rigol_dho800.yaml` except:** `:CHANnel{ch}:BWLimit OFF|20M|250M` (p92; 250M presumably
DHO4000), `:CHANnel{ch}:IMPedance OMEG|FIFTy` (DHO4000), `:ACQuire:TYPE` adds `HRESolution` and `ULTRa`
(p40), edge source includes `ACLine`/`EXT` (reply `ACL`, p305), `:AUToset` (p31; DHO800 definition uses
`:AUToscale`, which works via the `:AUT` short form but is not documented), `:TFORce` (p28-30),
`:MEASure:ITEM` (p196), `:DISPlay:DATA? BMP|PNG|JPG` (p138). Waveform section identical: NORMal 1000
points, preamble up to 50 000 000 points, YREF depends on format (p430-442), no per-read RAW limit stated.
`9.9E37` is not documented in either DHO guide (seen on the DHO804 bench).
**Confidence:** high (USB pid unknown). **DSL fit:** works today - the DHO800 definition with extra
models/features (12-bit data still read as BYTE in NORMal mode; WORD would need `u16le`).

### 2.3 Rigol MSO5000 and DS2000A / MSO2000A

**MSO5000** - `DOC/rigol_mso5000/doc/MSO5000_ProgrammingGuide_EN.pdf` (PGA25105-1110, May 2025, 282 pp.;
printed `2-N` = PDF N+16). MSO5072/5102 (2 ch), MSO5074/5104/5204/5354 (4 ch), 70-350 MHz, 8 GSa/s,
16 digital, 100/200 Mpts (p3-4). V/div 500 µV-10 V (p65); offset ±1 V (1-50 mV/div), ±30 V (51-260 mV),
±100 V (p64). Timebase 1/2/5 ns - 1000 s, 10 div (p189-190). USBTMC **`1ab1:0514`** (p269; sigrok
udev rules list `0515`). Same as the DHO800 definition except: BWLimit per model (5354 `20M|100M|200M|OFF`,
5204 `20M|100M|OFF`, others `20M|OFF`, p62), probe from 0.0001 (p65), edge source `ACLine` (reply
`ACL`, p197), `:ACQuire:TYPE NORM|AVER|PEAK|HRES` (p21), `:AUToscale :CLEar :RUN :STOP :SINGle` (p18-19),
`:MEASure:ITEM?` (p136), waveform NORMal 1000 points, **YREF fixed 128**, WORD = 8 valid bits (p259-267);
screenshot **`:DISPlay:DATA?` without argument, BMP only** (p88; raise `max_bytes`). Confidence high;
works today.

**DS2000A / MSO2000A** - `DOC/rigol_ds2000a/doc/MSODS2000A_ProgrammingGuide_EN.pdf` (PGA18106-1110,
May 2019, 410 pp.). DS/MSO2102A, 2202A, 2302A(-S): 100/200/300 MHz, **2 ch** (p4-5; DS2072A not in this
edition, sigrok lists it). V/div 500 µV-10 V (1 MΩ), 500 µV-1 V (50 Ω), `:CHANnel{ch}:IMPedance OMEG|FIFTy`
(p101-103). Timebase 1/2/5 ns - 1 ks (p292), **14 horizontal divisions**. 14 Mpts (56 Mpts option).
USBTMC **`1ab1:04b0`** (p394, p403). Differences from the DHO800 definition: BWLimit `20M|100M|OFF` (p98);
edge source `CHANnel1|2|EXT|ACLine|D0-15` (p319); **no `:MEASure:ITEM`** - use `:MEASure:VPP? CHANnel{ch}`,
`:MEASure:VAVG? CHANnel{ch}`, `:MEASure:VRMS? CHANnel{ch}`, `:MEASure:FREQuency? CHANnel{ch}`, each
`9.9e37` when invalid (p188, p218, p226, p228); waveform NORMal **1400 points**, YREF always 127, WORD =
8 valid bits (p377-390); RAW ≤ 250 000/125 000/15 625 points per read (p382) or a streaming mode
(`:WAV:RES`, `:WAV:BEG`, `:WAV:STAT?` → `IDLE,n`/`READ,n`, `:WAV:DATA?`, `:WAV:END`); screenshot
`:DISPlay:DATA?` without argument → BMP 800×480×24, 1 152 054 bytes (p125). Quirk: sigrok sleeps 500 ms
around single-shot captures on this generation. Guide has typos. Confidence high; works today.

---

### 2.4 Siglent SDS1000X-E (legacy dialect)

**Models:** SDS1052X-E/1102X-E/1202X-E (2 ch, 50/100/200 MHz), SDS1104X-E/1204X-E (4 ch, 100/200 MHz);
same legacy guide covers SDS2000X-E, SDS1000X/X+, SDS1000X-U, SDS2000X (older). 8-bit ADC,
**14 horizontal divisions** (time formula uses grid = 14), 8 vertical; V/div 500 µV-10 V (p49);
timebase 1 ns-100 s (p173); memory 14 Mpts.
**Interfaces:** USBTMC `f4ec:ee38` (SDS1104X-E, per sigrok udev rules; some units `f4ed:ee3a`),
LAN VXI-11, raw socket 5025, telnet 5024. Terminator `\n`.
**Doc:** `DOC/siglent_sds1000xe/doc/Oscilloscope_ProgrammingGuide_legacy.pdf` - *Digital Oscilloscope
Series Programming Guide EN02E*, 336 pp. Good, many examples. Not in the "Supported Models" list
of the new EN11x guide (SDS1000X-E stays on the legacy set).

**Identify:** `*IDN?` → `Siglent Technologies,SDS1104X-E,SDS1EBAX…,8.2.6.1.37R9` (community example;
format p.30 of the new guide is identical). Regex: `^Siglent Technologies,SDS1\d0[24]X-E,`.

**Reply headers:** default `CHDR SHORT` → `C1:VDIV 5.00E-01V`. **`CHDR OFF`** (p24) omits the header
**and the units** → `5.00E-01`. Put `CHDR OFF` in `session.on_connect`; every query then parses with
`[trim, number]`.

| Operation | Command (query / set) | Reply (CHDR OFF) | Page |
|---|---|---|---|
| display | `C{ch}:TRA?` / `C{ch}:TRA ON\|OFF` | `ON`/`OFF` | p47 |
| scale | `C{ch}:VDIV?` / `C{ch}:VDIV <v>` (500uV…10V) | `5.00E-01` | p49 |
| offset | `C{ch}:OFST?` / `C{ch}:OFST <v>` | `-5.00E-01` | p45 |
| coupling | `C{ch}:CPL?` / `C{ch}:CPL A1M\|D1M\|GND` (A50/D50 n/a on X-E) | `D1M` → map D1M→DC, A1M→AC | p44 |
| probe | `C{ch}:ATTN?` / `C{ch}:ATTN 1\|10\|100…` | `1.00E+01` | p42 |
| bwlimit | `C{ch}:BWL?` / `BWL C1,ON` (form `BANDWIDTH_LIMIT <ch>,<ON\|OFF>`) | `ON`/`OFF` → 20M/OFF | p43 |
| invert | `C{ch}:INVS?` / `C{ch}:INVS ON\|OFF` | `ON`/`OFF` | p50 |
| timebase | `TDIV?` / `TDIV <v>` (1NS…100S list, accepts numbers) | `5.00E-09` | p173 |
| timebase_offset | `TRDL?` / `TRDL <t>` | seconds | p174 |
| trigger_sweep | `TRMD?` / `TRMD AUTO\|NORM\|SINGLE` (query can also return `STOP`) | | p187-188 |
| trigger_source | `TRSE?` → `EDGE,SR,C1,HT,OFF` → `{split: {sep: ",", index: 2}}`; set `TRSE EDGE,SR,C{n},HT,OFF` | | p190-191 |
| trigger_slope | `<src>:TRSL?` / `<src>:TRSL POS\|NEG\|WINDOW` - **prefixed by the trigger source** | | p194 |
| trigger_level | `<src>:TRLV?` / `<src>:TRLV <v>` - prefixed by the trigger source | | p185 |
| trigger_status | `SAST?` → `Trig'd`, `Ready`, `Auto`, `Stop`, `Arm`, `Roll` | | p34 |
| sample_rate | `SARA?` → `5.00E+08` | | p35 |
| acquire_type | `ACQW?` / `ACQW SAMPLING\|PEAK_DETECT\|AVERAGE,<n>\|HIGH_RES` | | p30 |
| run | `ARM` (run/arm), `STOP`, `TRMD SINGLE`, `ASET` (autoset) | none | p28-29, p40 |
| measurement | `C{ch}:PAVA? PKPK` (MEAN, RMS, FREQ) → `PKPK,3.16E+00` → split index 1; invalid → `****` | | p118 |
| waveform | `WFSU SP,<n>,NP,<n>,FP,0` then `C{ch}:WF? DAT2` | see below | p247-256 |
| screenshot | `SCDP` → raw BMP (**no block header**) | | p143 |

Trigger level/slope need the source as prefix: a computed param `from: tsrc_q` (CEL over the cached
`TRSE?` split) gives `{src}:TRLV?` - but computed params are for setters; for the *query* use a
`before` that refreshes `tsrc_q`, or simply poll `C1:TRLV?` … per channel (`{ch}`) and pick the one
matching the source in CEL (works today, 4 small queries).

**Waveform (p247-254):** `C1:WF? DAT2` returns
`C1:WF DAT2,#9000000070<70 bytes>\n\n` (header text, then an IEEE block, then two LF). With
`CHDR OFF` community reports the prefix shrinks to `DAT2,#9…` (not documented) - **either way the
block does not start with `#`** → today's framer rejects it. Samples are **signed 8-bit** ("if > 127
subtract 256", p249). Conversion (p248-249):
`volts = code · (VDIV / 25) − OFST`; `time = −(TDIV · 14 / 2) + i / SARA` (the guide ignores TRDL;
the real first-sample time is `−TDIV·7 − TRDL`, sign to confirm on hardware).
Status mapping: `WAVE_Y0 = OFST·25/VDIV`, `WAVE_DY = VDIV/25`, `WAVE_X0 = −7·TDIV − TRDL`, `WAVE_DX = 1/SARA`
(with sparsing: `SP/SARA`).
Points: `WF? DAT2` returns the **whole memory depth** (up to 14 Mpts) unless limited: `WFSU SP,<s>,NP,0,FP,0`
(p255) with `s = SANU? C1 / 1400` (computed param from `SANU?`) gives ≈ 1400 screen-like points.
Note `WFSU NP` caps the count but does not decimate.
**Quirks:** data may contain `0x0A`; do not use a line terminator for the read (p247). Large reads need a
long timeout. SDS1000X-E firmware 1.3.27+ fixed several SCPI bugs (EEVblog); older firmware returns
headers even with CHDR OFF on some queries.
**Confidence:** medium (well documented, prefix + CHDR behaviour must be captured).
**DSL fit:** everything except waveform and screenshot works today (session `CHDR OFF`, `split`,
`map`). Waveform needs **`reply: {block: {skip_to: "#"}}`** (scan for `#` before the IEEE header) -
small framer change; screenshot needs a "read until idle / BMP length from header" reply.

### 2.5 Siglent SDS800X HD, SDS1000X HD, SDS2000X Plus, SDS2000X HD (and SDS3000X HD, SDS5000X)

**Models:** SDS804X HD/814X HD/824X HD (4 ch, 70/100/200 MHz, 12-bit); SDS1102X HD…SDS1204X HD
(2/4 ch, 100/200 MHz, 12-bit); SDS2104X Plus/2204X Plus/2354X Plus (4 ch, 8-bit, 10-bit mode);
SDS2104X HD…2504X HD (12-bit). 10 horizontal divisions (p760), 8 vertical. V/div 500 µV-10 V
(HD: 500 µV/div at 1X; model dependent, p59 "varies from the models").
**Firmware:** new command set from SDS2000X Plus 1.3.5R3, SDS2000X HD 1.2.0.2, SDS1000X HD 1.1.0.2,
SDS800X HD 1.1.3.1 (Supported Models table, `sig_new` p18).
**Interfaces:** USBTMC (vid `f4ec`, pid not confirmed for these models; match on vid like the
DHO800), LAN VXI-11, raw socket **5025** (p25), telnet 5024 (p24). Terminator `\n`.
**Docs:** `DOC/siglent_sds2000xhd/doc/SDS2000XHD_Series_ProgrammingGuide.pdf` (EN11G, 855 pp., same
document as `DOC/siglent_sds800xhd/doc/SDS800XHD_Series_ProgrammingGuide.pdf`),
`DOC/siglent_sds1000xhd/doc/SDS1000XHD_Series_ProgrammingGuide.pdf` and
`DOC/siglent_sds2000xplus/doc/SDS_Series_ProgrammingGuide.pdf` (EN11F, 791 pp.). Pages below: EN11G.
Excellent structure; replies are plain values (no headers, no units).

**Identify (p31):** `Siglent Technologies,<model>,<serial 14 chars>,<fw>` e.g.
`Siglent Technologies,SDS5104X,SDS5XDAD2R0160,4.6.0.8.7R1`. Regex `^Siglent Technologies,SDS8[0-2]4X HD,` etc.

| Operation | Command | Reply | Page |
|---|---|---|---|
| display | `:CHANnel{ch}:SWITch?` / `ON\|OFF` (`:VISible` = show/hide trace only) | `ON`/`OFF` | p61, p63 |
| scale | `:CHANnel{ch}:SCALe?` / `<NR3>` | `5.00E-02` | p59 |
| offset | `:CHANnel{ch}:OFFSet?` / `<NR3>` | `-3.8E+00` | p57 |
| coupling | `:CHANnel{ch}:COUPling DC\|AC\|GND` | `AC` | p52 |
| probe | `:CHANnel{ch}:PROBe?` → `1.00E+02`; set `:CHANnel{ch}:PROBe VALue,<r>` (or `DEFault`) | | p58 |
| bwlimit | `:CHANnel{ch}:BWLimit FULL\|20M\|200M` | `20M` | p51 |
| invert | `:CHANnel{ch}:INVert ON\|OFF` | | p54 |
| timebase | `:TIMebase:SCALe?` / `<NR3>` | `1.00E-07` | p477 |
| timebase_offset | `:TIMebase:DELay?` / `<NR3>` | `1.00E-05` | p474 |
| trigger_sweep | `:TRIGger:MODE SINGle\|NORMal\|AUTO\|FTRIG` | | p483 |
| trigger_status | `:TRIGger:STATus?` → `Arm\|Ready\|Auto\|Trig'd\|Stop\|Roll` | | p484 |
| trigger_source | `:TRIGger:EDGE:SOURce C<n>\|EX\|EX5\|LINE` | `C1` | p496 |
| trigger_slope | `:TRIGger:EDGE:SLOPe RISing\|FALLing\|ALTernate` | | p495 |
| trigger_level | `:TRIGger:EDGE:LEVel <NR3>` | `5.00E-01` | p493 |
| acquire_type | `:ACQuire:TYPE NORMal\|PEAK\|AVERage[,n]\|ERES[,bits]` | | p48 |
| sample_rate | `:ACQuire:SRATe?` | `5.00E+09` | p47 |
| memory points | `:ACQuire:POINts?` | `1.25E+08` | p43 |
| run | `:TRIGger:RUN`, `:TRIGger:STOP`, `:TRIGger:MODE SINGle`, `:AUToset` | none | p484-485, p34 |
| measurement | `:MEASure ON` (p337), `:MEASure:MODE SIMPle` (p365), `:MEASure:SIMPle:SOURce C{ch}` (p368), `:MEASure:SIMPle:VALue? PKPK\|MEAN\|RMS\|FREQ` | `2.000E+00`; invalid → `****` (community) | p369 |
| waveform | see below | | p748-762 |
| screenshot | `:PRINt? PNG` (or BMP) → IEEE block | | p34 |

Measurements read the *simple measurement's* source, so per channel use `before:
[meas_src]` (`:MEASure:SIMPle:SOURce C{ch}`), like DHO800's `:WAVeform:SOURce`. This changes the
on-screen measurement source each poll (visible to the user); alternative is `:MEASure:ADVanced:P<n>`
slots (p339-347), one per channel×item - more setup, no flicker.

**Waveform (p748-762):**
- `:WAVeform:SOURce C{ch}` (p749), `:WAVeform:WIDTh BYTE|WORD` (p754; **WORD required when ADC > 8 bit**,
  i.e. all HD models), `:WAVeform:BYTeorder LSB` (default, p748), `:WAVeform:STARt 0` (p750),
  `:WAVeform:POINt <n>` (p752), `:WAVeform:INTerval <k>` (decimation, p751), `:WAVeform:MAXPoint?` (p753).
- `:WAVeform:DATA?` (p758) → `#9000001000<data>\n\n` (the trailing **two** LF are documented, p759).
  Samples are **signed**: BYTE → `i8`; WORD → `i16le`, 12-bit data **left-aligned** (low bits zero).
- **Default transfer = whole memory** (up to 10 Mpts per piece on SDS2000X Plus, p753). For a screen
  trace set `:WAVeform:POINt 1000` and `:WAVeform:INTerval {k}` with `k = max(1, floor(ACQ:POINts / 1000))`
  (a computed param `from:` over `acq_points_q`) as `before` steps.
- `:WAVeform:PREamble?` (p755-757) is a **binary** IEEE block: 346-byte WAVEDESC, little-endian:
  `@156 f32 vertical_gain (V/div, without probe)`, `@160 f32 vertical_offset`, `@164 f32 code_per_div`,
  `@172 i16 adc_bit`, `@176 f32 horiz_interval (s)`, `@180 f64 horiz_offset (delay)`, `@324 i16 timebase enum`
  (table p757), `@328 f32 probe`, `@60 i32 data bytes`, `@116 i32 point count`.
- Conversion (p759-760): `volts = code · (vdiv / code_per_div) − voffset`,
  `time = delay − timebase · 10/2 + i · interval`. Example: 8-bit `code_per_div = 30`. For 12-bit WORD
  data left-aligned in 16 bits the effective `code_per_div = 30 · 256 = 7680` (inferred from the
  left alignment; confirm on hardware - the preamble reports it).
- Status today without parsing the binary preamble: `WAVE_DY = SCALe / cpd`, `WAVE_Y0 = OFFSet · cpd / SCALe`,
  `WAVE_X0 = DELay − 5 · TIMebase:SCALe`, `WAVE_DX = k / ACQuire:SRATe` with `cpd` a per-model
  feature constant (30 or 7680). `:CHANnel:SCALe?` includes the probe ratio, so volts come out at the
  probe tip (what the user wants).
**Quirks:** the guide shows `" #9…"` with a leading space in one place (p755) and the example code
searches for `#` (`recv_all.find(b'#')`, PDF p840) - capture first. Large default transfers will time
out if `POINt` is not set. `:CHANnel:SWITch OFF` + `:WAVeform:SOURce` on that channel → error; use
`requires: disp == 'ON'` as in DHO800.
**Confidence:** medium-high (excellent manual; preamble constants and the leading-space question need
a capture). **DSL fit:** works today with per-model `cpd`; a `{struct: {...}}` parse step for little-endian
binary preambles would make it exact (and also serves the legacy WAVEDESC).

---

### 2.6 Keysight InfiniiVision 1000X / 1200X, 2000X, 3000T

**Models:** EDUX1002A/G, EDUX1052A/G (2 ch, 50 MHz), DSOX1102A/G (2 ch, 70/100 MHz), DSOX1202A/G,
DSOX1204A/G (4 ch, 70-200 MHz); DSOX/MSOX2002A…2024A (70-200 MHz); DSOX/MSOX3000T (100 MHz-1 GHz).
V/div 500 µV-10 V (1000X), 1 mV-5 V (2000X); timebase 5 ns-50 s (1000X).
**Interfaces:** USBTMC (Keysight vid `2a8d`, older Agilent units `0957`; pids not confirmed - match on
vid), LAN VXI-11 + raw socket **5025** (5024 = telnet prompt; binary data over 5024 is corrupted, p630).
Terminator `\n`; every reply ends `\n` (block too).
**Docs:** `DOC/keysight_1000x/doc/1000X_ProgrammersGuide_v01.20_akmetron.pdf` (v01.20, 2019, 934 pp.,
mirror; official 1200X/EDUX1052 guide 9018-07747 is behind a bot check, URL in SOURCES.md),
`DOC/keysight_2000x/doc/2000X_ProgrammersGuide_xdevs.pdf` (v02.30, 932 pp., mirror). 3000T guide not
fetched (same command tree). Excellent quality.
**Identify (p116):** `KEYSIGHT TECHNOLOGIES,DSOX1204G,CN12345678,02.12.2021071625`; Agilent-era 2000X:
`AGILENT TECHNOLOGIES,DSO-X 2024A,…` → regex `^(KEYSIGHT|AGILENT) TECHNOLOGIES,(DSO|MSO|EDU)[- ]?X ?\d{4}`.

| Operation | Command | Reply | Page (1000X) |
|---|---|---|---|
| display | `:CHANnel{ch}:DISPlay 1\|0` | `1`/`0` | p207 |
| scale | `:CHANnel{ch}:SCALe <v>` | NR3 | p219 |
| offset | `:CHANnel{ch}:OFFSet <v>` | NR3 | p211 |
| coupling | `:CHANnel{ch}:COUPling AC\|DC` (no GND) | | p206 |
| probe | `:CHANnel{ch}:PROBe <ratio>` | NR3 | p212 |
| bwlimit | `:CHANnel{ch}:BWLimit 1\|0` | `1`/`0` | p205 |
| invert | `:CHANnel{ch}:INVert 1\|0` | | p209 |
| timebase | `:TIMebase:SCALe <s>` | | p567 |
| timebase_offset | `:TIMebase:POSition <s>` | | p564 |
| trigger_sweep | `:TRIGger:SWEep AUTO\|NORMal` (single = `:SINGle`) | | p584 |
| trigger_source | `:TRIGger[:EDGE]:SOURce CHANnel<n>\|EXTernal\|LINE\|WGEN` | `CHAN1` | p590 |
| trigger_slope | `:TRIGger[:EDGE]:SLOPe POSitive\|NEGative\|EITHer\|ALTernate` | `POS` | p589 |
| trigger_level | `:TRIGger[:EDGE]:LEVel <v>` | | p587 |
| trigger_status | `:OPERegister:CONDition?` bit 3 = Run, bit 5 = Wait-for-trigger (`bits` parse); `:TER?` = triggered since last read (clears) | int | p150, p164 |
| acquire_type | `:ACQuire:TYPE NORMal\|AVERage\|HRESolution\|PEAK` | `NORM` | p189 |
| sample_rate | `:ACQuire:SRATe?` | NR3 | p188 |
| run | `:RUN`, `:STOP`, `:SINGle`, `:AUToscale` | none | p159-163, p137 |
| measurement | `:MEASure:VPP? CHANnel{ch}`, `:MEASure:VAVerage? DISPlay,CHANnel{ch}`, `:MEASure:VRMS? DISPlay,DC,CHANnel{ch}`, `:MEASure:FREQuency? CHANnel{ch}`; **9.9E+37** = not measurable | NR3 | p384, p380, p385, p360, p347 |
| waveform | `:WAVeform:SOURce CHANnel{ch}`, `:WAVeform:FORMat BYTE`, `:WAVeform:POINts:MODE NORMal`, `:WAVeform:POINts 1000`, `:WAVeform:PREamble?`, `:WAVeform:DATA?` | | p642, p632, p635, p633, p637, p630 |
| screenshot | `:DISPlay:DATA? PNG,COLor` → IEEE block | | p234 |

**Waveform:** `:WAV:DATA?` → `#800001000<1000 bytes>\n` (IEEE block, p630-631). BYTE data is
**unsigned** by default (`:WAVeform:UNSigned`, p648) → `u8`; WORD is MSB-first unless
`:WAVeform:BYTeorder LSBFirst` (p628, p632) → `u16be`. Special codes: `0x00` hole, `0x01` clipped low,
`0xFF` clipped high (p630). Preamble (p637-638): text
`format,type,points,count,xincrement,xorigin,xreference,yincrement,yorigin,yreference` - **the same 10
fields as Rigol**. Conversion (p631): `volts = (code − yreference) · yincrement + yorigin`,
`time = (i − xreference) · xincrement + xorigin`. Status: `WAVE_Y0 = yref − yorig / yinc`, `WAVE_DY = yinc`,
`WAVE_X0 = xorig − xref · xinc`, `WAVE_DX = xinc`. NORMal mode ≤ 1000 points (screen, "measurement
record" up to 62 500, p635); RAW/MAXimum only when stopped (≤ 100 000 points on 1000X, p635).
**Quirks:** a hidden channel → `:WAV:DATA?` error (use `requires`). Booleans reply `1/0` (map like
DHO800). `:TER?` clears on read - do not poll it in two groups.
**Confidence:** high. **DSL fit:** works today; the DHO800 definition is a near drop-in
(`:TIMebase:MAIN:SCALe` → `:TIMebase:SCALe`, measurement syntax, status register for trigger state,
no GND coupling).

---

### 2.7 Tektronix TBS1000C (and TBS1000B/EDU, TBS2000B)

**Models:** TBS1052C/1072C/1102C/1202C (2 ch, 50-200 MHz, 1 GS/s, record 2000 or 20 000 points,
p98). TBS1000B/TBS1000B-EDU and TDS2000C use the older Tek "TDS2000" command set (`TRIGger:MAIn`,
`WFMPre`), TBS2000B (TBS2072B/2104B…) is closer to the TBS1000C (`TRIGger:A`, `WFMOutpre`) -
**neither manual was fetched** (web search budget exhausted; tek.com URLs for those manuals not
resolved), so they are listed as low confidence.
**Interfaces:** USBTMC (Tek vid `0699`, pid per model not confirmed), USB host; TBS2000B adds LAN.
Messages end LF + EOM (p17).
**Doc:** `DOC/tektronix_tbs1000c/doc/TBS1000C-Programmer-Manual-077169102.pdf` (210 pp.). PDF page ≈
printed + 1. Good, terse.
**Identify (p100):** `*IDN?` → `TEKTRONIX,TBS1102C,C012345,CF:91.1CT FV:v1.23 TBS 1XXXC:v…`.
**Headers:** `HEADer OFF` (p87) is mandatory in `on_connect` (default ON returns `:CH1:SCALE 1.0E0`).

| Operation | Command | Reply (HEADer OFF) | Page |
|---|---|---|---|
| display | `SELect:CH{ch} ON\|OFF` (also resets the acquisition) | `1`/`0` | p139 |
| scale | `CH{ch}:SCAle <v>` (3 significant digits) | `1.0000` | p44 |
| offset | `CH{ch}:OFFSet <v>` (volts); `CH{ch}:POSition` is in **divisions** (±5) | | p40-41 |
| coupling | `CH{ch}:COUPling AC\|DC` | | p38 |
| probe | `CH{ch}:PRObe:GAIN <g>` - **gain = 1/ratio** (10X probe = 0.1) | `0.1000E+00` | p41-42 |
| bwlimit | `CH{ch}:BANdwidth TWEnty\|FULl` | `FULL` | p38 |
| invert | `CH{ch}:INVert ON\|OFF` | | p39 |
| timebase | `HORizontal:MAIn:SCAle <s>` (2 ns-100 s) | | p96 |
| timebase_offset | `HORizontal:MAIn:DELay:TIMe <s>` (+ `…:DELay:STATe ON`) | | p95 |
| trigger_sweep | `TRIGger:A:MODe AUTO\|NORMal`; single = `ACQuire:STOPAfter SEQuence` | | p152, p25 |
| trigger_status | `TRIGger:STATE?` → `ARMED\|AUTO\|READY\|SAVE\|TRIGGER` | | p160 |
| trigger_source | `TRIGger:A:EDGE:SOUrce CH1\|CH2\|LINE\|AUX` | | p148 |
| trigger_slope | `TRIGger:A:EDGE:SLOpe RISe\|FALL` | | p148 |
| trigger_level | `TRIGger:A:LEVel <v>` (or `TRIGger:A:LEVel:CH<x>`) | | p150 |
| acquire_type | `ACQuire:MODe SAMple\|PEAKdetect\|HIRes\|AVErage` | | p23 |
| sample_rate | `HORizontal:MAIn:SAMPLERate?` | `2.0000E+9` | p95-96 |
| run | `ACQuire:STATE RUN\|STOP` (+ `ACQuire:STOPAfter RUNSTop\|SEQuence`), `AUTOSet EXECute` | | p25, p30 |
| measurement | `MEASUrement:IMMed:SOUrce1 CH{ch}`, `MEASUrement:IMMed:TYPe PK2Pk\|MEAN\|RMS\|FREQuency`, `MEASUrement:IMMed:VALue?`; **9.9000E+37** = error | NR3 | p113-115 |
| waveform | `DATa:SOUrce CH{ch}`, `WFMOutpre:ENCdg BINary`, `WFMOutpre:BN_Fmt RI`, `WFMOutpre:BYT_Nr 1`, `DATa:STARt 1`, `DATa:STOP 2000`, `WFMOutpre?`, `CURVe?` | | p58-60, p170-177, p55 |
| screenshot | none direct: `SAVe:IMAge "<file>"` to the USB stick, then `FILESystem:READFile` | | p136 |

**Waveform:** `CURVe?` (p55) → `#<x><yyy><data>\n` IEEE block; `BN_Fmt RI` = signed → `i8`
(`BYT_Nr 2` → 16-bit, MSB first by default, LSB always 0 → `i16be`). A hidden source returns
**nothing** + events 2244/420 (p55) → must use `requires`. Preamble `WFMOutpre?` (p170) with HEADer
OFF: `BYT_NR;BIT_NR;ENCDG;BN_FMT;BYT_OR;WFID;NR_PT;PT_FMT;XUNIT;XINCR;XZERO;PT_OFF;YUNIT;YMULT;YOFF;YZERO`
- `fields: {sep: ";"}` works (WFID is quoted and contains commas but no `;`), or query
`WFMOutpre:YMUlt?/YOFf?/YZEro?/XINcr?/XZEro?` separately (p174-177). Conversion (standard Tek):
`volts = (code − YOFF) · YMULT + YZERO`, `time = XZERO + (i − PT_OFF) · XINCR`. Status:
`WAVE_Y0 = YOFF − YZERO/YMULT`, `WAVE_DY = YMULT`, `WAVE_X0 = XZERO − PT_OFF·XINCR`, `WAVE_DX = XINCR`.
The doc example shows `YMULT 15.6250E-6; YOFF 6.4000E+3` for 16-bit data.
**Quirks:** `*IDN?`/`ID?` must be last in a concatenated message (p100). `CH:SCAle` and measurement
changes take time; doc recommends a single sequence + `*OPC?` before `CURVe?` (p55).
`DATa:STOP` does not follow record length changes (p59).
**Confidence:** medium-high for TBS1000C, low for TBS1000B/TBS2000B (no manual).
**DSL fit:** works today (`HEADer OFF`, per-channel `before:` for `DATa:SOUrce` and
`MEASUrement:IMMed:SOUrce1`+`TYPe`). Screenshot not possible.

---

### 2.8 UNI-T MSO/UPO2000 (UPO2102CS, UPO2104CS, MSO2000CS), UPO2000E/3000E, MSO2000X/3000X, UPO1000

**Models:** UPO2072CS/2102CS/2104CS, MSO2102CS/2104CS (70-100 MHz, 1 GS/s, 8-bit); UPO2000E/3000E;
MSO2000X/3000X (newer, 12-bit?); UPO1000(CS), UPO1002. Screen trace **1400 points** (NORMal,
`:WAVeform:STARt/STOP 1…1400`, upo p80-81).
**Interfaces:** USBTMC (vid `5656`, Uni-Trend; pid not confirmed), LAN (on MSO models). Terminator `\n`.
**Docs:** `DOC/unit_upo2000/doc/MSOUPO2000_Programming_Manual_V1.0.pdf` (161 pp.),
`DOC/unit_upo2000/doc/UPO3000E_UPO2000E_Programming_Manual_V1.0.pdf`,
`DOC/unit_mso2000x/doc/MSO3000X_MSO2000X_Programming_Manual_V1.1.pdf` (422 pp.),
`DOC/unit_upo1000/doc/UPO1000_Programming_Manual_V1.0.pdf`. Fair: translated, some inconsistencies.
**Identify (upo p5):** `UNI-T Technologies,MSO2000CS,<serial>,00.00.01` (the doc's example puts a
placeholder in the serial field).
Commands (MSO/UPO2000, page = PDF): `:CHANnel{ch}:DISPlay 1|0` (p14), `:CHANnel{ch}:COUPling DC|AC|GND`,
`:CHANnel{ch}:PROBe 10X` (accepts `0.001X…1000X`), `:CHANnel{ch}:OFFSet`, `:CHANnel{ch}:SCALe` (p14-16),
`:TIMebase:SCALe`, `:TIMebase:OFFSet`, `:TRIGger:SWEep AUTO|NORMal|SINGle`,
`:TRIGger:STATus?` → `STOP/ARMED/READY/TRIGED/AUTO/SCAN/RESET/REPLAY/WAIT`, `:TRIGger:SOURce CHANnel<n>`,
`:TRIGger:EDGE:SLOPe POSitive|NEGative|ALTernation`, `:TRIGger:LEVel <v>`, `:RUN`, `:STOP`, `:AUTO`,
`:MEASure:VPP? CHANnel{ch}` → `3.120e000`, `:MEASure:VRMS? DISPlay,CHANnel{ch}`, `:MEASure:VAVerage?`,
`:MEASure:FREQuency?`; invalid values are `*` (p4 "Data Return"), `:DISPlay:DATA?` → BMP in an IEEE block.
**Waveform (upo p79-84):** `:WAVeform:SOURce CHANnel{ch}`, `:WAVeform:MODE NORMal`, `:WAVeform:FORMat BYTE`
(WORD = low byte valid), `:WAVeform:DATA?` (IEEE block), `:WAVeform:PREamble?` →
`format,type,points,count,xinc,xor,xref,yinc,yor,yref` (example `1,0,0,1,8.000e-009,-6.000e-006,0,4.000e-002,0.000e000,100`).
MSO2000X: `:WAVeform:FORMat WORD|DWORD|ASCII` only (umso p227) and the conversion is stated
explicitly (umso p231): **`volts = (code − yreference − yorigin) · yincrement`** - the DHO800 formula
(`WAVE_Y0 = yorig + yref`). Byte signedness and WORD byte order are not stated → capture.
**Quirks:** NORMal XINC = TimeScale/… the manual is self-contradictory (says "/50" but 1400 points over 14 div);
use the preamble's xinc. Real values are returned as `1.230e000` (3 decimals) - fine for `number`.
**Confidence:** medium. **DSL fit:** works today (Rigol clone); samples type to confirm.

### 2.9 GW Instek GDS-1000B (GDS-1054B, GDS-1072B, GDS-1074B, GDS-1102B, GDS-1104B)

**Models:** 2/4 ch, 50/70/100 MHz, 1 GS/s, 10 Mpts, 8-bit.
**Interface:** USB device port is **USB CDC-ACM (virtual COM)** (p16 "USB CDC_ACM compatible") - a
serial link in the DSL (`link.serial`, baud irrelevant). GW Instek vid `2184` (pid not confirmed). LAN
on some variants. Terminator LF (p17).
**Doc:** `DOC/gwinstek_gds1000b/doc/GDS-1000B_Programming_Manual.pdf` (169 pp.). Pages below printed
(from the manual's command list). Good.
**Identify (p29):** `GW,GDS-1074B,PXXXXXX,V1.XX`.
Commands: `:CHANnel{ch}:DISPlay ON|OFF` (p44), `:CHANnel{ch}:SCALe` (p47), `:CHANnel{ch}:POSition` (volts,
p46), `:CHANnel{ch}:COUPling AC|DC|GND` (p44), `:CHANnel{ch}:PROBe:RATio` (p47), `:CHANnel{ch}:BWLimit` (p43),
`:CHANnel{ch}:INVert ON|OFF` (p46), `:TIMebase:SCALe` / `:TIMebase:POSition` (p114), `:TRIGger:MODe AUTo|NORMal`
(p120), `:TRIGger:SOURce CH1` (p119), `:TRIGger:LEVel` (p120), `:TRIGger:EDGe:SLOP RISe|FALL|EITher` (p122),
`:TRIGger:STATe?` (p131, `*ARMED/*AUTO/*READY/…`), `:RUN`/`:STOP`/`:SINGle` (p112), `:AUTOSet` (p42),
`:MEASure:SOURce1 CH{ch}` + `:MEASure:PK2Pk?|MEAN?|RMS?|FREQuency?` (p81-93), `:ACQuire:MODe SAMPle|PDETect|AVERage` (p36).
**Waveform (p36-38):** `:HEADer OFF` (p41, default ON) then `:ACQuire{ch}:MEMory?` → `#<n><len>` block of
**signed 16-bit big-endian** samples (`i16be`); with HEADer ON the block is preceded by a `;`-separated
text settings string (`Vertical Scale,5.000e+00;…;Waveform Data;#520000…`). Conversion (p37-38):
`volts = code / 25 · vertical_scale` (AD factor fixed 25; example 0x001C = 28 → 28/25·0.5 = 0.56 V); position
handling not stated (probably relative to screen centre → subtract `:CHANnel:POSition`; capture).
`:ACQuire{ch}:STATe?` = 1 when raw data is ready (p40). **Returns the full record length** (≥ 1000 points;
`:ACQuire:RECOrdlength 1e+3` sets the minimum, p40) - 1000 points only if the user accepts the
record length change; 10 Mpts at default would take minutes over CDC.
**Screenshot:** `:DISPlay:OUTPut?` → `#5<len>` block of 16-bit RGB **run-length encoded** data (p72) -
needs a decoder.
**Confidence:** medium. **DSL fit:** works today over serial (block reply, `i16be`); record length is the
practical problem.

### 2.10 OWON XDS2000/XDS3000/TAO3000, SDS(-E)/SDS1000, HDS200 handheld

**Docs:** `DOC/owon_xds/doc/XDS3000_Four-Channel_Series_Oscilloscopes_SCPI_Protocol.pdf` (53 pp.),
`DOC/owon_xds/doc/XDS3000_and_XDS2000_Dual-Channel_Series_Oscilloscopes_SCPI_Protocol.pdf`,
`DOC/owon_sds/doc/SDS_Series_Oscilloscopes_SCPI_Protocol.pdf`, `DOC/owon_sds/doc/SDS1000_Oscilloscopes_SCPI_Protocol.pdf`,
`DOC/owon_hds/doc/HDS200_Series_SCPI_Protocol.pdf`. Poor: no transport details, no conversion formula.
**Identify:** `OWON,<model>,<serial>,X.XX.XX` (SDS doc). HDS200: `<Manufacturer>,<model>,…`.
**Interfaces:** "USB port or LAN" (XDS p3). OWON scopes enumerate as `5345:1234` (usb.ids "Owon
PDS6062T") - a **vendor-specific bulk** interface, not USBTMC (community; not in the doc); LAN raw
socket (port set by `:LAN:PORT`, default 3000 per community). HDS200 handhelds use USB CDC serial
(community, unconfirmed).
**Commands (XDS 4ch):** `:CH{ch}:DISPlay ON|OFF`, `:CH{ch}:COUPling AC|DC|GND`, `:CH{ch}:PROBe 10X`,
`:CH{ch}:SCALe 1v` (**replies are strings with units**: `"1v"`, `"200us"`), `:CH{ch}:OFFSet <div·?>`
(integer −2000…2000, unit unclear), `:HORIzontal:SCALe 200us`, `:HORIzontal:OFFset <div>`,
`:TRIGger:STATus?` → `AUTO|READy|TRIG|SCAN|STOP`, `:TRIGger:SINGle:EDGE:SOURce|SLOPe|LEVel`,
`:TRIGger:SINGle:SWEEp`, `:RUNning RUN|STOP`, `:AUTOset ON`, `:MEASUrement:CH{ch}:<item>?`.
SDS doc: `:MEASure:SOURce`, `:MEASure:PKPK?`, `:MEASure:FREQuency?`, `:TIMebase:SCALe`, `:CHANnel{n}:…`
(different tree from XDS!), **no waveform commands**.
**Waveform (XDS p34-36, HDS p11):** `:DATA:WAVE:SCREen:HEAD?` → 4-byte length + **JSON** (timebase,
sample rate, per-channel scale/offset/probe strings); `:DATA:WAVE:SCREen:CH{ch}?` → 4-byte little-endian
length + 16-bit LE samples (12-bit on XDS, 8-bit-in-16 on HDS), ~1520 points;
`:DATA:WAVE:SCREen:BMP?` (XDS) screenshot. Volts conversion is not documented (offset in JSON is
an integer in screen units).
**Confidence:** low. **DSL fit:** blocked - needs a length-prefixed binary reply (`reply: {prefix:
u32le}`), a JSON parse step, an SI-suffix parse step (`"200us"` → 2e-4), and the vendor-bulk USB
transport or TCP.

### 2.11 Hantek DSO2000 (DSO2C10, DSO2C15, DSO2D10, DSO2D15)

**Doc:** `DOC/hantek_dso2000/doc/DSO2000_SCPI.pdf` (61 pp., "SCPI Programmers Manual"). Poor.
**Interface:** USB device; community reports the generic Linux gadget id `049f:505a` and USBTMC (not in
the doc). No LAN.
**Commands:** Rigol-like: `:CHANnel{ch}:DISPlay 1|0`, `:SCALe`, `:OFFSet`, `:COUPling AC|DC|GND`, `:PROBe 1|10|100|1000`,
`:BWLimit`, `:INVert` (p5-8), `:TIMebase:SCALe/POSition` (p9-10), `:ACQuire:TYPE/SRATe?/POINts` (p11-12),
`:TRIGger:SWEep AUTO|NORMal|SINGle`, `:TRIGger:STATus?` → `TRIGed|NOTRIG`, `:TRIGger:EDGe:SOURce/SLOPe/LEVel`
(p13-15). **No `:RUN`/`:STOP`, no `:MEASure` items, no `*IDN?` section, no screenshot.**
**Waveform (p42):** `:WAVeform:DATA:ALL?` → first call returns a 128-byte ASCII header
`#9<9 len><9 total><9 uploaded><run><trig><4×4 offsets><4×7 volts/div><4 enables><9 srate><6 multiple>…`;
repeated calls return `#9<len><total><uploaded>` + interleaved data until `uploaded == total`.
**Confidence:** low. **DSL fit:** blocked - multi-packet stateful read with a custom header (looks like
IEEE but the 9-digit length is followed by more ASCII fields). Settings/trigger subset would work today.

### 2.13 Rigol MHO900 (MHO934, MHO954, MHO984)

**Doc:** MHO900 Programming Guide, 2026 (518 PDF pages; pages here are printed). **Definition:**
`rigol_mho900`, analog scope only; details and open questions in `device-notes/rigol_mho900.md`.
**Models:** 350 / 500 / 800 MHz, 4 ch, 4 GSa/s, 12-bit, 100 Mpts (500 Mpts option), 16 digital channels
with the optional PLA2216 probe. **Interfaces:** USBTMC vid `1ab1` (**pid not found**), LAN raw socket 5555
(not in the guide, reached by users). **Identify:** `RIGOL TECHNOLOGIES,MHO954,...` seen on 00.01.00,
`NK TECHNOLOGIES,MHO954,...` reported on 00.01.03, so the regex names no vendor.
**Same as the base `scpi_osc` except:** `:AUToset` (p12), edge sources CH1-4 and D0-D15 without AC line (p335),
`:CHANnel{ch}:IMPedance OMEG|FIFTy` (p86), BW 20M/250M (p82), HRESolution (p19), WORD traces, read window reset on connect.
**Confidence:** high for the analog scope (one identify seen live, no waveform seen by us). **DSL fit:** works today.
**Not modelled:** `:WAVeform:SOURce` lists no digital channel (p468), unlike the MSO5000 guide.

### 2.12 Not supported / no remote interface

- **Hantek 6022BE/BL** (`04b5:6022`/`04b5:602a`, Cypress FX2): no SCPI; host uploads firmware (sigrok
  fx2lafw / hantek-6xxx), raw sample streaming. Out of scope for the DSL.
- **Micsig** tablet scopes (TO1104, STO, ATO): remote control exists on some models (USB/LAN, app/SCPI
  per marketing) but no programming manual was obtained in this pass. Unknown.
- **FNIRSI** (1013D, 1014D, DPOX180H, 2C23T, DSO152): no documented remote control; USB is mass
  storage/charging only.
- **Siglent SDS1000CML+/DL+** (legacy, very common used): legacy command set like SDS1000X-E but
  `WF?` with the 346-byte WAVEDESC binary prefix (`C1:WF? ALL`); USBTMC `f4ec:ee3a`. Same blocker as 2.4.

---

## 3. Shared SCPI baseline (`scpi_osc` base) and dialects

The Keysight InfiniiVision tree is the ancestor; Rigol (DS1000Z/MSO5000/DS2000A/DHO), UNI-T and
Hantek copied it. A base definition can carry everything below; families override a handful of
commands.

| Op | Base (`scpi_osc`) | Rigol DHO | Rigol DS1000Z/MSO5000 | Keysight | Siglent new | UNI-T | Tek TBS1000C |
|---|---|---|---|---|---|---|---|
| display | `:CHANnel{ch}:DISPlay 1\|0` | = | = | = | `:CHANnel{ch}:SWITch ON\|OFF` | = | `SELect:CH{ch}` |
| scale/offset | `:CHANnel{ch}:SCALe/OFFSet` | = | = | = | = | = | `CH{ch}:SCAle/OFFSet` |
| coupling | `:CHANnel{ch}:COUPling DC\|AC\|GND` | = | = | no GND | = | = | no GND |
| probe | `:CHANnel{ch}:PROBe <r>` | = | = | = | `PROBe VALue,<r>` | `10X` | `PRObe:GAIN 1/r` |
| bwlimit | `:CHANnel{ch}:BWLimit 20M\|OFF` | = | = | `1\|0` | `FULL\|20M\|200M` | ? | `BANdwidth TWEnty\|FULl` |
| timebase | `:TIMebase:SCALe` | `:TIMebase:MAIN:SCALe` | `:TIMebase:MAIN:SCALe` | = | = | = | `HORizontal:MAIn:SCAle` |
| tb offset | `:TIMebase:POSition` | `:TIMebase:MAIN:OFFSet` | `MAIN:OFFSet` | = | `:TIMebase:DELay` | `:TIMebase:OFFSet` | `HORizontal:MAIn:DELay:TIMe` |
| sweep | `:TRIGger:SWEep AUTO\|NORMal\|SINGle` | = | = | no SINGle | `:TRIGger:MODE` | = | `TRIGger:A:MODe` |
| trig status | `:TRIGger:STATus?` | TD/WAIT/RUN/AUTO/STOP | = | `:OPERegister:CONDition?` | `Arm/Ready/Auto/Trig'd/Stop/Roll` | STOP/ARMED/…/TRIGED | `TRIGger:STATE?` |
| edge trigger | `:TRIGger:EDGE:SOURce/SLOPe/LEVel` | = | = (DS1000Z line source `AC`, others `ACLine`) | `:TRIGger[:EDGE]:…` | = (`C1`, `RISing`) | `:TRIGger:SOURce`, `:TRIGger:LEVel` | `TRIGger:A:EDGE:…` |
| run | `:RUN :STOP :SINGle :AUToscale` | `:AUToset` (doc) | = | = | `:TRIGger:RUN/STOP`, `:AUToset` | `:AUTO` | `ACQuire:STATE`, `AUTOSet EXECute` |
| measure | `:MEASure:ITEM? VPP,CHANnel{ch}` | = | = (DS2000A: `:MEASure:VPP? CHANnel{ch}`) | `:MEASure:VPP? CHANnel{ch}` | `:MEASure:SIMPle:VALue? PKPK` (+source) | `:MEASure:VPP? CHANnel{ch}` | `MEASUrement:IMMed:*` |
| no-value | `9.9E37` | = | = | = | `****` | `*` | `9.9E37` |
| wave | `:WAVeform:SOURce/MODE/FORMat/PREamble?/DATA?` | = | = | `POINts:MODE` | `:WAVeform:WIDTh`, binary preamble | = | `DATa:SOUrce`, `CURVe?`, `WFMOutpre?` |
| samples | `u8`, `volts = (code − yorig − yref)·yinc` | = | = | `(code − yref)·yinc + yorig` | `i8`/`i16le`, `code·vdiv/cpd − voff` | = Rigol | `i8`, `(code − YOFF)·YMULT + YZERO` |
| screenshot | `:DISPlay:DATA? PNG` | = (stalls over kernel usbtmc) | DS1000Z `ON,OFF,PNG`; MSO5000/DS2000A no arg, BMP | `:DISPlay:DATA? PNG,COLor` | `:PRINt? PNG` | BMP | none |

Legacy Siglent (`C1:VDIV`, `TDIV`, `C1:WF? DAT2`) and OWON/Hantek/GW Instek are separate dialects
and should not extend the base.

**Base candidates (`extends: scpi_osc`):** Rigol DS1000Z, MSO5000, DS2000A, DHO800/900/1000/4000,
UNI-T MSO/UPO2000, Keysight 1000X/2000X/3000T (overrides: timebase, trigger status, measure).

---

## 4. DSL features that blocked families

| Feature | Blocks | Size |
|---|---|---|
| `reply: block` tolerant of a text prefix (`skip_to: "#"`) | Siglent SDS1000X-E/SDS1000X/SDS2000X-E waveforms (most common Siglent) | small (framer) |
| Binary struct parse step (`{struct: {endian: le, fields: {vdiv: {at: 156, type: f32}, …}}}`) | exact Siglent new-dialect and legacy WAVEDESC scaling (workaround: text queries + per-model `code_per_div`) | small |
| Length-prefixed binary reply (`reply: {prefix: u32le}`) | OWON XDS/TAO/HDS waveforms | small |
| JSON parse step, SI-suffix number parse (`"200us"`, `"1v"`) | OWON (all settings are strings with units) | small |
| TCP transport (`link.tcp`, raw socket 5025 / 5555 / 3000) | LAN use of every family (Siglent, Keysight, Rigol, UNI-T, OWON); no family here is LAN-only, but OWON may be effectively LAN-only if its USB is vendor-bulk | medium |
| Vendor-specific USB bulk transport (non-USBTMC) | OWON scopes (`5345:1234`) | medium |
| Multi-read accumulation (repeat a query until a header says done) | Hantek DSO2000 `:WAVeform:DATA:ALL?`, chunked RAW reads on all families | medium |
| Raw reply until idle / known length (no header) | Siglent legacy `SCDP` BMP screenshot | small |
| Userspace USBTMC multi-transfer reads (known issue from DHO800) | all PNG screenshots over Linux kernel usbtmc | in progress |
| RLE image decoder | GW Instek `:DISPlay:OUTPut?` screenshot | small, optional |

---

## 5. Notes for authors

- **DHO800 definition vs its manual** (found while cross-reading, worth a bench check): the guide
  documents `:AUToset` (printed p15), not `:AUToscale` (works only via the shared `:AUT` short form);
  the DHO800 edge-source list is `D0..D15|CHANnel1-4|EXT` without `ACLine` (the DHO1000/4000 guide has
  it); `9.9E37` is not documented in the DHO guides (observed on the bench).

- Every scope here replies to setters with nothing: `reply: none` everywhere.
- Always guard waveform/measurement reads with `requires: <display> == 'ON'` (Rigol/Keysight/Siglent
  error, Tek returns nothing).
- Poll groups as in DHO800: 250 ms trace (status, preamble, data), 1 s measurements, 3 s settings.
- Keep screen-size transfers (≈ 1000-1400 points); full-memory reads need chunked reads (not modelled).
- The class contract's `WAVE_Y0/DY/X0/DX` linear form covers all families above:
  Rigol/UNI-T `Y0 = yorig + yref`; Keysight `Y0 = yref − yorig/yinc`; Tek `Y0 = YOFF − YZERO/YMULT`;
  Siglent `Y0 = offset·cpd/scale`, `DY = scale/cpd`; GW Instek `DY = scale/25`.
