/**
 * Recorder dock at the bottom of the main window. Collapsed: a bar with the active recordings
 * (REC mark, name, elapsed time, series count) and "+ New recording". Expanded: the selected
 * recording's live chart (last 10 minutes or all; or a view a distribution adds, `views.tsx`), its
 * series legend, Pause/Resume and Stop.
 * Expanded state, height and selection are remembered per browser, hidden series per recording
 * (`hiddenSeries.ts`).
 */
import React, { useEffect, useMemo, useRef, useState } from 'react'
import { Selector } from '../kit'
import { buildOption, chartSeries, LIVE_WINDOW_MS } from './chartModel'
import { ConfirmButton, durationText, SeriesLegend, useChartTheme, useNow } from './charts'
import { NewRecordingForm } from './NewRecordingForm'
import { RecordingsApi, useRecordings } from './RecordingsProvider'
import { useViewOnly } from '../remote/RemoteProvider'
import { isActive, Recording, RecordingData } from './types'
import { RecordingPlot, useRecordingView, ViewSwitch } from './views'
import { useHiddenSeries } from './hiddenSeries'
import './recording.css'

export const DOCK_STORAGE_KEY = 'benchmesh:recorder-dock'
export const DOCK_MIN_H = 160
const DOCK_DEFAULT_H = 300

interface DockPrefs { expanded: boolean; height: number; selected: number | null }

export function loadDockPrefs(): DockPrefs {
  const def: DockPrefs = { expanded: false, height: DOCK_DEFAULT_H, selected: null }
  try {
    const v = JSON.parse(localStorage.getItem(DOCK_STORAGE_KEY) ?? 'null')
    if (!v || typeof v !== 'object') return def
    return {
      expanded: v.expanded === true,
      height: typeof v.height === 'number' && Number.isFinite(v.height) ? Math.max(DOCK_MIN_H, v.height) : def.height,
      selected: typeof v.selected === 'number' ? v.selected : null,
    }
  } catch {
    return def
  }
}

function saveDockPrefs(p: DockPrefs) {
  try { localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify(p)) } catch { /* storage unavailable */ }
}

/** Elapsed recording time: up to now, or up to the pause. */
export function elapsedOf(r: Recording, now: number): number {
  const end = r.state === 'stopped' ? (r.stopped_at ?? now) : r.state === 'paused' ? (r.paused_at ?? now) : now
  return end - r.started_at
}

export function RecorderDock() {
  const rec = useRecordings()
  if (!rec) return null
  return <Dock rec={rec} />
}

function Dock({ rec }: { rec: RecordingsApi }) {
  const [prefs, setPrefs] = useState(loadDockPrefs)
  const [creating, setCreating] = useState(false)
  const viewOnly = useViewOnly()
  useEffect(() => { saveDockPrefs(prefs) }, [prefs])
  const active = rec.recordings.filter(isActive)
  const now = useNow(1000, active.length > 0)
  const selected = active.find((r) => r.id === prefs.selected) ?? active[0] ?? null
  const set = (p: Partial<DockPrefs>) => setPrefs((x) => ({ ...x, ...p }))

  return (
    <section className="rec-dock" data-expanded={prefs.expanded} aria-label="Recorder">
      {prefs.expanded && <ResizeHandle height={prefs.height} onHeight={(h) => set({ height: h })} />}
      <div className="rec-dock__bar">
        <span className="rec-dock__title">Recorder</span>
        <div className="rec-dock__list" role="list" aria-label="Active recordings">
          {active.length === 0 && <span className="rec-dock__none">No active recordings</span>}
          {active.map((r) => (
            <div key={r.id} role="listitem" className="rec-dock__entry">
              <button type="button" className="rec-dock__item" aria-pressed={prefs.expanded && selected?.id === r.id}
                aria-label={`${r.name}, ${r.state === 'paused' ? 'paused' : 'recording'}, ${durationText(elapsedOf(r, now))}, ${activeSeries(r)} series`}
                onClick={() => set({ selected: r.id, expanded: !(prefs.expanded && selected?.id === r.id) })}>
                <span className="rec-mark" data-state={r.state} aria-hidden="true">{r.state === 'paused' ? 'PAUSE' : 'REC'}</span>
                <span className="rec-dock__name">{r.name}</span>
                <span className="rec-dock__time">{durationText(elapsedOf(r, now))}</span>
                <span className="rec-dock__count">{activeSeries(r)} ser.</span>
              </button>
            </div>
          ))}
        </div>
        {!viewOnly && <button type="button" className="bench-btn rec-dock__new" aria-expanded={creating} onClick={() => setCreating((c) => !c)}>+ New recording</button>}
        <button type="button" className="bench-wbtn rec-dock__toggle" aria-expanded={prefs.expanded}
          aria-label={prefs.expanded ? 'Collapse recorder' : 'Expand recorder'} title={prefs.expanded ? 'Collapse' : 'Expand'}
          onClick={() => set({ expanded: !prefs.expanded })}>
          <svg viewBox="0 0 16 16" width="14" height="14" aria-hidden="true" fill="none" stroke="currentColor" strokeWidth="1.5" strokeLinecap="round">
            <path d={prefs.expanded ? 'M3 6l5 5 5-5' : 'M3 10l5-5 5 5'} />
          </svg>
        </button>
      </div>
      {creating && (
        <div className="rec-dock__form">
          <NewRecordingForm onCreate={rec.create} onCancel={() => setCreating(false)}
            onDone={(r) => { setCreating(false); set({ selected: r.id, expanded: true }) }} />
        </div>
      )}
      {prefs.expanded && (
        <div className="rec-dock__body" style={{ height: prefs.height }}>
          {selected ? <DockRecording key={selected.id} rec={rec} recording={selected} now={now} />
            : <p className="bench-hint rec-dock__hint">Start a recording, then right-click readings on the instrument panels to add them.</p>}
        </div>
      )}
    </section>
  )
}

