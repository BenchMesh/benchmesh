import React, { useEffect, useState } from 'react'
import { Readout, Annunciators, RotaryDial, MeterSymbol, Selector, MicroSwitch, Bargraph, prefixed, siNumber } from '../../kit'
import { DmmProfile } from '../profiles'
import { ChannelApi, useAction } from '../api'
import { Recordable } from '../../recording/signal'

const PREFIX: Record<string, number> = { p: 1e-12, n: 1e-9, 'µ': 1e-6, u: 1e-6, m: 1e-3, k: 1e3, K: 1e3, M: 1e6, G: 1e9 }

/** Value of a range label such as `200mV`, `2kΩ` or `10000µF` (null when it is not one). */
export function rangeValue(display: string | undefined): number | null {
  const m = /^\s*([0-9]+(?:\.[0-9]+)?)\s*([pnµumkKMG]?)\s*[A-Za-zΩ°]*\s*$/.exec(display ?? '')
  if (!m) return null
  return Number(m[1]) * (m[2] ? PREFIX[m[2]] : 1)
}

/**
 * Full scale of the active range: the range value, the value of its label for meters that
 * report a range index (`2` = "20V"), or the decade above the reading in AUTO.
 */
export function fullScale(range: string | null, reading: number | null, options: { value: string; display: string }[] = []): number | null {
  const label = range != null ? options.find((o) => o.value === range)?.display : undefined
  const fromLabel = label != null && label !== range ? rangeValue(label) : null
  if (fromLabel != null && fromLabel > 0) return fromLabel
  const r = range != null ? Number(range) : NaN
  if (Number.isFinite(r) && r > 0) return r
  if (reading == null || reading === 0) return null
  return 10 ** Math.ceil(Math.log10(Math.abs(reading) * 1.0001))
}

interface Stats { min: number; max: number; sum: number; n: number }
const NO_STATS: Stats = { min: Infinity, max: -Infinity, sum: 0, n: 0 }

/** Running min/max/avg: one sample per status update (`tick`), restarted when `key` changes. */
export function useStats(value: number | null, tick: unknown, key: unknown): Stats {
  const add = (st: Stats) => (value == null ? st
    : { min: Math.min(st.min, value), max: Math.max(st.max, value), sum: st.sum + value, n: st.n + 1 })
  const [s, setS] = useState(() => ({ tick, key, stats: add(NO_STATS) }))
  if (s.tick !== tick || s.key !== key) {
    const stats = add(s.key !== key ? NO_STATS : s.stats)
    setS({ tick, key, stats })
    return stats
  }
  return s.stats
}

