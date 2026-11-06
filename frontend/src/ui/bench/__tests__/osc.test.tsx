import React from 'react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen, fireEvent, waitFor, within } from '@testing-library/react'
import fixtures from './bench-fixtures.json'
import { oscProfile, setterMap } from '../profiles'
import { OscPanel, traceOf } from '../panels/OscPanel'
import { RequestLogProvider } from '../../RequestLogContext'

const fx = fixtures['rigol-dho804:OSC'] as { features: any; methods: any[] }
const profile = oscProfile(fx.features, setterMap(fx.methods), 4)
const si = (val: string) => ({ val, sci: val, si: { number: val } })

// Captured from the DHO804 (2026-10-17), trace shortened.
const ch1 = {
  ACQ_TYPE: 'NORM', BWLIMIT: 'OFF', COUPLING: 'DC', DISPLAY: 'ON', FREQ: null, INVERT: 'OFF',
  OFFSET: si('0'), PROBE: 1, SCALE: si('0.05'), SRATE: 312500000, TIMEBASE: si('0.000002'), TIMEOFFSET: si('0'),
  TRIG_LEVEL: si('0'), TRIG_SLOPE: 'POS', TRIG_SOURCE: 'CHAN1', TRIG_STATUS: 'AUTO', TRIG_SWEEP: 'AUTO',
  VAVG: -0.00071333, VPP: 0.00272, VRMS: 0.0008282, WAVE: [127, 128, 126, 127], WAVE_DX: 2e-8, WAVE_DY: 0.0017067, WAVE_X0: -0.00001, WAVE_Y0: 128,
}
const off = { ...ch1, DISPLAY: 'OFF', WAVE: null, VPP: null, VAVG: null, VRMS: null }
const registry = { 'rigol-dho804': { IDN: 'RIGOL TECHNOLOGIES,DHO804', OSC: { status_ch1: ch1, status_ch2: off, status_ch3: off, status_ch4: off } } }

let posts: string[]
beforeEach(() => {
  posts = []
  global.fetch = vi.fn(async (url: string, init?: RequestInit) => {
    if (init?.method === 'POST') posts.push(String(url).replace(/^.*\/instruments/, ''))
    return new Response(null, { status: 204 })
  }) as any
})

const renderPanel = (reg = registry, online = true) => render(
  <RequestLogProvider>
    <OscPanel id="rigol-dho804" channels={['1', '2', '3', '4']} registry={reg} profile={profile} online={online} />
  </RequestLogProvider>,
)

describe('traceOf', () => {
  it('builds a trace only for displayed channels with data', () => {
    expect(traceOf('1', ch1)).toMatchObject({ ch: '1', y0: 128, dy: 0.0017067, offset: 0, scale: 0.05, codes: [127, 128, 126, 127] })
    expect(traceOf('2', off)).toBeNull()
    expect(traceOf('1', { ...ch1, WAVE_DY: null })).toBeNull()
  })
})