const activeSeries = (r: Recording) => r.series.filter((s) => s.removed_at == null).length

function ResizeHandle({ height, onHeight }: { height: number; onHeight: (h: number) => void }) {
  const drag = useRef<{ y: number; h: number } | null>(null)
  const clamp = (h: number) => Math.round(Math.max(DOCK_MIN_H, Math.min(h, window.innerHeight * 0.8)))
  return (
    <div className="rec-dock__resize" role="separator" aria-orientation="horizontal" aria-label="Recorder height"
      aria-valuenow={height} aria-valuemin={DOCK_MIN_H} tabIndex={0} title="Drag to resize"
      onPointerDown={(e) => { drag.current = { y: e.clientY, h: height }; (e.target as HTMLElement).setPointerCapture?.(e.pointerId) }}
      onPointerMove={(e) => { if (drag.current) onHeight(clamp(drag.current.h + drag.current.y - e.clientY)) }}
      onPointerUp={() => { drag.current = null }}
      onKeyDown={(e) => {
        if (e.key === 'ArrowUp') { e.preventDefault(); onHeight(clamp(height + 20)) }
        if (e.key === 'ArrowDown') { e.preventDefault(); onHeight(clamp(height - 20)) }
      }} />
  )
}

/** History of the live window from `/data` (re-read on reconnect), live ticks are added on top. */
function useWindowData(rec: RecordingsApi, recording: Recording, all: boolean) {
  const [data, setData] = useState<RecordingData | null>(null)
  const { client, epoch } = rec
  useEffect(() => {
    let cancelled = false
    const from = all ? undefined : Date.now() - LIVE_WINDOW_MS
    client.data(recording.id, { from, max_points: 2000 }).then((d) => { if (!cancelled) setData(d) }).catch(() => { if (!cancelled) setData(null) })
    return () => { cancelled = true }
  }, [client, recording.id, all, epoch])
  return data
}

function DockRecording({ rec, recording, now }: { rec: RecordingsApi; recording: Recording; now: number }) {
  const [all, setAll] = useState(false)
  const [hidden, toggleHidden] = useHiddenSeries(recording.id)
  const [error, setError] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [removing, setRemoving] = useState<Set<number>>(new Set())
  const theme = useChartTheme()
  const choice = useRecordingView()
  const data = useWindowData(rec, recording, all)
  const live = rec.live[recording.id]
  const series = useMemo(() => chartSeries(recording, data, live, theme.palette).filter((s) => !s.removed), [recording, data, live, theme])
  const lastT = Math.max(recording.last_t ?? 0, ...series.map((s) => s.points[s.points.length - 1]?.[0] ?? 0)) || now
  // live window: the last 10 minutes, or from the start while the recording is younger
  const min = all ? undefined : Math.max(lastT - LIVE_WINDOW_MS, recording.started_at)
  const max = all ? undefined : lastT
  const option = useMemo(() => buildOption({ series, theme, hidden, min, max }), [series, theme, hidden, min, max])

  const act = (p: Promise<unknown>) => {
    setBusy(true)
    setError(null)
    return p.catch((e) => setError(e instanceof Error ? e.message : String(e))).finally(() => setBusy(false))
  }
  const paused = recording.state === 'paused'

  return (
    <div className="rec-live">
      <div className="rec-live__head">
        <span className="rec-mark" data-state={recording.state} aria-hidden="true">{paused ? 'PAUSE' : 'REC'}</span>
        <h3 className="rec-live__name">{recording.name}</h3>
        <span className="rec-live__meta">every {recording.interval_s} s · {durationText(elapsedOf(recording, now))}</span>
        <div className="rec-live__actions">
          <ViewSwitch {...choice} />
          <Selector label="Time window" value={all ? 'all' : '10m'} onChange={(v) => setAll(v === 'all')}
            options={[{ value: '10m', label: '10 min', title: 'Follow the last 10 minutes' }, { value: 'all', label: 'All', title: 'The whole recording' }]} />
          <button type="button" className="bench-btn" disabled={busy} aria-busy={busy}
            onClick={() => act(paused ? rec.resume(recording.id) : rec.pause(recording.id))}>{paused ? 'Resume' : 'Pause'}</button>
          <ConfirmButton label="Stop" confirmLabel="Stop recording" question="Stop for good? A stopped recording cannot be resumed."
            disabled={busy} danger onConfirm={() => rec.stop(recording.id)} />
        </div>
      </div>
      {error && <span className="kit-error" role="alert">{error}</span>}
      <div className="rec-live__main">
        <div className="rec-chart">
          <RecordingPlot view={choice.view} chart={{ option, label: `Live chart of ${recording.name}` }}
            data={{ recording, series, hidden, min, max, live: true }} />
        </div>
        <SeriesLegend series={series} hidden={hidden} removing={removing}
          onToggle={toggleHidden}
          onRemove={(id) => {
            setRemoving((r) => new Set(r).add(id))
            act(rec.removeSeries(recording.id, id)).finally(() => setRemoving((r) => { const n = new Set(r); n.delete(id); return n }))
          }} />
      </div>
    </div>
  )
}
