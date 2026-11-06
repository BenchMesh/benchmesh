import React, { useEffect, useState } from 'react'
import { Annunciators, MicroSwitch, ScopeScreen, ScopeTrace, Selector, Stepper, siNumber, siText, step125 } from '../../kit'
import { OscProfile } from '../profiles'
import { ChannelApi, channelStatus, useAction, useChannelApi } from '../api'
import { Recordable } from '../../recording/signal'
import './osc.css'

const SLOPE_LABEL: Record<string, string> = { POS: '↑', NEG: '↓', RFAL: '↕' }
const SLOPE_TITLE: Record<string, string> = { POS: 'Rising edge', NEG: 'Falling edge', RFAL: 'Either edge' }
const RUN_TITLE: Record<string, string> = {
  RUN: 'Run / stop acquisition', SINGLE: 'Single shot: wait for one trigger', AUTO: 'Autoscale (changes all settings)', CLEAR: 'Clear the screen',
}
const MEAS_LABEL: Record<string, [string, string]> = { VPP: ['Vpp', 'V'], VAVG: ['Vavg', 'V'], VRMS: ['Vrms', 'V'], FREQ: ['Freq', 'Hz'] }
const IMPEDANCE_LABEL: Record<string, string> = { '1M': '1 MΩ', '50': '50 Ω' }
const ARM_MS = 3000
const sourceLabel = (s: string) => (s.startsWith('CHAN') ? `CH${s.slice(4)}` : s)
const chVar = (ch: string) => `var(--kit-ch${Math.min(4, Number(ch) || 1)})`

/** Screen trace of a channel from its status block, or null when there is none to draw. */
export function traceOf(ch: string, s: Record<string, any>): ScopeTrace | null {
  const scale = siNumber(s.SCALE)
  if (s.DISPLAY !== 'ON' || !Array.isArray(s.WAVE) || scale == null) return null
  const [y0, dy] = [siNumber(s.WAVE_Y0), siNumber(s.WAVE_DY)]
  if (y0 == null || dy == null) return null
  return { ch, codes: s.WAVE, y0, dy, offset: siNumber(s.OFFSET) ?? 0, scale }
}

/** One vertical channel: on/off, V/div, position, coupling, probe, bandwidth limit, invert. */
function OscChannel({ id, ch, status, profile, online }: {
  id: string; ch: string; status: Record<string, any>; profile: OscProfile; online: boolean
}) {
  const api = useChannelApi('OSC', id, ch)
  const [error, run] = useAction()
  const on = status.DISPLAY === 'ON'
  const scale = siNumber(status.SCALE)
  const offset = siNumber(status.OFFSET) ?? 0
  const probe = siNumber(status.PROBE) ?? 1
  const set = (op: string, v: string | number) => api.set(op, v)
  const sendTo = (op: string, v: number) => api.set(op, v).then(() => v)
  // 50 ohm can damage an input that is fine at 1 Mohm: a second press within 3 s sends it
  const [arming50, setArming50] = useState(false)
  useEffect(() => {
    if (!arming50) return
    const t = setTimeout(() => setArming50(false), ARM_MS)
    return () => clearTimeout(t)
  }, [arming50])
  const setImpedance = (v: string) => {
    if (v === '50' && !arming50) { setArming50(true); return Promise.resolve(false) }
    setArming50(false)
    return run(set('impedance', v))
  }
  return (
    <section className={`bench-osc__ch${on ? '' : ' bench-osc__ch--off'}`} aria-label={`Channel ${ch}`} style={{ '--ch': chVar(ch) } as React.CSSProperties}>
      <div className="bench-osc__ch-head">
        <span className="bench-osc__ch-tag">CH{ch}</span>
        {profile.display && <MicroSwitch label="On" checked={on} disabled={!online} onChange={(v) => set('display', v ? 'ON' : 'OFF')} />}
      </div>
      {on && (
        <>
          <div className="kit-row">
            <Stepper label={`CH${ch} scale`} disabled={!online || scale == null} text={`${siText(scale, 'V')}/div`} value={scale} format={(v) => `${siText(v, 'V')}/div`}
              onStep={(d) => sendTo('scale', step125(scale!, d, profile.scale.min * probe, profile.scale.max * probe))} />
            {profile.offset && (
              <Stepper label={`CH${ch} position`} disabled={!online || scale == null} text={siText(offset, 'V')} value={offset} format={(v) => siText(v, 'V')}
                title="Vertical offset, 0.2 div per step"
                onStep={(d) => sendTo('offset', Number((offset + d * 0.2 * scale!).toPrecision(6)))} />
            )}
          </div>
          {profile.couplings.length > 0 && (
            <Selector label={`CH${ch} coupling`} value={status.COUPLING ?? null} disabled={!online}
              options={profile.couplings.map((c) => ({ value: c, label: c }))} onChange={(v) => run(set('coupling', v))} />
          )}
          {profile.impedances.length > 0 && (
            <Selector label={`CH${ch} input impedance`} value={status.IMPEDANCE ?? null} disabled={!online}
              options={profile.impedances.map((z) => ({ value: z, label: IMPEDANCE_LABEL[z] ?? z, title: 'Input impedance' }))} onChange={setImpedance} />
          )}
          {arming50 && (
            <span className="kit-error" role="alert">
              Press 50 Ω again: {profile.impedance50Limit ? `this input takes at most ${profile.impedance50Limit}` : 'check the signal suits a 50 Ω input'}
            </span>
          )}
          {profile.probes.length > 0 && (
            <Selector variant="chips" label={`CH${ch} probe`} value={profile.probes.find((p) => Number(p) === probe) ?? null} disabled={!online}
              options={profile.probes.map((p) => ({ value: p, label: `${p}×`, title: `Probe ${p}:1` }))} onChange={(v) => run(set('probe', v))} />
          )}
          <div className="kit-row">
            {profile.bwlimit && profile.bwlimits.length === 1 && (
              <MicroSwitch label={`BW ${profile.bwlimits[0]}`} checked={status.BWLIMIT != null && status.BWLIMIT !== 'OFF'} disabled={!online}
                title="Bandwidth limit" onChange={(v) => set('bwlimit', v ? profile.bwlimits[0] : 'OFF')} />
            )}
            {profile.bwlimit && profile.bwlimits.length > 1 && (
              <Selector variant="chips" label={`CH${ch} bandwidth limit`} value={typeof status.BWLIMIT === 'string' ? status.BWLIMIT : null} disabled={!online}
                options={['OFF', ...profile.bwlimits].map((b) => ({ value: b, label: b === 'OFF' ? 'BW full' : b, title: b === 'OFF' ? 'No bandwidth limit' : `Bandwidth limit ${b}` }))}
                onChange={(v) => run(set('bwlimit', v))} />
            )}
            {profile.invert && <MicroSwitch label="Invert" checked={status.INVERT === 'ON'} disabled={!online}
              onChange={(v) => set('invert', v ? 'ON' : 'OFF')} />}
          </div>
        </>
      )}
      {error && <span className="kit-error" role="alert">{error}</span>}
    </section>
  )
}

