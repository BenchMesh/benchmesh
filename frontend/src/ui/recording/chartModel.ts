/**
 * Chart data shaping (no chart library here, so it is cheap to test and import): series from
 * `/data` and the live buffer, gaps, zoom detail merging, and the ECharts option with one
 * y-axis per series in the series' colour (alternating left / right, offset outwards).
 */
import { siText } from '../kit'
import type { LiveBuffer } from './store'
import type { Recording, RecordingData, Series, SeriesData } from './types'

export type Point = [number, number | null]

export interface ChartSeries {
  id: number
  label: string
  unit: string
  color: string
  points: Point[]
  removed: boolean
  /** added by the server and kept in every recording (environmental readings): cannot be removed */
  locked: boolean
}

/** Colours and font of the chart, read from the skin's tokens. */
export interface ChartTheme {
  ink: string
  ink2: string
  ink3: string
  edge: string
  panel: string
  font: string
  palette: string[]
}

export const SERIES_TOKENS = 8
/** Width reserved per y-axis (labels + name). */
export const AXIS_W = 56
/** Live view window. */
export const LIVE_WINDOW_MS = 10 * 60 * 1000

/** Theme from the kit tokens on `el` (the root element by default). */
export function readTheme(el: Element = document.documentElement): ChartTheme {
  const cs = getComputedStyle(el)
  const tok = (n: string, fb: string) => cs.getPropertyValue(n).trim() || fb
  return {
    ink: tok('--kit-ink', '#1d231c'),
    ink2: tok('--kit-ink-2', '#4b5649'),
    ink3: tok('--kit-ink-3', '#7d8a79'),
    edge: tok('--kit-panel-edge', '#b7bfb1'),
    panel: tok('--kit-panel', '#e9ece4'),
    font: tok('--kit-font-num', 'monospace'),
    palette: Array.from({ length: SERIES_TOKENS }, (_, i) => tok(`--kit-series-${i + 1}`, ['#1c3d8c', '#a8322c', '#2f6b43', '#8a5a00', '#6b2f8c', '#0f6f73', '#b0477a', '#4b5649'][i])),
  }
}

/** A series keeps its colour for the life of the recording: by its position in the recording. */
export function seriesColor(recording: Recording, seriesId: number, palette: string[]): string {
  const i = Math.max(0, recording.series.findIndex((s) => s.id === seriesId))
  return palette[i % palette.length]
}

/** Points of one series with `null` breaks where samples are missing (longer than 3 intervals). */
export function withGaps(t: number[], v: (number | null)[], intervalMs: number): Point[] {
  const out: Point[] = []
  // decimated data is spaced wider than the interval: use the typical spacing
  const steps = t.slice(1).map((x, i) => x - t[i]).filter((d) => d > 0).sort((a, b) => a - b)
  const typical = steps.length ? steps[Math.floor(steps.length / 2)] : intervalMs
  const limit = 3 * Math.max(intervalMs, typical)
  for (let i = 0; i < t.length; i++) {
    if (i > 0 && t[i] - t[i - 1] > limit) out.push([Math.round((t[i] + t[i - 1]) / 2), null])
    out.push([t[i], v[i] ?? null])
  }
  return out
}

/**
 * Overview data with the detail of a zoomed range: outside [from, to] the overview's points,
 * inside it the detail's (re-queried at full resolution for the range).
 */
export function mergeDetail(overview: SeriesData[], detail: SeriesData[], from: number, to: number): SeriesData[] {
  const det = new Map(detail.map((s) => [s.id, s]))
  return overview.map((s) => {
    const d = det.get(s.id)
    if (!d) return s
    const t: number[] = []
    const v: (number | null)[] = []
    const push = (tt: number, vv: number | null) => { t.push(tt); v.push(vv) }
    s.t.forEach((tt, i) => { if (tt < from) push(tt, s.v[i]) })
    d.t.forEach((tt, i) => { if (tt >= from && tt <= to) push(tt, d.v[i]) })
    s.t.forEach((tt, i) => { if (tt > to) push(tt, s.v[i]) })
    return { id: s.id, t, v }
  })
}

