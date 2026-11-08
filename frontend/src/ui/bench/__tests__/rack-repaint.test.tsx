import React from 'react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render } from '@testing-library/react'

const repaint = vi.hoisted(() => vi.fn())
vi.mock('../../kit', async (orig) => ({ ...(await orig<typeof import('../../kit')>()), repaintCanvases: repaint }))
vi.mock('../InstrumentPanel', () => ({ InstrumentPanel: () => <div /> }))

import { Rack } from '../Rack'
import { BenchLayout, move, panelsOf } from '../layout'

const instruments = [
  { id: 'dmm-1', name: 'DMM 1', classes: [{ class: 'DMM' }] },
  { id: 'dmm-2', name: 'DMM 2', classes: [{ class: 'DMM' }] },
] as never[]
const panels = panelsOf(instruments)
const layout: BenchLayout = { order: panels.map((p) => p.key), size: {}, hidden: [] } as unknown as BenchLayout

const rack = (l: BenchLayout) => (
  <Rack panels={panels} instruments={instruments} registry={{}} layout={l} onLayout={() => {}}
    arranging detached={[]} parked={new Set()} onDetach={() => {}} />
)

beforeEach(() => repaint.mockClear())

describe('Rack', () => {
  it('draws the canvases again when panels change places (the Linux webview leaves moved canvases blank)', () => {
    const view = render(rack(layout))
    repaint.mockClear()
    view.rerender(rack({ ...layout }))
    expect(repaint).not.toHaveBeenCalled()
    view.rerender(rack(move(layout, panels[1].key, 0)))
    expect(repaint).toHaveBeenCalledTimes(1)
  })
})
