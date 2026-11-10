/**
 * Recordings drawer: the history (name, state, start, duration, series, samples) and one
 * recording's full chart with zoom/pan (decimated `/data`, re-queried for the zoomed range; or a
 * view a distribution adds, `views.tsx`, re-queried for the range it shows),
 * rename / description, CSV export and delete (confirmed in the UI).
 */
import React, { useEffect, useMemo, useRef, useState } from 'react'
import { buildOption, chartSeries, mergeDetail } from './chartModel'
import { saveFile, savedText } from '../files'
import { exportUrl } from './client'
import { ConfirmButton, dateTimeText, durationText, SeriesLegend, useChartTheme, useNow } from './charts'
import { elapsedOf } from './RecorderDock'
import { RecordingsApi, useRecordings } from './RecordingsProvider'
import { isActive, Recording, RecordingData } from './types'
import { RecordingPlot, useRecordingView, ViewSwitch } from './views'
import { useHiddenSeries } from './hiddenSeries'
import './recording.css'

const STATE_TEXT: Record<string, string> = { recording: 'Recording', paused: 'Paused', stopped: 'Stopped' }

export function RecordingsDrawer() {
  const rec = useRecordings()
  const [open, setOpen] = useState<number | null>(null)
  if (!rec) return null
  const r = rec.recordings.find((x) => x.id === open)
  return r ? <RecordingView rec={rec} recording={r} onBack={() => setOpen(null)} /> : <RecordingList rec={rec} onOpen={setOpen} />
}

function RecordingList({ rec, onOpen }: { rec: RecordingsApi; onOpen: (id: number) => void }) {
  const now = useNow(1000, rec.recordings.some(isActive))
  if (!rec.recordings.length) {
    return <p className="bench-hint rec-history__empty">No recordings yet. Start one from the recorder at the bottom of the bench, or right-click a reading.</p>
  }
  return (
    <ul className="rec-history" aria-label="Recordings">
      {rec.recordings.map((r) => (
        <li key={r.id}>
          <button type="button" className="rec-history__row" onClick={() => onOpen(r.id)} aria-label={`Open ${r.name}`}>
            <span className="rec-history__name">{r.name}</span>
            <span className="rec-badge" data-state={r.state}>{STATE_TEXT[r.state] ?? r.state}</span>
            <span className="rec-history__meta">
              <span title="Started">{dateTimeText(r.started_at)}</span>
              <span title="Duration">{durationText(elapsedOf(r, now))}</span>
              <span>{r.series.length} series</span>
              <span>{r.samples.toLocaleString()} samples</span>
            </span>
          </button>
        </li>
      ))}
    </ul>
  )
}

/** Overview of the whole recording, plus full-resolution detail of the zoomed range. */
function useHistoryData(rec: RecordingsApi, id: number, reload: number) {
  const [overview, setOverview] = useState<RecordingData | null>(null)
  const [detail, setDetail] = useState<{ from: number; to: number; data: RecordingData } | null>(null)
  const [zoom, setZoom] = useState<{ from: number; to: number } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const { client } = rec

  useEffect(() => {
    let cancelled = false
    client.data(id, { max_points: 2000 }).then((d) => { if (!cancelled) { setOverview(d); setDetail(null); setError(null) } })
      .catch((e) => { if (!cancelled) setError(e instanceof Error ? e.message : String(e)) })
    return () => { cancelled = true }
  }, [client, id, reload])

  // re-query the zoomed range (debounced while the user drags or scrolls)
  const timer = useRef<ReturnType<typeof setTimeout>>()
  useEffect(() => {
    if (!zoom) return
    let cancelled = false
    clearTimeout(timer.current)
    timer.current = setTimeout(() => {
      client.data(id, { from: zoom.from, to: zoom.to, max_points: 2000 })
        .then((d) => { if (!cancelled) setDetail({ ...zoom, data: d }) }).catch(() => {})
    }, 300)
    return () => { cancelled = true; clearTimeout(timer.current) }
  }, [client, id, zoom])

  const data = useMemo<RecordingData | null>(() => {
    if (!overview || !detail) return overview
    return { ...overview, series: mergeDetail(overview.series, detail.data.series, detail.from, detail.to) }
  }, [overview, detail])
  return { data, zoom, setZoom, error }
}

