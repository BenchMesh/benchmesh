# RIGOL MHO900 series oscilloscope - device notes

Definition: `devices/rigol_mho900.yaml` (class `OSC`, base `scpi_osc`). Source: *MHO900 Programming
Guide*, 2026; pages are printed pages. No MHO900 is on the maintainers' bench: every model is
`awaiting`. This note says what is known, what only a real instrument can settle, and what a
definition change can and cannot fix.

## What has been seen on real instruments

| What | Source | Evidence |
|---|---|---|
| `RIGOL TECHNOLOGIES,MHO954,<serial>,00.01.00` over USBTMC (`/dev/usbtmc0`, kernel driver) | Detect in the official build, a photo of its result, 2026-11 | one identify exchange |
| `NK TECHNOLOGIES,MHO954,<serial>,00.01.03` over LAN; `:DISPlay:DATA?` gives a 1024x600 PNG of about 66 KB; port 5555 open | a user's report, [Classic-Repair-Toolbox #114](https://github.com/HovKlan-DH/Classic-Repair-Toolbox/issues/114) | that user's test run |
| `:CHANnel<n>:IMPedance?` OMEG / FIFT..., `:WAVeform:FORMat WORD` low byte first, volts = (code - yorigin - yreference) * yincrement, 9.9E37 = no value, chunks of 4 M points (8 MB) | [rigol-scope-kit](https://github.com/felixfinal/rigol-scope-kit), bench-tested on an MHO954 over LAN | its documents and code, not checked here |

Nothing here has run this definition on a real MHO900. What ran: the guide's replies through the real
engine (`devices/examples/rigol_mho900*.yaml`) and the real server, `bm-probe` and panel against
`scripts/mho900_fake.py`, a fake that refuses every command the guide does not document. That proves the
definition follows the guide, not that an instrument agrees with the guide.

## What the first real report should settle

Check these in its wire log and reads. All but the last are changes of the definition.

1. **WORD scaling.** The guide shows a preamble only for BYTE (`...,4.000000E-03,0,128`). In the wire log the
   preamble's format field should be 1. CH Vpp computed from `WAVE` should agree with the `VPP` reading;
   if it does not, the trace is off scale or flat. If WORD cannot be made right, `:WAVeform:FORMat BYTE`
   with `samples: u8` is the guide's documented path (8 of the 12 bits).
2. **USB under polling.** The DHO804 locked up over USB (`device-notes/rigol_dho800.md`). Poll times, timeouts
   and USBTMC recovery notes appear in the report's `history`. Slower poll groups are a definition change.
3. **USB ids.** `sweep.usb` of a report, or `lsusb`. Add `pid` to `link.usbtmc`, and correct `vid` if it differs.
4. **Replies the maps do not know.** `imp_q` maps OMEG, FIFT, FIFTy; any other spelling is a parse failure.
5. **BW limit, HRES, probe.** Replies 20M / 250M / OFF, HRES, scientific notation per the guide.
6. **Scale at 1 Mohm.** `features.scale.min` is 200 uV for the 50 ohm input; at 1 Mohm the scope should clamp to
   1 mV, leaving the control's request unconfirmed for that one step. Accepted for now.
7. **Screenshot.** Not bound: a PNG transfer stalled and rebooted a DHO804 over USB. It has only been seen on
   LAN. Binding it reaches USB users too, and the `screenshot` operation has no per-link switch, so a real USB test
   comes first.
8. **Not a definition change:** anything in the panel, the class contract or the engine.

## Left out on purpose

- **Logic analyzer (`:LA`, p147-152).** The controls are documented (on/off, POD1/POD2, per-line on/off,
  thresholds, labels), and so are D0-D15 as trigger sources (p335) and measurement sources (p211). They are left out
  because those sources need the PLA2216 probe, and a command the scope rejects shows an error on its
  screen. `:SYSTem:MODules?` (p291, first digit: LA) may report it; unchecked on a real scope.
- **Reading logic traces: unknown.** `:WAVeform:SOURce` (p468) lists CH1-4 and MATH1-4, no digital channel, while the
  MSO5000 guide lists D0-D15. To try with a probe, in this order:
  1. `:WAV:SOUR D0` then `:SYSTem:ERRor?` (undocumented here, best result if accepted).
  2. `:BUS1:PARallel:BUS D0D15` with `:BUS1:DATA?` (p24, p29): a table of time and value; whether it logs every change
     without a clock is unconfirmed. The same table gives UART, I2C, SPI, CAN and LIN decode.
  3. Logic math on one line, `:MATH1:LSOurce1 D3` (p177), read as `:WAV:SOUR MATH1`: 4 lines at most, 1000 points,
     uses the math slots.
  A saved `.bin` holds digital data (User Guide 23.2.4), but the guide shows no way to read a file back over SCPI.
- **Class design for it:** optional fields in `OSC`, not a new class, because analog and digital share the screen,
  timebase and trigger. That needs contract and panel work, so not the fixer's.
- **Deep records.** `:WAVeform:MODE RAW` only while stopped; needs a chunked read the definition language lacks (`features/OSC.md` 5.4).
- **Generator option (AFG50/AFG100).** Belongs in the `AWG` class as a second class of the definition. A class cannot yet depend
  on a licence (`:SYSTem:OPTion:STATus?`, p293), and a scope without the option would show a dead panel.
- **MHO98.** Has its own programming guide (not read), so its models are not in this definition.