/**
 * Chart series of a recording: history from `/data`, then live ticks newer than it.
 * Removed series are included when they have data (they stay part of the record).
 */
export function chartSeries(recording: Recording, data: RecordingData | null, live: LiveBuffer | undefined, palette: string[]): ChartSeries[] {
  const intervalMs = recording.interval_s * 1000
  const hist = new Map((data?.series ?? []).map((s) => [s.id, s]))
  const lastHist = Math.max(-Infinity, ...(data?.series ?? []).map((s) => (s.t.length ? s.t[s.t.length - 1] : -Infinity)))
  return recording.series.map((s: Series) => {
    const h = hist.get(s.id)
    const t = h ? [...h.t] : []
    const v: (number | null)[] = h ? [...h.v] : []
    const lv = live?.v[String(s.id)]
    if (live && lv) {
      live.t.forEach((tt, i) => { if (tt > lastHist && (t.length === 0 || tt > t[t.length - 1])) { t.push(tt); v.push(lv[i]) } })
    }
    return { id: s.id, label: s.label, unit: s.unit, color: seriesColor(recording, s.id, palette), points: withGaps(t, v, intervalMs), removed: s.removed_at != null, locked: s.locked === true }
  }).filter((c) => !c.removed || c.points.some((p) => p[1] != null))
}

/** Latest non-null value of a series. */
export function latest(points: Point[]): number | null {
  for (let i = points.length - 1; i >= 0; i--) if (points[i][1] != null) return points[i][1]
  return null
}

/** Axis placement: 1st left, 2nd right, 3rd left (further out), … */
export function axisLayout(n: number): { position: 'left' | 'right'; offset: number }[] {
  return Array.from({ length: n }, (_, i) => ({ position: i % 2 === 0 ? 'left' : 'right', offset: Math.floor(i / 2) * AXIS_W }))
}

/** Units shown without an SI prefix: already prefixed or not SI - the environmental readings in
 * every unit the sensor settings offer (1013.2 hPa, never 1.013 khPa; 101325 Pa as chosen). */
const PLAIN_UNITS = new Set(['%', '°C', '°F', 'K', '%RH', 'g/m³', 'hPa', 'mbar', 'kPa', 'Pa', 'bar', 'atm', 'mmHg', 'inHg', 'psi'])

/** Value text for legends and the tooltip. */
export function valueText(v: number | null | undefined, unit: string): string {
  if (!PLAIN_UNITS.has(unit) || v == null || !Number.isFinite(v)) return siText(v, unit, 4)
  return `${axisText(v, unit)} ${unit}`
}

/** Axis label of a series' value: the number only. */
export function axisText(v: number, unit: string): string {
  return PLAIN_UNITS.has(unit) ? String(Number(v.toPrecision(Math.max(4, Math.floor(Math.log10(Math.abs(v) || 1)) + 2)))) : siText(v, '', 4).trim()
}

const pad2 = (n: number) => String(n).padStart(2, '0')
/** Local time `HH:MM:SS` (with the date when `withDate`). */
export function timeText(t: number, withDate = false): string {
  const d = new Date(t)
  const hms = `${pad2(d.getHours())}:${pad2(d.getMinutes())}:${pad2(d.getSeconds())}`
  return withDate ? `${d.getFullYear()}-${pad2(d.getMonth() + 1)}-${pad2(d.getDate())} ${hms}` : hms
}

export interface OptionInput {
  series: ChartSeries[]
  theme: ChartTheme
  /** Hidden series (legend toggles): left out together with their axes. */
  hidden?: Set<number>
  /** Visible time range (live window); undefined = fit the data. */
  min?: number
  max?: number
  /** Zoom/pan with the wheel and a slider (history view); the range shown when set. */
  zoom?: { enabled: boolean; from?: number; to?: number }
}