function RecordingView({ rec, recording, onBack }: { rec: RecordingsApi; recording: Recording; onBack: () => void }) {
  const [reload, setReload] = useState(0)
  const [editing, setEditing] = useState(false)
  const [hidden, toggleHidden] = useHiddenSeries(recording.id)
  const [actionError, setActionError] = useState<string | null>(null)
  const theme = useChartTheme()
  const choice = useRecordingView()
  const now = useNow(1000, isActive(recording))
  const { data, zoom, setZoom, error } = useHistoryData(rec, recording.id, reload)
  const series = useMemo(() => chartSeries(recording, data, undefined, theme.palette), [recording, data, theme])
  const option = useMemo(() => buildOption({ series, theme, hidden, zoom: { enabled: true, from: zoom?.from, to: zoom?.to } }), [series, theme, hidden, zoom])
  const act = (p: Promise<unknown>) => { setActionError(null); return p.catch((e) => setActionError(e instanceof Error ? e.message : String(e))) }
  const [saved, setSaved] = useState<string | null>(null)
  const exportCsv = () => { setSaved(null); return act(saveFile(`${recording.name}.csv`, { url: exportUrl(recording.id) }).then((r) => setSaved(savedText(r)))) }

  return (
    <div className="rec-view">
      <div className="rec-view__nav">
        <button type="button" className="bench-btn" onClick={onBack}>← All recordings</button>
      </div>
      {editing ? (
        <EditForm recording={recording} onSave={(p) => rec.update(recording.id, p)} onDone={() => setEditing(false)} />
      ) : (
        <div className="rec-view__head">
          <h3 className="rec-view__name">{recording.name}</h3>
          <span className="rec-badge" data-state={recording.state}>{STATE_TEXT[recording.state]}</span>
          <button type="button" className="bench-btn" onClick={() => setEditing(true)}>Edit</button>
          {recording.description && <p className="rec-view__desc">{recording.description}</p>}
        </div>
      )}
      <dl className="rec-view__facts">
        <div><dt>Started</dt><dd>{dateTimeText(recording.started_at)}</dd></div>
        <div><dt>Duration</dt><dd>{durationText(elapsedOf(recording, now))}</dd></div>
        <div><dt>Interval</dt><dd>{recording.interval_s} s</dd></div>
        <div><dt>Samples</dt><dd>{recording.samples.toLocaleString()}</dd></div>
      </dl>
      <div className="rec-view__actions">
        <button type="button" className="bench-btn" onClick={() => void exportCsv()}>Export CSV</button>
        <button type="button" className="bench-btn" onClick={() => setReload((n) => n + 1)}>Reload</button>
        <ViewSwitch {...choice} />
        {zoom && !choice.view && <button type="button" className="bench-btn" onClick={() => setZoom(null)}>Reset zoom</button>}
        {recording.state === 'recording' && <button type="button" className="bench-btn" onClick={() => act(rec.pause(recording.id))}>Pause</button>}
        {recording.state === 'paused' && <button type="button" className="bench-btn" onClick={() => act(rec.resume(recording.id))}>Resume</button>}
        {isActive(recording) && (
          <ConfirmButton label="Stop" confirmLabel="Stop recording" question="Stop for good?" danger onConfirm={() => rec.stop(recording.id)} />
        )}
        <ConfirmButton label="Delete" confirmLabel="Delete for good" question="Delete this recording and all its data?" danger
          onConfirm={async () => { await rec.remove(recording.id); onBack() }} />
      </div>
      {(error || actionError) && <span className="kit-error" role="alert">{error ?? actionError}</span>}
      {saved && <span className="bench-hint" role="status">{saved}</span>}
      <div className="rec-chart rec-chart--history">
        <RecordingPlot view={choice.view} chart={{ option, label: `Chart of ${recording.name}`, onZoom: (from, to) => setZoom({ from, to }) }}
          data={{ recording, series, hidden, live: false, onRange: (from, to) => setZoom({ from, to }) }} />
      </div>
      <SeriesLegend series={series} hidden={hidden}
        onToggle={toggleHidden} />
    </div>
  )
}

function EditForm({ recording, onSave, onDone }: {
  recording: Recording; onSave: (p: { name?: string; description?: string }) => Promise<unknown>; onDone: () => void
}) {
  const [name, setName] = useState(recording.name)
  const [description, setDescription] = useState(recording.description ?? '')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const nameError = !name.trim() ? 'Name is required' : name.trim().length > 100 ? 'At most 100 characters' : null
  return (
    <form className="rec-form rec-form--edit" aria-label="Edit recording" noValidate
      onSubmit={async (e) => {
        e.preventDefault()
        if (nameError) return
        setBusy(true)
        setError(null)
        try {
          await onSave({ name: name.trim(), description: description.trim() })
          onDone()
        } catch (err) { setError(err instanceof Error ? err.message : String(err)) } finally { setBusy(false) }
      }}>
      <div className="kit-field rec-form__name">
        <label className="kit-label" htmlFor="rec-edit-name">Name</label>
        <input id="rec-edit-name" className="kit-input" type="text" value={name} maxLength={100} aria-invalid={!!nameError} onChange={(e) => setName(e.target.value)} />
        {nameError && <span className="kit-error">{nameError}</span>}
      </div>
      <div className="kit-field rec-form__desc">
        <label className="kit-label" htmlFor="rec-edit-desc">Description</label>
        <textarea id="rec-edit-desc" className="kit-input" value={description} rows={2} onChange={(e) => setDescription(e.target.value)} />
      </div>
      <div className="rec-form__actions">
        <button type="submit" className="bench-btn bench-btn--primary" disabled={busy || !!nameError}>Save</button>
        <button type="button" className="bench-btn" onClick={onDone}>Cancel</button>
        {error && <span className="kit-error" role="alert">{error}</span>}
      </div>
    </form>
  )
}
