import { describe, it, expect } from 'vitest'
import { AXIS_W, axisLayout, buildOption, chartSeries, ChartTheme, latest, mergeDetail, withGaps } from '../chartModel'
import { recording, series, T0 } from './fakes'

const theme: ChartTheme = { ink: '#000', ink2: '#111', ink3: '#222', edge: '#333', panel: '#fff', font: 'mono', palette: ['#a', '#b', '#c'] }

describe('chart data shaping', () => {
  it('gives every series its own y-axis in its colour, alternating left/right and offset outwards', () => {
    expect(axisLayout(4)).toEqual([
      { position: 'left', offset: 0 }, { position: 'right', offset: 0 },
      { position: 'left', offset: AXIS_W }, { position: 'right', offset: AXIS_W },
    ])
    const s = [
      { id: 1, label: 'PSU · VOUT', unit: 'V', color: '#a', points: [[T0, 4.9], [T0 + 1000, 5]] as [number, number][], removed: false, locked: false },
      { id: 2, label: 'LOAD · VIN', unit: 'V', color: '#b', points: [[T0, 118], [T0 + 1000, 120]] as [number, number][], removed: false, locked: false },
      { id: 3, label: 'LOAD · IIN', unit: 'A', color: '#c', points: [], removed: false, locked: false },
    ]
    const o = buildOption({ series: s, theme })
    expect(o.yAxis).toHaveLength(3)
    expect(o.yAxis.map((a: any) => [a.position, a.offset, a.axisLine.lineStyle.color, a.name, a.scale])).toEqual([
      ['left', 0, '#a', 'V', true], ['right', 0, '#b', 'V', true], ['left', AXIS_W, '#c', 'A', true],
    ])
    expect(o.series.map((x: any) => [x.yAxisIndex, x.lineStyle.color, x.connectNulls])).toEqual([[0, '#a', false], [1, '#b', false], [2, '#c', false]])
    expect(o.grid.left).toBeGreaterThan(o.grid.right) // two axes left, one right
    expect(o.xAxis.type).toBe('time')
    expect(o.tooltip.trigger).toBe('axis')

    // a hidden series takes its axis with it
    const h = buildOption({ series: s, theme, hidden: new Set([1]) })
    expect(h.yAxis.map((a: any) => [a.position, a.axisLine.lineStyle.color])).toEqual([['left', '#b'], ['right', '#c']])
    expect(h.series.map((x: any) => x.yAxisIndex)).toEqual([0, 1])
  })

  it('the tooltip lists every shown series at the time', () => {
    const s = [
      { id: 1, label: 'A', unit: 'V', color: '#a', points: [[T0, 4.9]] as [number, number][], removed: false, locked: false },
      { id: 2, label: 'B', unit: 'A', color: '#b', points: [[T0, null]] as [number, null][], removed: false, locked: false },
    ]
    const html: string = buildOption({ series: s, theme }).tooltip.formatter([
      { axisValue: T0, seriesId: 's1', value: [T0, 4.9] }, { axisValue: T0, seriesId: 's2', value: [T0, null] },
    ])
    expect(html).toContain('4.9 V')
    expect(html).toContain('-')
  })

  it('live window and zoom go to the axis and data zoom', () => {
    const o = buildOption({ series: [], theme, min: 1, max: 2 })
    expect([o.xAxis.min, o.xAxis.max]).toEqual([1, 2])
    expect(o.dataZoom).toEqual([])
    const z = buildOption({ series: [], theme, zoom: { enabled: true, from: 10, to: 20 } })
    expect(z.dataZoom.map((d: any) => [d.type, d.startValue, d.endValue])).toEqual([['inside', 10, 20], ['slider', 10, 20]])
  })

  it('breaks lines where samples are missing', () => {
    const p = withGaps([0, 1000, 2000, 9000, 10000], [1, 2, 3, 4, 5], 1000)
    expect(p).toEqual([[0, 1], [1000, 2], [2000, 3], [5500, null], [9000, 4], [10000, 5]])
    expect(withGaps([0, 1000], [1, 2], 5000)).toHaveLength(2)
  })

  it('merges zoomed detail into the overview', () => {
    const overview = [{ id: 1, t: [0, 100, 200, 300], v: [0, 1, 2, 3] }]
    const detail = [{ id: 1, t: [100, 150, 200], v: [1, 1.5, 2] }]
    expect(mergeDetail(overview, detail, 100, 200)).toEqual([{ id: 1, t: [0, 100, 150, 200, 300], v: [0, 1, 1.5, 2, 3] }])
  })

  it('combines history and newer live ticks, colours by position in the recording', () => {
    const r = recording({ series: [series({ id: 11 }), series({ id: 12, field: 'IOUT', unit: 'A' }), series({ id: 13, removed_at: T0 })] })
    const data = { recording_id: 1, from: null, to: null, series: [{ id: 11, t: [T0, T0 + 1000], v: [1, 2] }] }
    const live = { t: [T0 + 1000, T0 + 2000], v: { 11: [2, 3], 12: [null, 0.5] } }
    const cs = chartSeries(r, data, live, theme.palette)
    expect(cs.map((s) => [s.id, s.color, s.points])).toEqual([
      [11, '#a', [[T0, 1], [T0 + 1000, 2], [T0 + 2000, 3]]],
      [12, '#b', [[T0 + 2000, 0.5]]],
    ]) // removed series without data are left out
    expect(latest(cs[0].points)).toBe(3)
    expect(latest([[0, 1], [1, null]])).toBe(1)
  })

  it('shows environmental units without an SI prefix', async () => {
    const { valueText, axisText } = await import('../chartModel')
    expect(valueText(1013.25, 'hPa')).toBe('1013.3 hPa')
    expect(valueText(22.86, '°C')).toBe('22.86 °C')
    expect(valueText(41.18, '%RH')).toBe('41.18 %RH')
    expect(axisText(1013.4, 'hPa')).toBe('1013.4')
    expect(axisText(22.5, '°C')).toBe('22.5')
    // every unit of the environmental settings is shown as chosen, never SI-prefixed
    expect(valueText(101325, 'Pa')).toBe('101325 Pa')
    expect(valueText(29.92, 'inHg')).toBe('29.92 inHg')
    expect(valueText(72.5, '°F')).toBe('72.5 °F')
    expect(valueText(1.01325, 'bar')).toBe('1.013 bar')
    expect(valueText(1500, 'V')).toBe('1.5 kV')
    expect(axisText(1500, 'V')).toBe('1.5 k')
  })
})

