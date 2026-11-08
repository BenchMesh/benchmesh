import React from 'react'

/**
 * Meter function symbols (V⎓, A~, continuity, diode, capacitor, ...) as letters of the kit font
 * plus vector marks drawn on the font's cap height, so a mark matches the letters beside it in
 * size, weight and baseline: on a canvas (`drawSymbol`, the dial) and in the page (`MeterSymbol`).
 */

/** A part of a symbol: letters of the kit font (`small`: a subscript such as 4W) or a mark. */
export type SymbolPart = { text: string; small?: boolean } | { mark: MarkName }
export type MarkName = 'ac' | 'dc' | 'beep' | 'diode' | 'cap' | 'temp'

interface Mark {
  /** width in cap heights */
  w: number
  /** stroked path, coordinates in cap heights: y 0 at the top of capitals, 1 on the baseline */
  stroke: string
  /** filled path */
  fill?: string
  lineCap: CanvasLineCap
  /** the same mark as plain text (accessible names, fallbacks) */
  text: string
}

/** Stroke width in cap heights: the bold kit font's stems. */
export const STROKE = 0.17

const dot = (cx: number, cy: number, r: number) => `M${cx - r} ${cy} a${r} ${r} 0 1 0 ${2 * r} 0 a${r} ${r} 0 1 0 ${-2 * r} 0 Z`
const f = (n: number) => Math.round(n * 1000) / 1000

/** Sound waves of the continuity beeper: a dot and three arcs around it. */
function beepArcs(cx: number, cy: number, radii: number[], deg: number): string {
  const a = (deg * Math.PI) / 180
  return radii.map((r) => `M${f(cx + r * Math.cos(a))} ${f(cy - r * Math.sin(a))} A${r} ${r} 0 0 1 ${f(cx + r * Math.cos(a))} ${f(cy + r * Math.sin(a))}`).join(' ')
}

export const MARKS: Record<MarkName, Mark> = {
  // sine wave
  ac: { w: 0.95, stroke: 'M0.06 0.52 C0.2 0.1 0.34 0.1 0.475 0.5 C0.61 0.9 0.75 0.9 0.89 0.48', lineCap: 'round', text: '~' },
  // IEC 60417-5031: a line over a dashed line
  dc: { w: 0.95, stroke: 'M0 0.32 H0.95 M0 0.74 H0.25 M0.35 0.74 H0.6 M0.7 0.74 H0.95', lineCap: 'butt', text: '⎓' },
  // a beeper's sound
  beep: { w: 0.98, stroke: beepArcs(0.17, 0.5, [0.34, 0.56, 0.78], 40), fill: dot(0.17, 0.5, 0.17), lineCap: 'round', text: '·)))' },
  // IEC 60617 diode: a triangle against a bar, on a line
  diode: { w: 1.1, stroke: 'M0 0.5 H1.1 M0.78 0.06 V0.94', fill: 'M0.3 0.08 L0.3 0.92 L0.76 0.5 Z', lineCap: 'butt', text: '→|' },
  // capacitor: two plates between leads
  cap: { w: 1.0, stroke: 'M0 0.5 H0.39 M0.61 0.5 H1 M0.39 0.04 V0.96 M0.61 0.04 V0.96', lineCap: 'butt', text: '⊣⊢' },
  // thermometer: a stem with a bulb and two ticks
  temp: { w: 0.62, stroke: 'M0.2 0.04 V0.66 M0.42 0.2 H0.6 M0.42 0.42 H0.6', fill: dot(0.2, 0.79, 0.19), lineCap: 'round', text: '°' },
}

/** Smaller letters (the 4W of four-wire resistance), relative to the font size. */
const SMALL = 0.66
/** A mark that is the whole symbol (continuity, diode, ...) is drawn this many cap heights tall,
 *  centred on the letters' middle, as meters print them; its strokes keep the letters' weight. */
export const ALONE = 1.4
const alone = (parts: SymbolPart[]) => parts.length === 1 && 'mark' in parts[0]
/** Space between parts, in cap heights. */
const GAP = 0.16

/** The symbol as plain text, e.g. `V⎓`: its accessible name and what tests compare. */
export function symbolText(parts: SymbolPart[]): string {
  return parts.map((p) => ('mark' in p ? MARKS[p.mark].text : p.text)).join('')
}

/**
 * Draw a symbol centred on (cx, cy) in the canvas's current fill colour: letters in `font`
 * (`size` px, `weight`), marks scaled to the font's cap height.
 */
export function drawSymbol(ctx: CanvasRenderingContext2D, parts: SymbolPart[], cx: number, cy: number, size: number, family: string, weight = 700) {
  const fontOf = (small?: boolean) => `${weight} ${small ? Math.round(size * SMALL * 10) / 10 : size}px ${family}`
  ctx.font = fontOf()
  const cap = ctx.measureText('H').actualBoundingBoxAscent || size * 0.7
  const k = alone(parts) ? ALONE : 1
  const widths = parts.map((p) => {
    if ('mark' in p) return MARKS[p.mark].w * cap * k
    ctx.font = fontOf(p.small)
    return ctx.measureText(p.text).width
  })
  const total = widths.reduce((a, b) => a + b, 0) + GAP * cap * (parts.length - 1)
  const base = cy + cap / 2
  let x = cx - total / 2
  ctx.save()
  ctx.textAlign = 'left'
  ctx.textBaseline = 'alphabetic'
  ctx.strokeStyle = ctx.fillStyle
  parts.forEach((p, i) => {
    if ('mark' in p) {
      const m = MARKS[p.mark]
      ctx.save()
      ctx.translate(x, base - cap / 2 - (cap * k) / 2)
      ctx.scale(cap * k, cap * k)
      ctx.lineWidth = STROKE / k
      ctx.lineCap = m.lineCap
      ctx.lineJoin = 'round'
      ctx.stroke(new Path2D(m.stroke))
      if (m.fill) ctx.fill(new Path2D(m.fill))
      ctx.restore()
    } else {
      ctx.font = fontOf(p.small)
      ctx.fillText(p.text, x, base)
    }
    x += widths[i] + GAP * cap
  })
  ctx.restore()
}

/** A symbol in the page, in the surrounding font and colour. */
export function MeterSymbol({ parts, label }: { parts: SymbolPart[]; label?: string }) {
  return (
    <span className={`kit-sym${alone(parts) ? ' kit-sym--alone' : ''}`} role="img" aria-label={label ?? symbolText(parts)}>
      {parts.map((p, i) => {
        if (!('mark' in p)) return <span key={i} className={p.small ? 'kit-sym__small' : undefined} aria-hidden>{p.text}</span>
        const m = MARKS[p.mark]
        const k = alone(parts) ? ALONE : 1
        return (
          <svg key={i} className="kit-sym__mark" viewBox={`0 0 ${m.w} 1`} style={{ width: `${m.w * k}cap` }} aria-hidden data-mark={p.mark}>
            <path d={m.stroke} fill="none" stroke="currentColor" strokeWidth={STROKE / k} strokeLinecap={m.lineCap} strokeLinejoin="round" />
            {m.fill && <path d={m.fill} fill="currentColor" />}
          </svg>
        )
      })}
    </span>
  )
}
