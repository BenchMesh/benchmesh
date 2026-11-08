/**
 * Class profiles: what one class panel needs to know about a particular instrument, derived
 * from the API alone - features (`GET /instruments/{CLS}/{id}`) and the setters' value specs
 * (`GET /instruments/{CLS}/{id}/methods`). Every PSU uses the same panel; only these limits,
 * steps and optional controls differ between manufacturers.
 */
import { decimalsFromFormat, intDigitsFor } from '../kit/format'
import type { DigitSpec } from '../kit/digitModel'
import { WAVE_SHAPES } from '../kit/WavePreview'
import { siText } from '../kit/scope'
import { symbolText, type SymbolPart } from '../kit/meterSymbol'

export interface ValueSpec { type: string; min?: number; max?: number; format?: string; unit?: string; values?: string[] }
export interface Setter { value?: ValueSpec; energizes?: boolean }
export type Setters = Record<string, Setter>

export interface NumberSpec extends DigitSpec { unit: string }

export function setterMap(methods: { name: string; http_method: string; value?: ValueSpec; energizes?: boolean }[]): Setters {
  const out: Setters = {}
  for (const m of methods ?? []) if (m.http_method === 'POST') out[m.name] = { value: m.value, energizes: m.energizes }
  return out
}

const isNum = (x: unknown): x is number => typeof x === 'number' && Number.isFinite(x)

function decimalsOfStep(step: number): number {
  const s = String(step)
  return s.includes('e-') ? Number(s.split('e-')[1]) : (s.split('.')[1] ?? '').length
}

/**
 * Digit editor spec for a numeric setter: the tightest of the definition's and the model's
 * limits; decimals from the model's step, else the wire format, else what fits 6 digits.
 */
export function numberSpec(setter: Setter | undefined, model: { min?: number; max?: number; step?: number; unit?: string } = {}): NumberSpec | null {
  const v = setter?.value
  if (!v || (v.type !== 'number' && v.type !== 'integer')) return null
  const max = [v.max, model.max].filter(isNum).reduce((a, b) => Math.min(a, b), Infinity)
  const top = Number.isFinite(max) ? max : 1000
  const intDigits = intDigitsFor(Math.abs(top))
  const decimals = v.type === 'integer' ? 0
    : isNum(model.step) ? decimalsOfStep(model.step)
    : decimalsFromFormat(v.format, Math.min(3, Math.max(0, 6 - intDigits)))
  const resolution = 10 ** -decimals
  const lo = [v.min, model.min].filter(isNum).reduce((a, b) => Math.max(a, b), -Infinity)
  // the smallest positive value the editor can show, so `min: 1e-6` with 0 decimals becomes 1
  const min = !Number.isFinite(lo) ? 0 : lo > 0 ? Math.max(lo, resolution) : lo
  return { min, max: top, intDigits, decimals, unit: model.unit ?? v.unit ?? '' }
}

const enumSetter = (s: Setters, op: string) => s[op]?.value?.type === 'enum'

// ---- PSU -------------------------------------------------------------------------------

export function psuProfile(features: any, setters: Setters) {
  const cv = features?.modes?.CV
  const cc = features?.modes?.CC
  const abs = features?.absolute_limits ?? {}
  return {
    voltage: numberSpec(setters.voltage, { max: cv?.setpoint?.max ?? abs.voltage?.max, step: cv?.setpoint?.step, unit: 'V' }),
    current: numberSpec(setters.current, { max: cc?.setpoint?.max ?? abs.current?.max, step: cc?.setpoint?.step, unit: 'A' }),
    voltageLimit: numberSpec(setters.voltage_limit, { ...cv?.protection?.OVP, unit: 'V' }),
    currentLimit: numberSpec(setters.current_limit, { ...cc?.protection?.OCP, unit: 'A' }),
    output: !!setters.output,
    protectionSwitches: ['ovp', 'ocp'].filter((op) => enumSetter(setters, op)),
    remote: enumSetter(setters, 'remote'),
    maxPower: isNum(abs.power?.max) ? abs.power.max : null,
  }
}
export type PsuProfile = ReturnType<typeof psuProfile>

// ---- ELL -------------------------------------------------------------------------------

const ELL_ORDER = ['CURR', 'VOLT', 'RES', 'POW']
const ELL_SHORT: Record<string, string> = { CURR: 'CC', VOLT: 'CV', RES: 'CR', POW: 'CP' }

