import React, { useEffect, useState } from 'react'
import { useCanvas, token } from './useCanvas'
import { NOT_CONFIRMED, useConfirmation } from './confirm'
import { useFocusedWheel } from './useFocusedWheel'
import { drawSymbol, type SymbolPart } from './meterSymbol'

/** `symbol`: drawn instead of the label text (letters and marks on the font's cap height). */
export interface DialOption { value: string; label: string; title?: string; symbol?: SymbolPart[] }

const A0 = Math.PI * 0.8
const A1 = Math.PI * 2.2

function angleOf(i: number, n: number) {
  return n < 2 ? -Math.PI / 2 : A0 + ((A1 - A0) * i) / (n - 1)
}

/** Index of the option whose label position is nearest to the pointer angle. */
export function nearestOption(x: number, y: number, cx: number, cy: number, n: number): number {
  const a = Math.atan2(y - cy, x - cx)
  let best = 0
  let bestD = Infinity
  for (let i = 0; i < n; i++) {
    const d = Math.abs(Math.atan2(Math.sin(a - angleOf(i, n)), Math.cos(a - angleOf(i, n))))
    if (d < bestD) { bestD = d; best = i }
  }
  return best
}

/**
 * Function selector drawn as a rotary switch: labels around the edge, a ridged knob pointing
 * at the active option. Click a label, scroll while focused, or use arrow keys.
 */
export function RotaryDial({ options, value, onChange, label, disabled, busy }: {
  options: DialOption[]; value: string; onChange: (v: string) => void | Promise<boolean | void>; label: string; disabled?: boolean
  /** A change is being applied: greyed out and locked in its position. */
  busy?: boolean
}) {
  const n = options.length
  const idx = Math.max(0, options.findIndex((o) => o.value === value))
  // the requested function is marked (blinking) until the instrument reports it
  const confirm = useConfirmation(value)
  const targetIdx = confirm.target != null ? options.findIndex((o) => o.value === confirm.target) : -1
  const [blink, setBlink] = useState(true)
  useEffect(() => {
    if (confirm.state !== 'waiting') return
    const t = setInterval(() => setBlink((b) => !b), 550)
    return () => clearInterval(t)
  }, [confirm.state])
  // steady when nothing is pending
  const mark = confirm.state !== 'waiting' || blink
  const ref = useCanvas((ctx, w, h, el) => {
    const cx = w / 2, cy = h / 2, R = Math.min(w, h) / 2
    const ink = token(el, '--kit-ink'), ink2 = token(el, '--kit-ink-2'), ink3 = token(el, '--kit-ink-3')
    const accent = token(el, '--kit-accent'), ctl = token(el, '--kit-ctl'), hi = token(el, '--kit-ctl-hi'), edge = token(el, '--kit-panel-edge')
    const font = token(el, '--kit-font-num')
    options.forEach((o, i) => {
      const a = angleOf(i, n)
      ctx.fillStyle = i === idx ? accent : ink2
      drawSymbol(ctx, o.symbol ?? [{ text: o.label }], cx + Math.cos(a) * (R - 15), cy + Math.sin(a) * (R - 15), 12, font)
      if (i === targetIdx && mark) {
        ctx.fillStyle = confirm.state === 'unconfirmed' ? token(el, '--kit-alert') : accent
        ctx.beginPath()
        ctx.arc(cx + Math.cos(a) * (R - 28), cy + Math.sin(a) * (R - 28), 2.6, 0, 7)
        ctx.fill()
      }
      ctx.strokeStyle = ink3
      ctx.lineWidth = 1
      ctx.beginPath()
      ctx.moveTo(cx + Math.cos(a) * (R - 29), cy + Math.sin(a) * (R - 29))
      ctx.lineTo(cx + Math.cos(a) * (R - 34), cy + Math.sin(a) * (R - 34))
      ctx.stroke()
    })
    const kr = R - 40
    const grad = ctx.createRadialGradient(cx - kr * 0.25, cy - kr * 0.35, 4, cx, cy, kr)
    grad.addColorStop(0, hi)
    grad.addColorStop(1, ctl)
    ctx.fillStyle = grad
    ctx.beginPath()
    ctx.arc(cx, cy, kr, 0, 7)
    ctx.fill()
    ctx.strokeStyle = edge
    ctx.lineWidth = 1.5
    ctx.stroke()
    ctx.save()
    ctx.translate(cx, cy)
    ctx.rotate(angleOf(idx, n))
    ctx.fillStyle = hi
    ctx.fillRect(-kr * 0.95, -kr * 0.16, kr * 1.9, kr * 0.32)
    ctx.strokeStyle = edge
    ctx.strokeRect(-kr * 0.95, -kr * 0.16, kr * 1.9, kr * 0.32)
    ctx.fillStyle = disabled ? ink3 : ink
    ctx.beginPath()
    ctx.moveTo(kr * 0.95, 0)
    ctx.lineTo(kr * 0.62, -kr * 0.1)
    ctx.lineTo(kr * 0.62, kr * 0.1)
    ctx.closePath()
    ctx.fill()
    ctx.restore()
  }, [options.map((o) => o.symbol ? JSON.stringify(o.symbol) : o.label).join('|'), idx, disabled, targetIdx, mark, confirm.state])

  const pick = (i: number) => {
    if (disabled || !n) return
    const o = options[(i + n) % n]
    if (o.value === (confirm.target ?? value)) return
    confirm.begin(o.value)
    const r = onChange(o.value)
    if (r instanceof Promise) r.then((ok) => { if (ok === false) confirm.cancel() }, () => confirm.cancel())
  }
  // keep turning from the requested position, not the still-reported one
  const at = targetIdx >= 0 ? targetIdx : idx
  useFocusedWheel(ref, (dir) => pick(at - dir))

  return (
    <canvas
      ref={ref}
      className={`kit-canvas kit-dial${busy ? ' kit-dial--busy' : ''}`}
      tabIndex={disabled ? -1 : 0}
      role="slider"
      aria-label={label}
      aria-valuetext={options[idx]?.title ?? options[idx]?.label}
      aria-busy={confirm.state === 'waiting'}
      data-confirm={confirm.state}
      title={confirm.state === 'unconfirmed' ? NOT_CONFIRMED : undefined}
      aria-disabled={disabled || undefined}
      onClick={(e) => {
        const r = e.currentTarget.getBoundingClientRect()
        e.currentTarget.focus()
        pick(nearestOption(e.clientX - r.left, e.clientY - r.top, r.width / 2, r.height / 2, n))
      }}
      onKeyDown={(e) => {
        const d = ['ArrowRight', 'ArrowDown'].includes(e.key) ? 1 : ['ArrowLeft', 'ArrowUp'].includes(e.key) ? -1 : 0
        if (d) { e.preventDefault(); pick(at + d) }
      }}
    />
  )
}
