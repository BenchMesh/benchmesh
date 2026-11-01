/** The series legend: an environmental reading (locked) cannot be removed, an instrument reading can. */
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import { SeriesLegend } from '../charts'
import type { ChartSeries } from '../chartModel'

afterEach(cleanup)

const s = (id: number, label: string, locked: boolean): ChartSeries =>
  ({ id, label, unit: '', color: '#a', points: [[0, 1]], removed: false, locked })

describe('SeriesLegend', () => {
  it('offers no removal for a locked series', () => {
    const onRemove = vi.fn()
    render(<SeriesLegend series={[s(1, 'Tenma · VOUT', false), s(2, 'Environment · Temperature', true)]} hidden={new Set()} onToggle={() => {}} onRemove={onRemove} />)
    expect(screen.queryByRole('button', { name: 'Remove Environment · Temperature' })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Remove Tenma · VOUT' }))
    expect(onRemove).toHaveBeenCalledWith(1)
  })
})
