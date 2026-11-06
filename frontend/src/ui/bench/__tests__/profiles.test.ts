import { describe, it, expect } from 'vitest'
import fixtures from './bench-fixtures.json'
import { psuProfile, ellProfile, dmmProfile, awgProfile, oscProfile, setterMap, modeGlyph, displayDigits } from '../profiles'

const fx = (key: keyof typeof fixtures) => {
  const f = fixtures[key] as { features: any; methods: any[] }
  return [f.features, setterMap(f.methods)] as const
}

describe('PSU profile (same panel, per-model limits)', () => {
  it('Tenma 72-2540: model limits from features, decimals from the wire format', () => {
    const p = psuProfile(...fx('tenma-72-2540:PSU'))
    expect(p.voltage).toMatchObject({ min: 0, max: 30, intDigits: 2, decimals: 2, unit: 'V' })
    expect(p.current).toMatchObject({ min: 0, max: 5, intDigits: 1, decimals: 3, unit: 'A' })
    expect(p.output).toBe(true)
    expect(p.protectionSwitches).toEqual(['ovp', 'ocp'])
    expect(p.voltageLimit).toBeNull()
    expect(p.remote).toBe(false)
  })
  it('OWON SPM3103: mode setpoint limits and steps, protection levels, remote', () => {
    const p = psuProfile(...fx('owon-spm3103:PSU'))
    expect(p.voltage).toMatchObject({ max: 31, decimals: 2 })
    expect(p.current).toMatchObject({ max: 10.1, intDigits: 2, decimals: 3 })
    expect(p.voltageLimit).toMatchObject({ min: 0.1, max: 32, unit: 'V' })
    expect(p.currentLimit).toMatchObject({ min: 0.1, max: 10.2, unit: 'A' })
    expect(p.protectionSwitches).toEqual([])
    expect(p.remote).toBe(true)
  })
})

describe('ELL profile', () => {
  it('lists settable modes in CC/CV/CR/CP order with their setpoint ranges', () => {
    const p = ellProfile(...fx('owon-oel1515:ELL'))
    // DYN has no setpoint here but the mode setter offers it: selectable, parameters on the load
    expect(p.modes.map((m) => m.key)).toEqual(['CURR', 'VOLT', 'RES', 'POW', 'DYN'])
    expect(p.modes.map((m) => m.short)).toEqual(['CC', 'CV', 'CR', 'CP', 'DYN'])
    expect(p.modes[4]).toMatchObject({ op: null, spec: null, name: 'Dynamic' })
    const res = p.modes.find((m) => m.key === 'RES')!
    expect(res).toMatchObject({ op: 'res', name: 'Constant Resistance' })
    expect(res.spec).toMatchObject({ min: 0.05, max: 1000, intDigits: 4, decimals: 2, unit: 'Ω' })
    expect(p.modes[0].spec).toMatchObject({ max: 15, intDigits: 2, decimals: 3, unit: 'A' })
    expect(p).toMatchObject({ input: true, short: true, remote: true, sense: true, remoteRequired: true })
  })

  it('never offers a short-circuit mode, nor modes the mode setter does not take', () => {
    const features = { modes: { CURR: { setpoint: { min: 0, max: 5 } }, SHORT: { ui_name: 'Short circuit' }, LED: { ui_name: 'LED' } } }
    const setters = setterMap([
      { name: 'mode', http_method: 'POST', value: { type: 'enum', values: ['CURR', 'SHORT'] } },
      { name: 'curr', http_method: 'POST', value: { type: 'number', min: 0, max: 30 } },
    ])
    expect(ellProfile(features, setters).modes.map((m) => m.key)).toEqual(['CURR'])
  })
})

describe('DMM profile', () => {
  it('builds the dial and per-function ranges (without MIN/MAX)', () => {
    const p = dmmProfile(...fx('owon-spm3103:DMM'))
    expect(p.modes.map((m) => m.key)).toEqual(['VOLT_DC', 'VOLT_AC', 'CURR_DC', 'CURR_AC', 'RES', 'CONT', 'DIOD', 'CAP'])
    expect(p.modes.map((m) => m.glyph)).toEqual(['V⎓', 'V~', 'A⎓', 'A~', 'Ω', '·)))', '→|', '⊣⊢'])
    expect(p.ranges.VOLT_DC.map((r) => r.value)).toEqual(['AUTO', '200E-3', '2', '20', '200', '1000'])
    expect(p.canRange).toBe(true)
    expect(p.rates).toEqual([])
  })
  it('XDM1241: rates and temperature options', () => {
    const p = dmmProfile(...fx('owon-xdm1241:DMM'))
    expect(p.modes.find((m) => m.key === 'FRESistance')?.glyph).toBe('Ω4W')
    expect(p.rates.map((r) => r.value)).toEqual(['F', 'M', 'L'])
    expect(p.tempSensors.map((s) => s.value)).toEqual(['KITS90', 'PT100'])
    expect(p.tempScales.map((s) => s.value)).toEqual(['C', 'F', 'K'])
  })
  it('display digits from features.counts (digits or a count), else 5', () => {
    expect(displayDigits(undefined)).toBe(5)
    expect(displayDigits(6)).toBe(6)
    expect(displayDigits(55000)).toBe(5)
    expect(displayDigits(220000)).toBe(6)
    expect(dmmProfile({ counts: 1200000 }, {}).counts).toBe(7)
  })
  it('falls back to a short key for unknown functions', () => {
    expect(modeGlyph('WEIRD', {})).toBe('WEIR')
  })
})