export interface EllMode { key: string; op: string | null; short: string; name: string; spec: NumberSpec | null }

export function ellProfile(features: any, setters: Setters) {
  const settable = setters.mode?.value?.type === 'enum' ? setters.mode.value.values ?? [] : null
  const modes: EllMode[] = Object.entries<any>(features?.modes ?? {})
    .map(([key, m]) => {
      const op = key.toLowerCase()
      const spec = m?.setpoint && setters[op] ? numberSpec(setters[op], m.setpoint) : null
      return { key, op: spec ? op : null, short: ELL_SHORT[key] ?? key, name: m?.ui_name ?? key, spec }
    })
    // a mode without a setpoint here (dynamic, LED, ...) only when the mode setter offers it;
    // a short-circuit *mode* never: the Short switch (two presses) is the way to short the input
    .filter((m) => (m.spec || settable?.includes(m.key)) && m.key !== 'SHORT')
    .sort((a, b) => (ELL_ORDER.indexOf(a.key) + 99) % 99 - (ELL_ORDER.indexOf(b.key) + 99) % 99)
  return {
    modes,
    input: !!setters.input,
    short: enumSetter(setters, 'short'),
    remote: enumSetter(setters, 'remote'),
    sense: enumSetter(setters, 'remote_compensation'),
    remoteRequired: features?.compulsory_lock === true,
  }
}
export type EllProfile = ReturnType<typeof ellProfile>

// ---- DMM -------------------------------------------------------------------------------

/** Digits a reading shows: `features.counts` as digits (6) or as a count (55000 -> 5); else 5. */
export function displayDigits(counts: unknown): number {
  return isNum(counts) && counts > 0 ? (counts > 9 ? Math.ceil(Math.log10(counts + 1)) : counts) : 5
}

/** Dial symbol for a meter function: letters and vector marks (`kit/meterSymbol`). */
export function modeSymbol(key: string, m: { unit?: string; note?: string }): SymbolPart[] {
  const k = key.toUpperCase()
  if (k.startsWith('CONT')) return [{ mark: 'beep' }]
  if (k.startsWith('DIOD')) return [{ mark: 'diode' }]
  if (k.startsWith('TEMP')) return [{ mark: 'temp' }]
  if (k.startsWith('PER')) return [{ text: 'PER' }]
  const current: SymbolPart = { mark: m.note === 'AC' ? 'ac' : 'dc' }
  switch (m.unit) {
    case 'V': return [{ text: 'V' }, current]
    case 'A': return [{ text: 'A' }, current]
    case 'Ω': return m.note === '4W' ? [{ text: 'Ω' }, { text: '4W', small: true }] : [{ text: 'Ω' }]
    case 'F': return [{ mark: 'cap' }]
    case 'Hz': return [{ text: 'Hz' }]
  }
  return [{ text: k.slice(0, 4) }]
}

// Physical dial order: voltage, current, resistance family, then the rest.
const DMM_ORDER = ['VOLT', 'CURR', 'RES', 'FRES', 'CONT', 'DIOD', 'CAP', 'FREQ', 'PER', 'TEMP']
const dmmRank = (key: string) => {
  const k = key.toUpperCase()
  const i = DMM_ORDER.findIndex((p) => k.startsWith(p))
  return (i < 0 ? 99 : i) * 10 + (k.includes('AC') ? 1 : 0)
}

export interface Option { value: string; display: string }

export function dmmProfile(features: any, setters: Setters) {
  const modes = Object.entries<any>(features?.modes ?? {})
    .map(([key, m]) => ({ key, symbol: modeSymbol(key, m ?? {}), glyph: symbolText(modeSymbol(key, m ?? {})), name: [m?.ui_name ?? key, m?.note === '4W' ? '4-wire' : ''].filter(Boolean).join(' '), unit: m?.unit ?? '' }))
    .sort((a, b) => dmmRank(a.key) - dmmRank(b.key))
  const R = features?.RANGe ?? {}
  const ranges: Record<string, Option[]> = {}
  for (const [k, list] of Object.entries<any>(R)) {
    if (Array.isArray(list)) ranges[k] = list.filter((r: Option) => r.value !== 'MIN' && r.value !== 'MAX')
  }
  const temp = R.TEMP && !Array.isArray(R.TEMP) ? R.TEMP : {}
  return {
    modes,
    ranges,
    canRange: !!setters.range,
    rates: setters.rate && Array.isArray(features?.rate) ? (features.rate as Option[]) : [],
    tempSensors: setters.temp_sensor && Array.isArray(temp.Sensor) ? (temp.Sensor as Option[]) : [],
    tempScales: setters.temp_scale && Array.isArray(temp.Scale)
      ? (temp.Scale as { symbol: string; unit: string; display: string }[]).map((s) => ({ value: s.symbol, display: s.unit }))
      : [],
    counts: displayDigits(features?.counts),
  }
}
export type DmmProfile = ReturnType<typeof dmmProfile>