describe('OscPanel', () => {
  it('summarises timebase, sample rate, trigger and measurements', () => {
    renderPanel()
    expect(screen.getByText(/2 µs\/div · 312\.5 MSa\/s · trigger CH1 ↑ 0 V/)).toBeInTheDocument()
    expect(screen.getByRole('img', { name: /CH1 shown/ })).toBeInTheDocument()
    expect(screen.getByText('2.72 mV')).toBeInTheDocument() // Vpp
    expect(screen.getByLabelText('RUN on')).toBeInTheDocument()
    expect(screen.getByLabelText('STOP off')).toBeInTheDocument()
  })

  it('steps V/div and time/div along 1-2-5 and sends each step', async () => {
    renderPanel()
    fireEvent.click(screen.getByRole('button', { name: 'CH1 scale up' }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/scale/0.1'))
    fireEvent.click(screen.getByRole('button', { name: 'Time/div down' }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/timebase/0.000001'))
  })

  it('switches channels on, stops acquisition, sets the trigger', async () => {
    renderPanel()
    const ch2 = screen.getByRole('region', { name: 'Channel 2' })
    fireEvent.click(within(ch2).getByRole('switch', { name: /On/ }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/2/display/ON'))
    fireEvent.click(screen.getByRole('button', { name: /Running/ }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/run/STOP'))
    fireEvent.click(screen.getByRole('button', { name: 'CH2' }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/trigger_source/CHAN2'))
    fireEvent.click(screen.getByRole('button', { name: '↓' }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/trigger_slope/NEG'))
  })

  it('hides the settings of channels that are off', () => {
    renderPanel()
    expect(within(screen.getByRole('region', { name: 'Channel 2' })).queryByRole('spinbutton')).toBeNull()
    expect(within(screen.getByRole('region', { name: 'Channel 1' })).getByRole('spinbutton', { name: 'CH1 scale' })).toHaveAttribute('aria-valuetext', '50 mV/div')
  })

  it('shows a stopped scope and disables control while offline', () => {
    const stopped = { ...registry, 'rigol-dho804': { ...registry['rigol-dho804'], OSC: { ...registry['rigol-dho804'].OSC, status_ch1: { ...ch1, TRIG_STATUS: 'STOP' } } } }
    renderPanel(stopped, false)
    expect(screen.getByLabelText('STOP on')).toBeInTheDocument()
    expect(screen.getByRole('button', { name: /Stopped/ })).toBeDisabled()
  })
})

describe('bandwidth limit', () => {
  it('one limit: a switch named after it; several: chips with every value of the setter', async () => {
    expect(profile.bwlimits).toEqual(['20M'])
    const methods = fx.methods.map((m) => (m.name === 'bwlimit' && m.http_method === 'POST'
      ? { ...m, value: { type: 'enum', values: ['250M', 'OFF', '20M'] } } : m))
    const p = oscProfile(fx.features, setterMap(methods), 4)
    expect(p.bwlimits).toEqual(['20M', '250M'])
    render(
      <RequestLogProvider>
        <OscPanel id="rigol-dho804" channels={['1', '2', '3', '4']} registry={registry} profile={p} online />
      </RequestLogProvider>,
    )
    const chips = screen.getByRole('group', { name: 'CH1 bandwidth limit' })
    fireEvent.click(within(chips).getByRole('button', { name: '250M' }))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/bwlimit/250M'))
  })
})

describe('OscPanel input impedance', () => {
  const methods = [...fx.methods, { name: 'impedance', http_method: 'POST', value: { type: 'enum', values: ['1M', '50'] } }]
  const features = { ...fx.features, impedance_50_limit: '5 Vrms' }
  const withImpedance = oscProfile(features, setterMap(methods), 4)
  const at = (impedance: string) => ({ 'rigol-dho804': { IDN: 'x', OSC: { status_ch1: { ...ch1, IMPEDANCE: impedance }, status_ch2: off, status_ch3: off, status_ch4: off } } })
  const show = (impedance: string, p = withImpedance) => render(
    <RequestLogProvider>
      <OscPanel id="rigol-dho804" channels={['1', '2', '3', '4']} registry={at(impedance)} profile={p} online />
    </RequestLogProvider>,
  )
  const choice = (name: string) => within(screen.getByRole('group', { name: 'CH1 input impedance' })).getByRole('button', { name })

  it('is not offered by an instrument without the setter', () => {
    show('1M', profile)
    expect(screen.queryByRole('group', { name: 'CH1 input impedance' })).toBeNull()
  })

  it('asks a second press for 50 ohm, naming the input limit, then sends it', async () => {
    show('1M')
    fireEvent.click(choice('50 Ω'))
    expect(screen.getByRole('alert')).toHaveTextContent('Press 50 Ω again: this input takes at most 5 Vrms')
    expect(posts).toEqual([])
    fireEvent.click(choice('50 Ω'))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/impedance/50'))
    expect(screen.queryByText(/Press 50 Ω again/)).toBeNull()
  })

  it('goes back to 1 Mohm with one press', async () => {
    show('50')
    fireEvent.click(choice('1 MΩ'))
    await waitFor(() => expect(posts).toContain('/OSC/rigol-dho804/1/impedance/1M'))
  })

  it('shows a 50 ohm input in the legend beside the scale', () => {
    show('50')
    expect(screen.getByText('50 mV/div DC 50 Ω')).toBeInTheDocument()
  })
})

