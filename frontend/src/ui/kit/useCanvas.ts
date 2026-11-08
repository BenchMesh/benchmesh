import { useEffect, useLayoutEffect, useRef } from 'react'
import { useSkin } from './skin'

/** The paint of every mounted kit canvas, for `repaintCanvases`. */
const painters = new Set<() => void>()

/**
 * Draw every mounted kit canvas again. Call after canvases were moved in the document (panels
 * reordered): WebKitGTK, the Linux app's webview, then stops showing a moved canvas although its
 * drawing is kept, and only drawing on it again brings the picture back.
 */
export function repaintCanvases() {
  painters.forEach((paint) => paint())
}

export type Draw = (ctx: CanvasRenderingContext2D, w: number, h: number, el: HTMLCanvasElement) => void

/** Kit colour token, read from the canvas element so the skin stays in CSS. */
export function token(el: Element, name: string): string {
  return getComputedStyle(el).getPropertyValue(name).trim()
}

/** True when the active skin lights its displays (`--kit-glow: 1`, LED segment). */
export function glows(el: Element): boolean {
  return Number(token(el, '--kit-glow')) > 0
}

/** Colour token of a per-quantity tone (`--kit-volt` ...), or the plain segment ink. */
export type Tone = 'volt' | 'amp' | 'watt' | 'meas'
export function toneVar(tone?: Tone): string {
  return tone ? `--kit-${tone}` : '--kit-seg-on'
}

/**
 * A canvas sized to its CSS box at device pixel ratio, redrawn when `deps` change and
 * when the box resizes or the skin changes (colours are tokens read at draw time).
 * `draw` gets CSS-pixel coordinates.
 */
export function useCanvas(draw: Draw, deps: unknown[]) {
  const { skin } = useSkin()
  const ref = useRef<HTMLCanvasElement>(null)
  const drawRef = useRef(draw)
  useLayoutEffect(() => { drawRef.current = draw })

  const paint = () => {
    const el = ref.current
    if (!el) return
    const ctx = el.getContext?.('2d')
    if (!ctx) return // jsdom
    const r = el.getBoundingClientRect()
    const dpr = window.devicePixelRatio || 1
    const w = Math.max(1, Math.round(r.width * dpr))
    const h = Math.max(1, Math.round(r.height * dpr))
    if (el.width !== w || el.height !== h) {
      el.width = w
      el.height = h
    }
    ctx.setTransform(dpr, 0, 0, dpr, 0, 0)
    // context state outlives a paint: reset what draws change (glow from a previous skin)
    ctx.shadowBlur = 0
    ctx.shadowColor = 'transparent'
    ctx.globalAlpha = 1
    ctx.clearRect(0, 0, r.width, r.height)
    drawRef.current(ctx, r.width, r.height, el)
  }

  // eslint-disable-next-line react-hooks/exhaustive-deps -- callers pass what their drawing depends on
  useLayoutEffect(paint, [...deps, skin])

  useEffect(() => {
    painters.add(paint)
    return () => { painters.delete(paint) }
  }, [])

  useEffect(() => {
    const el = ref.current
    if (!el || typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(() => paint())
    ro.observe(el)
    // Web fonts arrive after first paint; canvas text must be redrawn with them.
    document.fonts?.ready.then(() => paint()).catch(() => {})
    return () => ro.disconnect()
  }, [])

  return ref
}