// ---- AWG -------------------------------------------------------------------------------

export const AWG_MODES = ['CW', 'MOD', 'SWEEP', 'BURST']

/** A numeric setter's raw limits (time setters span ns to s: the editor picks a unit). */
function limits(setter: Setter | undefined): { min: number; max: number } | null {
  const v = setter?.value
  return v && (v.type === 'number' || v.type === 'integer') && isNum(v.min) && isNum(v.max) ? { min: v.min, max: v.max } : null
}

export function awgProfile(features: any, setters: Setters) {
  const shapes = setters.shape?.value?.values ?? []
  const f = setters.frequency?.value
  const amplitude = numberSpec(setters.amplitude)
  const offset = numberSpec(setters.offset)
  return {
    shapes: [...shapes].sort((a, b) => (WAVE_SHAPES.indexOf(a) + 99) % 99 - (WAVE_SHAPES.indexOf(b) + 99) % 99),
    frequency: numberSpec(setters.frequency),
    /** The setter's own limits (the editor's are rounded to its digits): the period range. */
    frequencyLimits: f && isNum(f.min) && isNum(f.max) && f.min > 0 ? { min: f.min, max: f.max } : null,
    amplitude,
    offset,
    /** High / low level entry: both levels within the offset range widened by half the amplitude. */
    levelLimit: amplitude && offset ? Math.max(Math.abs(offset.min), Math.abs(offset.max), amplitude.max / 2) : null,
    phase: numberSpec(setters.phase, { unit: '°' }),
    symmetry: numberSpec(setters.symmetry, { unit: '%' }),
    duty: numberSpec(setters.duty, { unit: '%' }),
    pulseWidth: limits(setters.pulse_width),
    pulseRise: limits(setters.pulse_rise),
    pulseFall: limits(setters.pulse_fall),
    loads: ordered(setters, 'load', ['50', 'HIGHZ']),
    /** Modes the definition can switch (CW / MOD / SWEEP / BURST); empty: continuous output only. */
    modes: ordered(setters, 'mode', AWG_MODES),
    modTypes: ordered(setters, 'mod_type', ['AM', 'FM', 'PM', 'FSK', 'ASK', 'PSK', 'PWM']),
    modShapes: ordered(setters, 'mod_shape', WAVE_SHAPES),
    modFrequency: numberSpec(setters.mod_frequency, { unit: 'Hz' }),
    amDepth: numberSpec(setters.am_depth, { unit: '%' }),
    fmDeviation: numberSpec(setters.fm_deviation, { unit: 'Hz' }),
    pmDeviation: numberSpec(setters.pm_deviation, { unit: '°' }),
    sweepStart: numberSpec(setters.sweep_start, { unit: 'Hz' }),
    sweepStop: numberSpec(setters.sweep_stop, { unit: 'Hz' }),
    sweepTime: limits(setters.sweep_time),
    sweepSpacings: ordered(setters, 'sweep_spacing', ['LIN', 'LOG']),
    sweepTriggers: ordered(setters, 'sweep_trigger', ['INT', 'MAN', 'EXT']),
    burstModes: ordered(setters, 'burst_mode', ['NCYC', 'INF', 'GATED']),
    burstCycles: numberSpec(setters.burst_cycles),
    burstPeriod: limits(setters.burst_period),
    burstTriggers: ordered(setters, 'burst_trigger', ['INT', 'MAN', 'EXT']),
    trigger: !!setters.trigger,
    output: !!setters.output,
    channelColors: (features?.channel_colors ?? {}) as Record<string, string>,
  }
}
export type AwgProfile = ReturnType<typeof awgProfile>

