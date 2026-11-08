import React from 'react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render } from '@testing-library/react'
import { useCanvas, repaintCanvases } from '../useCanvas'

afterEach(() => vi.restoreAllMocks())

function Drawn({ draw }: { draw: () => void }) {
  const ref = useCanvas(draw, [])
  return <canvas ref={ref} />
}

describe('repaintCanvases', () => {
  it('draws every mounted kit canvas again, and no longer an unmounted one', () => {
    const ctx = { setTransform: vi.fn(), clearRect: vi.fn() } as unknown as CanvasRenderingContext2D
    vi.spyOn(HTMLCanvasElement.prototype, 'getContext').mockReturnValue(ctx as never)
    const a = vi.fn(), b = vi.fn()
    const one = render(<Drawn draw={a} />)
    render(<Drawn draw={b} />)
    a.mockClear(); b.mockClear()

    repaintCanvases()
    expect(a).toHaveBeenCalledTimes(1)
    expect(b).toHaveBeenCalledTimes(1)

    one.unmount()
    repaintCanvases()
    expect(a).toHaveBeenCalledTimes(1)
    expect(b).toHaveBeenCalledTimes(2)
  })
})