/**
 * Oscilloscope: the screen (all displayed channels) with run state and measurements, the
 * acquisition / horizontal / trigger controls, and one strip per vertical channel.
 */
export function OscPanel({ id, channels, registry, profile, online }: {
  id: string; channels: string[]; registry: any; profile: OscProfile; online: boolean
}) {
  const api: ChannelApi = useChannelApi('OSC', id, channels[0] ?? '1')
  const [error, run] = useAction()
  const sendTo = (op: string, v: number) => api.set(op, v).then(() => v)
  const st = Object.fromEntries(channels.map((ch) => [ch, channelStatus(registry, id, 'OSC', ch)]))
  const g = st[channels[0]] ?? {}
  const traces = channels.map((ch) => traceOf(ch, st[ch])).filter((t): t is ScopeTrace => t != null)
  const shown = channels.filter((ch) => st[ch].DISPLAY === 'ON')

  const status = typeof g.TRIG_STATUS === 'string' ? g.TRIG_STATUS : null
  const running = status != null && status !== 'STOP'
  const timebase = siNumber(g.TIMEBASE)
  const tOffset = siNumber(g.TIMEOFFSET) ?? 0
  const srcCh = typeof g.TRIG_SOURCE === 'string' && g.TRIG_SOURCE.startsWith('CHAN') ? g.TRIG_SOURCE.slice(4) : null
  const level = siNumber(g.TRIG_LEVEL)
  const srcScale = srcCh ? siNumber(st[srcCh]?.SCALE) : null
  const srcOffset = srcCh ? siNumber(st[srcCh]?.OFFSET) ?? 0 : 0
  const trigger = srcCh && level != null && srcScale && st[srcCh]?.DISPLAY === 'ON'
    ? { ch: srcCh, levelDivs: (level + srcOffset) / srcScale } : null

  const runOptions = profile.runActions.filter((a) => a !== 'STOP')
  const summary = `${siText(timebase, 's')}/div · ${siText(siNumber(g.SRATE), 'Sa/s')} · trigger ${sourceLabel(g.TRIG_SOURCE ?? '-')} ${SLOPE_LABEL[g.TRIG_SLOPE] ?? ''} ${siText(level, 'V')}`

  return (
    <div className="bench-osc">
      <div className="kit-glass bench-osc__screen">
        <div className="bench-osc__bar">
          <Annunciators items={[
            { label: 'RUN', on: running },
            { label: 'STOP', on: status === 'STOP', alert: true },
            { label: "T'D", on: status === 'TD', title: 'Triggered' },
            { label: 'WAIT', on: status === 'WAIT', title: 'Waiting for a trigger' },
            { label: g.TRIG_SWEEP ?? 'AUTO', on: g.TRIG_SWEEP != null, title: 'Trigger sweep' },
            { label: g.ACQ_TYPE ?? 'NORM', on: g.ACQ_TYPE != null, title: 'Acquisition type' },
          ]} />
          <span className="bench-osc__summary">{summary}</span>
        </div>
        <ScopeScreen traces={traces} hdivs={profile.hdivs} vdivs={profile.vdivs}
          channels={shown.map((ch) => {
            const scale = siNumber(st[ch].SCALE)
            return { ch, offsetDivs: scale ? (siNumber(st[ch].OFFSET) ?? 0) / scale : 0 }
          })}
          trigger={trigger} triggerPosDivs={timebase ? -tOffset / timebase : 0}
          label={`Oscilloscope screen: ${shown.length ? shown.map((c) => `CH${c}`).join(', ') : 'no channels'} shown, ${summary}`} />
        <div className="bench-osc__legend">
          {shown.map((ch) => (
            <div key={ch} className="bench-osc__legend-ch" style={{ '--ch': chVar(ch) } as React.CSSProperties}>
              <b>CH{ch}</b>
              <span>{siText(siNumber(st[ch].SCALE), 'V')}/div {st[ch].COUPLING ?? ''}{st[ch].IMPEDANCE === '50' ? ' 50 Ω' : ''}</span>
              {profile.measurements.map((m) => (
                <Recordable key={m} inline field={m} unit={MEAS_LABEL[m]?.[1] ?? ''} channel={ch}><i>{MEAS_LABEL[m]?.[0] ?? m}</i>{siText(siNumber(st[ch][m]), MEAS_LABEL[m]?.[1] ?? '')}</Recordable>
              ))}
            </div>
          ))}
        </div>
      </div>

      <div className="bench-osc__side">
        <div className="kit-field">
          <span className="kit-label">Acquisition</span>
          <div className="kit-row">
            {profile.runActions.includes('STOP') && (
              <button type="button" className={`bench-osc__run${running ? ' bench-osc__run--on' : ''}`} disabled={!online}
                aria-pressed={running} title={RUN_TITLE.RUN} onClick={() => run(api.set('run', running ? 'STOP' : 'RUN'))}>
                <span className="bench-osc__lamp" />{running ? 'Running' : 'Stopped'}
              </button>
            )}
            {runOptions.filter((a) => a !== 'RUN').map((a) => (
              <button key={a} type="button" className="bench-btn" disabled={!online} title={RUN_TITLE[a]} onClick={() => run(api.set('run', a))}>
                {a === 'AUTO' ? 'Auto' : a === 'SINGLE' ? 'Single' : a[0] + a.slice(1).toLowerCase()}
              </button>
            ))}
          </div>
          {profile.acqTypes.length > 0 && (
            <Selector label="Acquisition type" value={g.ACQ_TYPE ?? null} disabled={!online}
              options={profile.acqTypes.map((t) => ({ value: t, label: t }))} onChange={(v) => run(api.set('acquire_type', v))} />
          )}
        </div>
        <div className="kit-row">
          <Stepper label="Time/div" disabled={!online || timebase == null} text={`${siText(timebase, 's')}/div`} value={timebase} format={(v) => `${siText(v, 's')}/div`}
            onStep={(d) => sendTo('timebase', step125(timebase!, d, profile.timebase.min, profile.timebase.max))} />
          {profile.timebaseOffset && (
            <Stepper label="H position" disabled={!online || timebase == null} text={siText(tOffset, 's')} value={tOffset} format={(v) => siText(v, 's')} title="Horizontal position, 0.5 div per step"
              onStep={(d) => sendTo('timebase_offset', Number((tOffset - d * 0.5 * timebase!).toPrecision(6)))} />
          )}
        </div>
        <div className="kit-field">
          <span className="kit-label">Trigger</span>
          {profile.triggerSources.length > 0 && (
            <Selector label="Trigger source" value={g.TRIG_SOURCE ?? null} disabled={!online}
              options={profile.triggerSources.map((s) => ({ value: s, label: sourceLabel(s) }))} onChange={(v) => run(api.set('trigger_source', v))} />
          )}
          <div className="kit-row">
            {profile.slopes.length > 0 && (
              <Selector label="Trigger slope" value={g.TRIG_SLOPE ?? null} disabled={!online}
                options={profile.slopes.map((s) => ({ value: s, label: SLOPE_LABEL[s] ?? s, title: SLOPE_TITLE[s] }))} onChange={(v) => run(api.set('trigger_slope', v))} />
            )}
            {profile.sweeps.length > 0 && (
              <Selector label="Trigger sweep" value={g.TRIG_SWEEP ?? null} disabled={!online}
                options={profile.sweeps.map((s) => ({ value: s, label: s }))} onChange={(v) => run(api.set('trigger_sweep', v))} />
            )}
          </div>
          {profile.triggerLevel && (
            <Stepper label="Trigger level" disabled={!online || level == null} text={siText(level, 'V')}
              title="0.1 div of the source channel per step"
              value={level} format={(v) => siText(v, 'V')} onStep={(d) => sendTo('trigger_level', Number(((level ?? 0) + d * 0.1 * (srcScale ?? 0.1)).toPrecision(6)))} />
          )}
        </div>
        {error && <span className="kit-error" role="alert">{error}</span>}
      </div>

      <div className="bench-osc__channels">
        {channels.map((ch) => <OscChannel key={ch} id={id} ch={ch} status={st[ch]} profile={profile} online={online} />)}
      </div>
    </div>
  )
}
