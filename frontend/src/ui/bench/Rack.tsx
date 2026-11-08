import React, { useEffect, useLayoutEffect, useRef, useState } from 'react'
import { Instrument } from '../types'
import { InstrumentPanel } from './InstrumentPanel'
import { BenchLayout, columnsFor, hide, move, PanelRef, setSize, show, spanFor, toggleFold, toggleStacked } from './layout'
import { getClassDescription } from '../instrumentClasses'
import { repaintCanvases } from '../kit'

/** Columns that fit the rack's current width. */
function useColumns(ref: React.RefObject<HTMLElement>) {
  const [columns, setColumns] = useState(3)
  useEffect(() => {
    const el = ref.current
    if (!el) return
    const update = () => setColumns(columnsFor(el.clientWidth))
    update()
    if (typeof ResizeObserver === 'undefined') return
    const ro = new ResizeObserver(update)
    ro.observe(el)
    return () => ro.disconnect()
  }, [ref])
  return columns
}

/**
 * The bench: instrument panels in a responsive grid, in the user's order and widths.
 * In arrange mode panels can be dragged (or moved with ◀ ▶), resized, hidden and, with more than
 * one channel, set to show their channels side by side or stacked.
 */
export function Rack({ panels, instruments, registry, layout, onLayout, arranging, detached, parked, onDetach }: {
  panels: PanelRef[]
  instruments: Instrument[]
  registry: any
  layout: BenchLayout
  onLayout: (l: BenchLayout) => void
  arranging: boolean
  /** Panels shown in their own window: left out of the rack (not rendered at all). */
  detached: string[]
  /** Parked instruments: their panels wait in the parking strip and keep their place here. */
  parked: Set<string>
  onDetach: (key: string, title: string, size: { width: number; height: number }) => void
}) {
  const ref = useRef<HTMLDivElement>(null)
  const columns = useColumns(ref)
  const [dragging, setDragging] = useState<string | null>(null)
  const [target, setTarget] = useState<string | null>(null)
  const byKey = new Map(panels.map((p) => [p.key, p]))
  const visible = layout.order.filter((k) => byKey.has(k) && !layout.hidden.includes(k) && !detached.includes(k) && !parked.has(byKey.get(k)!.instrumentId))
  // reordering moves panels in the document: the Linux webview then shows their canvases blank
  // (a multimeter's dial) until drawn again
  const order = visible.join('|')
  useLayoutEffect(() => { repaintCanvases() }, [order])
  const hidden = layout.hidden.filter((k) => byKey.has(k))
  const label = (k: string) => {
    const p = byKey.get(k)!
    const ins = instruments.find((i) => i.id === p.instrumentId)
    return `${getClassDescription(p.klass)} · ${ins?.name ?? p.instrumentId}`
  }

  return (
    <>
      {arranging && (
        <div className="bench-shelf" aria-label="Hidden panels">
          <span className="kit-label">Arrange: drag panels by their header, set widths, put channels side by side or stacked, hide what you don't need.</span>
          {hidden.map((k) => (
            <button key={k} type="button" className="bench-chip" onClick={() => onLayout(show(layout, k))}>+ {label(k)}</button>
          ))}
        </div>
      )}
      <div ref={ref} className={`bench-rack${arranging ? ' bench-rack--arranging' : ''}`} style={{ gridTemplateColumns: `repeat(${columns}, minmax(0, 1fr))` }}>
        {visible.map((k, i) => {
          const p = byKey.get(k)!
          const instrument = instruments.find((x) => x.id === p.instrumentId)
          if (!instrument) return null
          const size = layout.size[k] ?? 's'
          return (
            <div
              key={k}
              className={`bench-slot${dragging === k ? ' bench-slot--dragging' : ''}${target === k && dragging !== k ? ' bench-slot--target' : ''}`}
              style={{ gridColumn: `span ${spanFor(size, columns)}` }}
              draggable={arranging}
              onDragStart={(e) => { setDragging(k); e.dataTransfer.effectAllowed = 'move'; e.dataTransfer.setData('text/plain', k) }}
              onDragEnd={() => { setDragging(null); setTarget(null) }}
              onDragOver={(e) => {
                if (!arranging) return
                e.preventDefault()
                e.dataTransfer.dropEffect = 'move'
                if (target !== k) setTarget(k)
              }}
              onDrop={(e) => {
                e.preventDefault()
                const from = e.dataTransfer.getData('text/plain') || dragging
                if (from && from !== k && byKey.has(from)) onLayout(move(layout, from, layout.order.indexOf(k)))
                setDragging(null)
                setTarget(null)
              }}
            >
              <InstrumentPanel
                instrument={instrument}
                klass={p.klass}
                registry={registry}
                stacked={(layout.stacked ?? []).includes(k)}
                win={{
                  folded: (layout.folded ?? []).includes(k),
                  onFold: () => onLayout(toggleFold(layout, k)),
                  onDetach: (size) => onDetach(k, label(k), size),
                }}
                arrange={arranging ? {
                  size,
                  onSize: (s) => onLayout(setSize(layout, k, s)),
                  onHide: () => onLayout(hide(layout, k)),
                  onStack: () => onLayout(toggleStacked(layout, k)),
                  onMove: (d) => {
                    const target = visible[i + d]
                    if (target) onLayout(move(layout, k, layout.order.indexOf(target)))
                  },
                } : undefined}
              />
            </div>
          )
        })}
      </div>
    </>
  )
}