// ---- OSC -------------------------------------------------------------------------------

/** Enum values of a setter in front-panel order (`order` first, anything else after). */
function ordered(setters: Setters, op: string, order: string[]): string[] {
  const values = setters[op]?.value?.values ?? []
  const rank = (v: string) => { const i = order.indexOf(v); return i < 0 ? order.length : i }
  return [...values].sort((a, b) => rank(a) - rank(b) || a.localeCompare(b, undefined, { numeric: true }))
}

export function oscProfile(features: any, setters: Setters, channels: number) {
  const range = (r: any, min: number, max: number) => ({ min: isNum(r?.min) ? r.min : min, max: isNum(r?.max) ? r.max : max })
  const chans = Array.from({ length: channels }, (_, i) => `CHAN${i + 1}`)
  const sources = ordered(setters, 'trigger_source', [...chans, 'EXT', 'AC'])
    .filter((s) => !/^CHAN\d+$/.test(s) || chans.includes(s))
  return {
    hdivs: isNum(features?.divisions?.horizontal) ? features.divisions.horizontal : 10,
    vdivs: isNum(features?.divisions?.vertical) ? features.divisions.vertical : 8,
    scale: range(features?.scale, 1e-3, 10),
    timebase: range(features?.timebase, 1e-9, 100),
    couplings: ordered(setters, 'coupling', ['DC', 'AC', 'GND']),
    impedances: ordered(setters, 'impedance', ['1M', '50']),
    /** What the 50 ohm input takes, shown before switching to it. */
    impedance50Limit: typeof features?.impedance_50_limit === 'string' ? (features.impedance_50_limit as string) : null,
    probes: [...(setters.probe?.value?.values ?? [])].sort((a, b) => Number(a) - Number(b)),
    triggerSources: sources,
    slopes: ordered(setters, 'trigger_slope', ['POS', 'NEG', 'RFAL']),
    sweeps: ordered(setters, 'trigger_sweep', ['AUTO', 'NORM', 'SING']),
    acqTypes: ordered(setters, 'acquire_type', ['NORM', 'PEAK', 'AVER']),
    runActions: ordered(setters, 'run', ['RUN', 'STOP', 'SINGLE', 'AUTO', 'CLEAR']),
    measurements: Array.isArray(features?.measurements) ? (features.measurements as string[]) : [],
    display: enumSetter(setters, 'display'),
    bwlimit: enumSetter(setters, 'bwlimit'),
    /** Bandwidth limits the setter offers besides OFF, lowest first (20M, 100M, 200M, ...). */
    bwlimits: ordered(setters, 'bwlimit', []).filter((v) => v !== 'OFF')
      .sort((a, b) => (parseFloat(a) || 0) * (/G/i.test(a) ? 1e3 : 1) - (parseFloat(b) || 0) * (/G/i.test(b) ? 1e3 : 1)),
    invert: enumSetter(setters, 'invert'),
    offset: !!setters.offset,
    timebaseOffset: !!setters.timebase_offset,
    triggerLevel: !!setters.trigger_level,
    channelColors: (features?.channel_colors ?? {}) as Record<string, string>,
  }
}
export type OscProfile = ReturnType<typeof oscProfile>

// ---- LCR -------------------------------------------------------------------------------

/** One parameter pair of an LCR meter (a `features.functions` key, canonical E4980 token). */
export interface LcrFunction {
  key: string
  primary: string
  secondary: string | null
  circuit: 'SER' | 'PAR' | null
  /** Units of the primary and secondary reading ('' = unitless, '°' or 'rad' for θ). */
  units: [string, string]
  name: string
  /** Front-panel primary key: L, C, R, Z, Y, G or DCR. */
  group: string
}