/** The ECharts option (a plain object: the library is loaded only with the chart). */
export function buildOption({ series, theme, hidden, min, max, zoom }: OptionInput): Record<string, any> {
  const shown = series.filter((s) => !hidden?.has(s.id))
  const axes = axisLayout(shown.length)
  const nLeft = axes.filter((a) => a.position === 'left').length
  const nRight = axes.length - nLeft
  const text = { color: theme.ink2, fontFamily: theme.font, fontSize: 10 }
  return {
    animation: false,
    backgroundColor: 'transparent',
    textStyle: { fontFamily: theme.font, color: theme.ink2 },
    grid: {
      left: 6 + Math.max(1, nLeft) * AXIS_W,
      right: 14 + nRight * AXIS_W,
      top: 26,
      bottom: zoom?.enabled ? 64 : 26,
      containLabel: false,
    },
    tooltip: {
      trigger: 'axis',
      confine: true,
      backgroundColor: theme.panel,
      borderColor: theme.edge,
      textStyle: { color: theme.ink, fontFamily: theme.font, fontSize: 11 },
      axisPointer: { type: 'line', lineStyle: { color: theme.ink3 } },
      formatter: (params: any[]) => {
        const list = Array.isArray(params) ? params : [params]
        if (!list.length) return ''
        const t = list[0].axisValue as number
        const rows = shown.map((s) => {
          const p = list.find((x) => x.seriesId === `s${s.id}`)
          const v = p ? (p.value as Point)[1] : null
          return `<div style="display:flex;gap:8px;justify-content:space-between"><span><span style="display:inline-block;width:8px;height:8px;border-radius:2px;margin-right:6px;background:${s.color}"></span>${escapeHtml(s.label)}</span><b>${valueText(v, s.unit)}</b></div>`
        })
        return `<div style="margin-bottom:4px;opacity:.75">${timeText(t, true)}</div>${rows.join('')}`
      },
    },
    xAxis: {
      type: 'time',
      min,
      max,
      axisLine: { lineStyle: { color: theme.edge } },
      axisTick: { lineStyle: { color: theme.edge } },
      axisLabel: { ...text, hideOverlap: true, formatter: (t: number) => timeText(t) },
      splitLine: { show: true, lineStyle: { color: theme.edge, opacity: 0.45 } },
    },
    yAxis: shown.length ? shown.map((s, i) => ({
      type: 'value',
      scale: true,
      position: axes[i].position,
      offset: axes[i].offset,
      name: s.unit || undefined,
      nameGap: 8,
      nameTextStyle: { ...text, color: s.color, fontWeight: 'bold' },
      axisLine: { show: true, lineStyle: { color: s.color, width: 1.5 } },
      axisTick: { show: true, lineStyle: { color: s.color } },
      axisLabel: { ...text, color: s.color, formatter: (v: number) => axisText(v, s.unit) },
      splitLine: { show: i === 0, lineStyle: { color: theme.edge, opacity: 0.45 } },
    })) : [{ type: 'value', axisLabel: text, splitLine: { lineStyle: { color: theme.edge, opacity: 0.45 } } }],
    dataZoom: zoom?.enabled ? [
      { type: 'inside', xAxisIndex: 0, filterMode: 'none', startValue: zoom.from, endValue: zoom.to },
      {
        type: 'slider', xAxisIndex: 0, filterMode: 'none', startValue: zoom.from, endValue: zoom.to, height: 22, bottom: 8,
        borderColor: theme.edge, fillerColor: 'rgba(128,128,128,.18)', textStyle: text,
        handleStyle: { color: theme.panel, borderColor: theme.ink3 }, moveHandleStyle: { color: theme.ink3 },
        dataBackground: { lineStyle: { color: theme.ink3 }, areaStyle: { color: theme.ink3, opacity: 0.15 } },
        labelFormatter: (t: number) => timeText(t),
      },
    ] : [],
    series: shown.map((s, i) => ({
      id: `s${s.id}`,
      name: s.label,
      type: 'line',
      yAxisIndex: i,
      showSymbol: false,
      connectNulls: false,
      sampling: undefined,
      lineStyle: { width: 1.6, color: s.color },
      itemStyle: { color: s.color },
      emphasis: { disabled: true },
      data: s.points,
    })),
  }
}

function escapeHtml(s: string) {
  return s.replace(/[&<>"']/g, (c) => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]!))
}