describe('AWG profile', () => {
  it('takes waveforms and limits from the definition', () => {
    const p = awgProfile(...fx('owon-dge2070:AWG'))
    expect(p.shapes).toEqual(['SIN', 'SQU', 'RAMP', 'PUL', 'PRN'])
    expect(p.frequency).toMatchObject({ min: 1, max: 70e6, intDigits: 8, decimals: 0, unit: 'Hz' })
    expect(p.amplitude).toMatchObject({ min: 0.001, max: 20, intDigits: 2, decimals: 3, unit: 'Vpp' })
    expect(p.offset).toMatchObject({ min: -10, max: 10, decimals: 3 })
    expect(p.channelColors).toEqual({ 1: '#FFD700', 2: '#4169E1' })
  })

  it('phase, symmetry, the raw frequency limits (period range) and the level limit', () => {
    const p = awgProfile(...fx('owon-dge2070:AWG'))
    expect(p.frequencyLimits).toEqual({ min: 1e-6, max: 70e6 })
    expect(p.phase).toMatchObject({ min: 0, max: 360, unit: '°' })
    expect(p.symmetry).toMatchObject({ min: 0, max: 100, unit: '%' })
    expect(p.levelLimit).toBe(10)
  })

  it('DGE2070: modes, modulation, sweep, burst, pulse and load from the setters, in panel order', () => {
    const p = awgProfile(...fx('owon-dge2070:AWG'))
    expect(p.modes).toEqual(['CW', 'MOD', 'SWEEP', 'BURST'])
    expect(p.modTypes).toEqual(['AM', 'FM', 'PM'])
    expect(p.modShapes).toEqual(['SIN', 'SQU', 'RAMP', 'PRN'])
    expect(p.modFrequency).toMatchObject({ max: 100000, unit: 'Hz' })
    expect(p.amDepth).toMatchObject({ min: 0, max: 100, decimals: 1 })
    expect(p.pmDeviation).toMatchObject({ max: 180 })
    expect(p.sweepTime).toEqual({ min: 0.001, max: 500 })
    expect(p.sweepSpacings).toEqual(['LIN', 'LOG'])
    // the DGE2000 has no trigger input: internal and the front-panel key only
    expect(p.sweepTriggers).toEqual(['INT', 'MAN'])
    expect(p.burstTriggers).toEqual(['INT', 'MAN'])
    expect(p.burstModes).toEqual(['NCYC', 'INF'])
    expect(p.burstCycles).toMatchObject({ min: 1, max: 60000, decimals: 0 })
    expect(p.pulseWidth).toEqual({ min: 2.4e-8, max: 1e6 })
    expect(p.loads).toEqual(['50', 'HIGHZ'])
    expect(p.trigger).toBe(false)
  })

  it('a generator without modes offers continuous output only', () => {
    const p = awgProfile({}, setterMap([{ name: 'frequency', http_method: 'POST', value: { type: 'number', min: 1, max: 1e6 } }]))
    expect(p.modes).toEqual([])
    expect(p.loads).toEqual([])
  })
})

describe('OSC profile', () => {
  it('RIGOL DHO804: ranges from features, options from the setters, in front-panel order', () => {
    const [features, setters] = fx('rigol-dho804:OSC')
    const p = oscProfile(features, setters, 4)
    expect(p).toMatchObject({ hdivs: 10, vdivs: 8, scale: { min: 0.0005, max: 10 }, timebase: { min: 5e-9, max: 1000 } })
    expect(p.couplings).toEqual(['DC', 'AC', 'GND'])
    expect(p.probes).toEqual(['0.1', '1', '10', '100', '1000'])
    expect(p.triggerSources).toEqual(['CHAN1', 'CHAN2', 'CHAN3', 'CHAN4', 'AC'])
    expect(p.slopes).toEqual(['POS', 'NEG', 'RFAL'])
    expect(p.sweeps).toEqual(['AUTO', 'NORM', 'SING'])
    expect(p.acqTypes).toEqual(['NORM', 'PEAK', 'AVER'])
    expect(p.runActions).toEqual(['RUN', 'STOP', 'SINGLE', 'AUTO', 'CLEAR'])
    expect(p.measurements).toEqual(['VPP', 'VAVG', 'VRMS', 'FREQ'])
    expect(p).toMatchObject({ bwlimit: true, invert: true, offset: true, timebaseOffset: true, triggerLevel: true })
  })
  it('a 2-channel model only offers its own channels as trigger sources', () => {
    const [features, setters] = fx('rigol-dho804:OSC')
    expect(oscProfile(features, setters, 2).triggerSources).toEqual(['CHAN1', 'CHAN2', 'AC'])
  })
  it('input impedance comes from the setter, its 50 ohm limit from the features', () => {
    const setters = setterMap([{ name: 'impedance', http_method: 'POST', value: { type: 'enum', values: ['50', '1M'] } }])
    expect(oscProfile({ impedance_50_limit: '5 Vrms' }, setters, 1)).toMatchObject({ impedances: ['1M', '50'], impedance50Limit: '5 Vrms' })
    expect(oscProfile({}, {}, 1)).toMatchObject({ impedances: [], impedance50Limit: null })
  })
  it('works without optional features', () => {
    const p = oscProfile({}, {}, 1)
    expect(p).toMatchObject({ hdivs: 10, vdivs: 8, couplings: [], triggerSources: [], measurements: [] })
  })
})