type Canon = [string, string | null, 'SER' | 'PAR' | null, string, string]
/** Canonical E4980/Tonghui function tokens (definitions may override any field in features). */
const LCR_CANON: Record<string, Canon> = {
  CPD: ['Cp', 'D', 'PAR', 'F', ''], CPQ: ['Cp', 'Q', 'PAR', 'F', ''], CPG: ['Cp', 'G', 'PAR', 'F', 'S'], CPRP: ['Cp', 'Rp', 'PAR', 'F', 'Ω'],
  CSD: ['Cs', 'D', 'SER', 'F', ''], CSQ: ['Cs', 'Q', 'SER', 'F', ''], CSRS: ['Cs', 'Rs', 'SER', 'F', 'Ω'], CSESR: ['Cs', 'ESR', 'SER', 'F', 'Ω'],
  LPD: ['Lp', 'D', 'PAR', 'H', ''], LPQ: ['Lp', 'Q', 'PAR', 'H', ''], LPG: ['Lp', 'G', 'PAR', 'H', 'S'], LPRP: ['Lp', 'Rp', 'PAR', 'H', 'Ω'], LPRD: ['Lp', 'Rdc', 'PAR', 'H', 'Ω'],
  LSD: ['Ls', 'D', 'SER', 'H', ''], LSQ: ['Ls', 'Q', 'SER', 'H', ''], LSRS: ['Ls', 'Rs', 'SER', 'H', 'Ω'], LSRD: ['Ls', 'Rdc', 'SER', 'H', 'Ω'], LSESR: ['Ls', 'ESR', 'SER', 'H', 'Ω'],
  RX: ['R', 'X', null, 'Ω', 'Ω'], RPQ: ['Rp', 'Q', 'PAR', 'Ω', ''], RSQ: ['Rs', 'Q', 'SER', 'Ω', ''],
  ZTD: ['Z', 'θ', null, 'Ω', '°'], ZTR: ['Z', 'θ', null, 'Ω', 'rad'], ZD: ['Z', 'D', null, 'Ω', ''], ZQ: ['Z', 'Q', null, 'Ω', ''],
  GB: ['G', 'B', null, 'S', 'S'], YTD: ['Y', 'θ', null, 'S', '°'], YTR: ['Y', 'θ', null, 'S', 'rad'],
  DCR: ['DCR', null, null, 'Ω', ''],
}
/** Primary keys in front-panel order (handhelds: L / C / R / Z / DCR). */
export const LCR_GROUPS = ['L', 'C', 'R', 'Z', 'Y', 'G', 'DCR']
const LCR_SECONDARY_ORDER = ['D', 'Q', 'θ', 'ESR', 'Rs', 'Rp', 'X', 'G', 'B', 'Rdc']

/** The pair behind a function key: features entry over the canonical token. */
export function lcrFunction(key: string, f: any = {}): LcrFunction {
  const c: Canon = LCR_CANON[key.toUpperCase()] ?? [key, null, null, '', '']
  const primary = typeof f?.primary === 'string' ? f.primary : c[0]
  const secondary = f?.secondary === undefined ? c[1] : (f.secondary || null)
  const circuit = f?.circuit === 'SER' || f?.circuit === 'PAR' ? f.circuit : f?.circuit === undefined ? c[2] : null
  const units: [string, string] = Array.isArray(f?.units) ? [String(f.units[0] ?? c[3]), String(f.units[1] ?? c[4])] : [c[3], c[4]]
  const group = primary.toUpperCase() === 'DCR' ? 'DCR' : primary.charAt(0).toUpperCase()
  return { key, primary, secondary, circuit, units, name: f?.ui_name ?? (secondary ? `${primary}-${secondary}` : primary), group }
}

/** A discrete list (chips) or a continuous range (digit editor + presets) for frequency / level. */
export type LcrSetting =
  | { kind: 'list'; values: { value: string; num: number }[] }
  | { kind: 'range'; spec: NumberSpec; presets: number[] }
  | null

/**
 * Volt-scale settings whose wire format counts mV (`scale: 1000, format: %.0f`; `/methods` does not
 * report the scale): keep millivolt resolution instead of whole volts.
 */
function fineVolts(spec: NumberSpec | null): NumberSpec | null {
  if (!spec || spec.decimals > 0 || spec.max > 10) return spec
  return { ...spec, decimals: 3 }
}

