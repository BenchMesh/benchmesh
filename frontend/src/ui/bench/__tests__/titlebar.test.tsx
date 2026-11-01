import React from 'react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, act } from '@testing-library/react'
import { TitleBar } from '../TitleBar'

const UA = navigator.userAgent

function mockTauri(maximized = false) {
  let resized: (() => void) | null = null
  const win = {
    minimize: vi.fn(async () => {}),
    toggleMaximize: vi.fn(async () => {}),
    close: vi.fn(async () => {}),
    startDragging: vi.fn(async () => {}),
    isMaximized: vi.fn(async () => maximized),
    onResized: vi.fn(async (cb: () => void) => { resized = cb; return () => { resized = null } }),
  }
  ;(window as any).__TAURI__ = { window: { getCurrentWindow: () => win }, webviewWindow: {} }
  return { win, setMaximized: (m: boolean) => { maximized = m; resized?.() } }
}

function setUserAgent(ua: string) {
  Object.defineProperty(navigator, 'userAgent', { value: ua, configurable: true })
}

afterEach(() => {
  delete (window as any).__TAURI__
  setUserAgent(UA)
})

const bar = (props: Partial<React.ComponentProps<typeof TitleBar>> = {}) =>
  render(<TitleBar version={{ official: true, version: '0.1.8', display: 'BenchMesh 0.1.8' }} onAbout={() => {}} arranging={false} canArrange onArrange={() => {}} {...props} />)

describe('TitleBar', () => {
  it('shows the badges a distribution adds after the version, and they do not drag the window', () => {
    const { win } = mockTauri()
    const open = vi.fn()
    bar({ badges: <button type="button" className="bench-title__badge" onClick={open}>Update available</button> })
    const badge = screen.getByRole('button', { name: 'Update available' })
    expect(screen.getByRole('button', { name: 'v0.1.8' }).nextElementSibling).toBe(badge)
    fireEvent.mouseDown(badge, { button: 0, detail: 1 })
    fireEvent.click(badge)
    expect(open).toHaveBeenCalledOnce()
    expect(win.startDragging).not.toHaveBeenCalled()
  })

  it('shows the readouts a distribution adds on the right, just before Arrange; none: no readout area', () => {
    const { container } = bar({ readouts: <span data-testid="readout">22.8 °C</span> })
    const area = container.querySelector('.bench-title__readouts')!
    expect(area.contains(screen.getByTestId('readout'))).toBe(true)
    expect(area.nextElementSibling!.contains(screen.getByRole('button', { name: /Arrange/ }))).toBe(true)
    expect(area.previousElementSibling!.className).toBe('bench-title__drag')
    const { container: plain } = bar()
    expect(plain.querySelector('.bench-title__readouts')).toBeNull()
  })

  it('a community build: "v<version> community"; none while the version is unknown', () => {
    const { unmount } = bar({ version: { official: false, version: '0.1.16', display: 'BenchMesh 0.1.16 (community build)' } })
    expect(screen.getByRole('button', { name: 'v0.1.16 community' })).toBeInTheDocument()
    unmount()
    bar({ version: null })
    expect(screen.queryByRole('button', { name: /^v\d/ })).toBeNull()
  })

  it('shows brand, version (opens About) and Arrange; no window controls in a browser', () => {
    const onAbout = vi.fn()
    const onArrange = vi.fn()
    bar({ onAbout, onArrange })
    expect(screen.getByText('BenchMesh')).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'v0.1.8' }))
    expect(onAbout).toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Arrange' }))
    expect(onArrange).toHaveBeenCalled()
    expect(screen.queryByRole('group', { name: 'Window' })).toBeNull()
  })

  it('Arrange shows Done while arranging and is disabled without instruments', () => {
    const { rerender } = bar({ arranging: true })
    expect(screen.getByRole('button', { name: 'Done' })).toHaveAttribute('aria-pressed', 'true')
    // both words stay in the button (the hidden one keeps its width): readouts beside it do not move
    expect(screen.getByRole('button', { name: 'Done' }).querySelector('[data-on="false"]')!.textContent).toBe('Arrange')
    rerender(<TitleBar onAbout={() => {}} arranging={false} canArrange={false} onArrange={() => {}} />)
    expect(screen.getByRole('button', { name: 'Arrange' })).toBeDisabled()
  })

  it('desktop app on Windows / Linux: minimize, maximize / restore, close and drag the window', async () => {
    setUserAgent('Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 Edg/120')
    const { win, setMaximized } = mockTauri()
    bar()
    const controls = screen.getByRole('group', { name: 'Window' })
    expect(controls).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Minimize' }))
    expect(win.minimize).toHaveBeenCalled()
    fireEvent.click(await screen.findByRole('button', { name: 'Maximize' }))
    expect(win.toggleMaximize).toHaveBeenCalledTimes(1)
    await act(async () => { setMaximized(true) })
    expect(await screen.findByRole('button', { name: 'Restore' })).toBeInTheDocument()
    fireEvent.click(screen.getByRole('button', { name: 'Close' }))
    expect(win.close).toHaveBeenCalled()

    // the bar drags the window; a double press maximizes; buttons never drag
    const title = screen.getByText('BenchMesh')
    fireEvent.mouseDown(title, { button: 0, detail: 1 })
    expect(win.startDragging).toHaveBeenCalledTimes(1)
    fireEvent.mouseDown(title, { button: 0, detail: 2 })
    expect(win.toggleMaximize).toHaveBeenCalledTimes(2)
    fireEvent.mouseDown(screen.getByRole('button', { name: 'Arrange' }), { button: 0, detail: 1 })
    expect(win.startDragging).toHaveBeenCalledTimes(1)
  })

  it('desktop app on macOS keeps the native title bar: no own window controls, no dragging', () => {
    setUserAgent('Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15')
    const { win } = mockTauri()
    bar()
    expect(screen.queryByRole('group', { name: 'Window' })).toBeNull()
    fireEvent.mouseDown(screen.getByText('BenchMesh'), { button: 0, detail: 1 })
    expect(win.startDragging).not.toHaveBeenCalled()
  })
})