/** Multimeter: function dial, main reading with range bar and statistics, range and options. */
export function DmmPanel({ status, profile, api, online }: {
  status: Record<string, any>; profile: DmmProfile; api: ChannelApi; online: boolean
}) {
  // The meter reports no function while it settles after a range or function change: keep the
  // last reported one so the dial never jumps (and statistics do not restart).
  const reported = typeof status.MODE === 'string' ? status.MODE : null
  const [lastMode, setLastMode] = useState<string | null>(reported)
  if (reported != null && reported !== lastMode) setLastMode(reported)
  const mode = reported ?? lastMode ?? profile.modes[0]?.key ?? ''
  const settling = online && reported == null && lastMode != null
  const info = profile.modes.find((m) => m.key === mode)
  const [hold, setHold] = useState<number | null>(null)
  const [holdOn, setHoldOn] = useState(false)
  const [rel, setRel] = useState<number | null>(null)
  const [range, setRange] = useState<string>('AUTO')
  const [rate, setRate] = useState<string | null>(null)
  const [sensor, setSensor] = useState<string | null>(null)
  const [scale, setScale] = useState<string | null>(null)
  const [error, run] = useAction()
  // a range/function request is in flight; with `settling` the dial is greyed out meanwhile
  const [sending, setSending] = useState(false)
  const apply = (p: Promise<unknown>, onOk?: () => void) => {
    setSending(true)
    return run(p, onOk).finally(() => setSending(false))
  }
  const applying = sending || settling

  const raw = siNumber(status.MEAS)
  const live = holdOn ? hold : raw
  const shown = live != null && rel != null ? live - rel : live
  const overload = status.OVERLOAD === true
  const activeRange = typeof status.RANGE === 'string' ? status.RANGE : range

  // statistics, REL and the range selection restart with the function
  const [modeSeen, setModeSeen] = useState(mode)
  if (modeSeen !== mode) {
    setModeSeen(mode)
    setRel(null)
    setRange('AUTO')
  }
  const stats = useStats(holdOn ? null : shown, status, mode)
  // read whenever the meter comes online (added, app start, reconnect) and per function
  // (meters keep a rate per function)
  useEffect(() => {
    if (!profile.rates.length || !online) return
    let cancelled = false
    api.get('rate').then((v) => { if (!cancelled && typeof v === 'string') setRate(v.trim().toUpperCase()) }).catch(() => {})
    return () => { cancelled = true }
  }, [api, profile.rates.length, online, mode])

  const isTemp = mode.toUpperCase().startsWith('TEMP')
  // the meter converts to the chosen temperature scale: label the reading with it
  const tempUnit = isTemp && scale ? profile.tempScales.find((t) => t.value === scale)?.display : undefined
  const unit = tempUnit ?? info?.unit ?? ''
  const main = prefixed(shown, profile.counts, overload)
  const ranges = profile.ranges[mode] ?? []
  const fs = fullScale(activeRange, raw, ranges)
  const fsText = fs != null ? (() => { const p = prefixed(fs, 3); return `${p.text.trim().replace(/\.0+$/, '')} ${p.prefix}${unit}` })() : ''
  const fmt = (x: number) => { const p = prefixed(x, profile.counts); return `${p.text.trim()} ${p.prefix}${unit}` }

  return (
    <div className="bench-dmm">
      <RotaryDial label="Function" disabled={!online || applying} busy={applying}
        options={profile.modes.map((m) => ({ value: m.key, label: m.glyph, symbol: m.symbol, title: m.name }))}
        value={mode} onChange={(k) => apply(api.set('mode', k))} />
      <div className="bench-dmm__main">
        <div className="kit-glass">
          <Recordable field="MEAS" unit={unit}>
            <Readout tag={info ? <MeterSymbol parts={info.symbol} label={info.name} /> : mode} text={main.text} unit={isTemp ? unit : `${main.prefix}${unit}`} label={info?.name ?? 'Reading'} tone="meas" />
          </Recordable>
          <Bargraph tone="meas" label="Reading as a fraction of the range" fraction={fs && shown != null ? Math.abs(shown) / fs : overload ? 1 : 0} marks={['0', fsText]} />
          <div className="kit-stats" aria-label="Statistics">
            <span><b>MIN</b>{stats.n ? fmt(stats.min) : '-'}</span>
            <span><b>MAX</b>{stats.n ? fmt(stats.max) : '-'}</span>
            <span><b>AVG</b>{stats.n ? fmt(stats.sum / stats.n) : '-'}</span>
            <span><b>N</b>{stats.n}</span>
          </div>
          <Annunciators items={[
            { label: 'AUTO', on: activeRange === 'AUTO' },
            { label: 'HOLD', on: holdOn },
            { label: 'REL', on: rel != null },
            { label: 'OL', on: overload, alert: true },
            ...(rate ? [{ label: `RATE ${rate}`, on: true }] : []),
          ]} />
        </div>
        {profile.canRange && ranges.length > 0 && (
          <div className="kit-field">
            <span className="kit-label">Range</span>
            <Selector variant="chips" label="Range" disabled={!online} value={activeRange}
              options={ranges.map((r) => ({ value: r.value, label: r.display }))}
              onChange={(v) => apply(api.set('range', v), () => setRange(v))} />
          </div>
        )}
        {isTemp && profile.tempSensors.length > 0 && (
          <div className="kit-row">
            <Selector variant="chips" label="Sensor" disabled={!online} value={sensor}
              options={profile.tempSensors.map((s) => ({ value: s.value, label: s.display }))}
              onChange={(v) => run(api.set('temp_sensor', v), () => setSensor(v))} />
            <Selector variant="chips" label="Scale" disabled={!online} value={scale}
              options={profile.tempScales.map((s) => ({ value: s.value, label: s.display }))}
              onChange={(v) => run(api.set('temp_scale', v), () => setScale(v))} />
          </div>
        )}
        <div className="kit-row">
          <MicroSwitch label="Hold" checked={holdOn} onChange={(on) => { setHoldOn(on); setHold(on ? raw : null) }} />
          <MicroSwitch label="Rel Δ" checked={rel != null} onChange={(on) => setRel(on ? raw : null)} />
          {profile.rates.length > 0 && (
            <Selector label="Rate" disabled={!online} value={rate}
              options={profile.rates.map((r) => ({ value: r.value, label: r.value, title: r.display }))}
              onChange={(v) => run(api.set('rate', v), () => setRate(v))} />
          )}
        </div>
        {error && <span className="kit-error" role="alert">{error}</span>}
      </div>
    </div>
  )
}
