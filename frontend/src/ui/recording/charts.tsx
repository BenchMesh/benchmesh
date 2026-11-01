/** Chart pieces shared by the recorder dock and the recordings drawer (the chart itself loads lazily). */
import React, { lazy, Suspense, useEffect, useMemo, useState } from 'react'
import { useSkin } from '../kit'
import { ChartSeries, latest, readTheme, valueText } from './chartModel'
import type { RecordingChartProps } from './RecordingChart'

const RecordingChart = lazy(() => import('./RecordingChart'))

export function LazyChart(props: RecordingChartProps) {
  return (
    <Suspense fallback={<div className="rec-chart__canvas rec-chart__loading">Loading chart…</div>}>
      <RecordingChart {...props} />
    </Suspense>
  )
}

/** Chart colours of the active skin (re-read when the skin changes). */
export function useChartTheme() {
  const { skin } = useSkin()
  // eslint-disable-next-line react-hooks/exhaustive-deps -- the tokens change with the skin
  return useMemo(() => readTheme(), [skin])
}

/** Current time, updated every `ms` while `on`. */
export function useNow(ms: number, on = true) {
  const [now, setNow] = useState(() => Date.now())
  useEffect(() => {
    if (!on) return
    const t = setInterval(() => setNow(Date.now()), ms)
    return () => clearInterval(t)
  }, [ms, on])
  return now
}

/** `H:MM:SS` (days as `Nd H:MM:SS`). */
export function durationText(ms: number): string {
  const s = Math.max(0, Math.floor(ms / 1000))
  const d = Math.floor(s / 86400)
  const h = Math.floor((s % 86400) / 3600)
  const m = Math.floor((s % 3600) / 60)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d ? `${d}d ` : ''}${h}:${pad(m)}:${pad(s % 60)}`
}

/** Local date and time, minutes precision. */
export function dateTimeText(t: number): string {
  const d = new Date(t)
  const pad = (n: number) => String(n).padStart(2, '0')
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}`
}

/** Series legend: colour, label, latest value; click toggles the series, × removes it (live only). */
export function SeriesLegend({ series, hidden, onToggle, onRemove, removing }: {
  series: ChartSeries[]
  hidden: Set<number>
  onToggle: (id: number) => void
  onRemove?: (id: number) => void
  removing?: Set<number>
}) {
  if (!series.length) return <p className="bench-hint rec-legend__empty">No readings yet. Right-click a reading on an instrument panel to add it.</p>
  return (
    <ul className="rec-legend" aria-label="Series">
      {series.map((s) => {
        const off = hidden.has(s.id)
        return (
          <li key={s.id} className="rec-legend__item" data-off={off} data-removed={s.removed}>
            <button type="button" className="rec-legend__toggle" aria-pressed={!off} onClick={() => onToggle(s.id)}
              title={off ? 'Show on the chart' : 'Hide from the chart'}>
              <i className="rec-legend__swatch" style={{ background: s.color }} aria-hidden="true" />
              <span className="rec-legend__label">{s.label}</span>
              <span className="rec-legend__value">{valueText(latest(s.points), s.unit)}</span>
            </button>
            {onRemove && !s.removed && !s.locked && (
              <button type="button" className="bench-icon rec-legend__remove" aria-label={`Remove ${s.label}`} title="Stop recording this reading (its data is kept)"
                disabled={removing?.has(s.id)} aria-busy={removing?.has(s.id) || undefined} data-confirm={removing?.has(s.id) ? 'waiting' : undefined}
                onClick={() => onRemove(s.id)}>×</button>
            )}
          </li>
        )
      })}
    </ul>
  )
}

/** Two-step action (no browser dialog): the first press asks, the second does it. */
export function ConfirmButton({ label, confirmLabel, question, onConfirm, disabled, danger }: {
  label: string; confirmLabel: string; question: string; onConfirm: () => Promise<unknown> | void; disabled?: boolean; danger?: boolean
}) {
  const [asking, setAsking] = useState(false)
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  if (!asking) {
    return (
      <span className="rec-confirm">
        <button type="button" className="bench-btn" disabled={disabled} onClick={() => { setError(null); setAsking(true) }}>{label}</button>
        {error && <span className="kit-error" role="alert">{error}</span>}
      </span>
    )
  }
  return (
    <span className="rec-confirm rec-confirm--asking" role="group" aria-label={question}>
      <span className="rec-confirm__q">{question}</span>
      <button type="button" className={`bench-btn${danger ? ' rec-btn--danger' : ' bench-btn--primary'}`} disabled={busy} aria-busy={busy}
        onClick={async () => {
          setBusy(true)
          try { await onConfirm(); setAsking(false) } catch (e) { setError(e instanceof Error ? e.message : String(e)); setAsking(false) } finally { setBusy(false) }
        }}>{confirmLabel}</button>
      <button type="button" className="bench-btn" disabled={busy} onClick={() => setAsking(false)}
        onKeyDown={(e) => { if (e.key === 'Escape') { e.stopPropagation(); setAsking(false) } }}>Cancel</button>
    </span>
  )
}