function lcrSetting(setter: Setter | undefined, f: any, unit: string): LcrSetting {
  const v = setter?.value
  if (!v) return null
  if (v.type === 'enum') {
    const values = (v.values ?? []).map((s) => ({ value: s, num: Number(s) })).filter((x) => Number.isFinite(x.num))
    return { kind: 'list', values: values.sort((a, b) => a.num - b.num) }
  }
  const n = numberSpec(setter, { min: f?.min, max: f?.max, unit })
  const spec = unit === 'V' ? fineVolts(n && { ...n, min: Math.max(isNum(v.min) ? v.min : 0, isNum(f?.min) ? f.min : 0) }) : n
  if (!spec) return null
  // a model with discrete points (features list) behind a numeric setter shared with continuous models
  if (Array.isArray(f?.list) && f.list.length) {
    // raw setter limits: a scaled wire format (`%.0f` in mV) would round the spec's limits
    const lo = Math.max(isNum(v.min) ? v.min : -Infinity, isNum(f?.min) ? f.min : -Infinity)
    const hi = Math.min(isNum(v.max) ? v.max : Infinity, isNum(f?.max) ? f.max : Infinity)
    const values = f.list.filter((x: unknown): x is number => isNum(x) && x >= lo && x <= hi)
      .sort((a: number, b: number) => a - b).map((n: number) => ({ value: String(n), num: n }))
    if (values.length) return { kind: 'list', values }
  }
  const presets = (Array.isArray(f?.presets) ? f.presets : [])
    .filter((x: unknown): x is number => isNum(x) && x >= spec.min && x <= spec.max)
  return { kind: 'range', spec, presets }
}

/** Options of an enum setter, labelled from a features list `[{value, display}]`. */
function enumOptions(setters: Setters, op: string, order: string[], display: any): Option[] {
  const names = new Map<string, string>(Array.isArray(display) ? display.map((d: Option) => [String(d.value), String(d.display ?? d.value)]) : [])
  return ordered(setters, op, order).map((v) => ({ value: v, display: names.get(v) ?? v }))
}

export function lcrProfile(features: any, setters: Setters) {
  const fdefs = features?.functions && typeof features.functions === 'object' && !Array.isArray(features.functions) ? features.functions : {}
  const keys: string[] = setters.function?.value?.values?.length ? setters.function.value.values : Object.keys(fdefs)
  const groupRank = (g: string) => { const i = LCR_GROUPS.indexOf(g); return i < 0 ? 99 : i }
  const secRank = (s: string | null) => { const i = LCR_SECONDARY_ORDER.indexOf(s ?? ''); return i < 0 ? 99 : i }
  const functions = keys.map((k) => lcrFunction(k, fdefs[k]))
    .sort((a, b) => groupRank(a.group) - groupRank(b.group) || secRank(a.secondary) - secRank(b.secondary) || (a.circuit ?? '').localeCompare(b.circuit ?? ''))
  const counts = displayDigits(features?.counts)
  const rangeNames = Array.isArray(features?.ranges) ? features.ranges : []
  const ranges = ordered(setters, 'range', ['AUTO', 'HOLD']).map((v) => {
    const d = rangeNames.find((r: Option) => String(r.value) === v)?.display
    const n = Number(v)
    return { value: v, display: d ?? (Number.isFinite(n) && v.trim() !== '' ? siText(n, 'Ω', 3) : v) }
  })
  // numeric ranges ascending after AUTO/HOLD
  ranges.sort((a, b) => {
    const na = Number(a.value), nb = Number(b.value)
    const fa = Number.isFinite(na), fb = Number.isFinite(nb)
    return fa && fb ? na - nb : fa ? 1 : fb ? -1 : 0
  })
  return {
    functions,
    canFunction: !!setters.function,
    circuit: enumSetter(setters, 'circuit'),
    frequency: lcrSetting(setters.frequency, features?.frequency, 'Hz'),
    level: lcrSetting(setters.level, features?.level, 'V'),
    speeds: enumOptions(setters, 'speed', ['FAST', 'MED', 'SLOW'], features?.speeds),
    ranges,
    triggerSources: ordered(setters, 'trigger_source', ['INT', 'BUS', 'MAN', 'EXT', 'HOLD']),
    trigger: !!setters.trigger,
    corrections: ordered(setters, 'correction', ['OPEN', 'SHORT', 'AUTO']),
    correctionSwitches: ['open_correction', 'short_correction'].filter((op) => enumSetter(setters, op)),
    bias: fineVolts(numberSpec(setters.bias, { ...(features?.bias ?? {}), unit: 'V' })),
    /** The bias setter itself applies the bias (no separate output switch): a value needs two presses. */
    biasEnergizes: !!setters.bias?.energizes,
    biasOutput: !!setters.bias_output,
    remote: enumSetter(setters, 'remote'),
    counts,
  }
}
export type LcrProfile = ReturnType<typeof lcrProfile>

// ---- SAL -------------------------------------------------------------------------------

/** 1-3-10 sequence (RBW / VBW steps) between min and max. */
export function steps1310(min: number, max: number): number[] {
  const out: number[] = []
  if (!(min > 0) || !(max >= min)) return out
  for (let e = Math.floor(Math.log10(min)); e <= Math.ceil(Math.log10(max)); e++) {
    for (const m of [1, 3]) {
      const v = Number((m * 10 ** e).toPrecision(6))
      if (v >= min * 0.9999 && v <= max * 1.0001) out.push(v)
    }
  }
  return out
}

/** Bandwidth steps: an enum setter's values, else the features list, else 1-3-10 over the setter's limits. */
function bandwidths(setter: Setter | undefined, list: unknown): number[] {
  const v = setter?.value
  if (!v) return []
  if (v.type === 'enum') return (v.values ?? []).map(Number).filter(Number.isFinite).sort((a, b) => a - b)
  if (Array.isArray(list)) return list.filter(isNum).sort((a, b) => a - b)
  return isNum(v.min) && isNum(v.max) ? steps1310(v.min, v.max) : []
}

export function salProfile(features: any, setters: Setters) {
  const fr = features?.freq_range ?? {}
  const freq = (op: string, min?: number) => numberSpec(setters[op], { min: min ?? fr.min, max: fr.max, unit: 'Hz' })
  const range = (r: any, s: Setter | undefined, min: number, max: number) => ({
    min: isNum(r?.min) ? r.min : isNum(s?.value?.min) ? s!.value!.min! : min,
    max: isNum(r?.max) ? r.max : isNum(s?.value?.max) ? s!.value!.max! : max,
  })
  const att = features?.attenuation ?? {}
  const scales = setters.scale ? [1, 2, 5, 10].filter((s) => s >= (setters.scale?.value?.min ?? 0) && s <= (setters.scale?.value?.max ?? 20)) : []
  return {
    hdivs: isNum(features?.divisions?.horizontal) ? features.divisions.horizontal : 10,
    vdivs: isNum(features?.divisions?.vertical) ? features.divisions.vertical : 10,
    freqRange: isNum(fr.min) && isNum(fr.max) ? { min: fr.min as number, max: fr.max as number } : null,
    center: freq('center'),
    span: freq('span', 0),
    start: freq('start'),
    stop: freq('stop'),
    fullSpan: !!setters.full_span,
    rbw: bandwidths(setters.rbw, features?.rbw),
    rbwAuto: enumSetter(setters, 'rbw_auto'),
    vbw: bandwidths(setters.vbw, features?.vbw),
    vbwAuto: enumSetter(setters, 'vbw_auto'),
    refLevel: setters.ref_level ? range(features?.ref_level, setters.ref_level, -100, 30) : null,
    scales,
    attenuation: setters.attenuation ? { ...range(att, setters.attenuation, 0, 50), step: isNum(att.step) ? att.step as number : 10 } : null,
    attenAuto: enumSetter(setters, 'atten_auto'),
    preamp: enumSetter(setters, 'preamp'),
    sweeps: ordered(setters, 'sweep', ['CONT', 'SINGLE', 'HOLD', 'RESTART']),
    sweepTime: setters.sweep_time ? range(null, setters.sweep_time, 1e-6, 1000) : null,
    traceModes: ordered(setters, 'trace_mode', ['WRITE', 'MAXHOLD', 'MINHOLD', 'AVERAGE', 'VIEW']),
    detectors: ordered(setters, 'detector', ['POS', 'NEG', 'NORM', 'SAMP', 'RMS', 'AVER', 'QPE']),
    marker: enumSetter(setters, 'marker'),
    markerFreq: freq('marker_freq'),
    peaks: ordered(setters, 'peak_search', ['PEAK', 'NEXT', 'LEFT', 'RIGHT', 'MKR_CF']),
    // the generator control only on models that have one (definitions bind tg_output family-wide)
    tg: setters.tg_output && features?.tg ? { level: numberSpec(setters.tg_level, { ...(features?.tg ?? {}), unit: 'dBm' }) } : null,
  }
}
export type SalProfile = ReturnType<typeof salProfile>
