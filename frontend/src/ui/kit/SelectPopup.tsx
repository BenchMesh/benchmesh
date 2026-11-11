/**
 * Dropdown lists in the kit's skin. A native `<select>` opens a list the operating system draws
 * (on Linux a GTK menu in the desktop theme, light on a dark bench) and no CSS reaches it. This
 * module takes over opening every single-choice `<select>` of the window: a press, or Space /
 * Enter / Alt+↓ / F4 on the focused select, shows a `kit-menu` list under it instead. Choosing
 * sets the select's value and fires `input` and `change`, so the `<select>` stays the control:
 * its value, label, form and tests (`selectOption`, `fireEvent.change`) work as before.
 * Touch screens (coarse pointer) keep the system's picker, which suits them.
 * List keys: ↑/↓, Home/End, Page Up/Down, a letter jumps to the next option starting with it,
 * Enter/Space choose, Escape or Tab close; a press outside, scrolling, resizing close it.
 */
import { useEffect, useLayoutEffect, useRef, useState } from 'react'
import type React from 'react'
import { createRoot } from 'react-dom/client'

const MARGIN = 4
const MAX_HEIGHT = 320

export type ListRow =
  | { kind: 'group'; label: string }
  | { kind: 'option'; index: number; label: string; disabled: boolean; selected: boolean }

/** A select whose popup this module draws: one choice, a dropdown (no list box), enabled. */
export function takesOver(el: Element | null): el is HTMLSelectElement {
  return el instanceof HTMLSelectElement && !el.multiple && el.size <= 1 && !el.disabled
}

/** The rows of the list: options (hidden ones left out) with the labels of their groups. */
export function listRows(sel: HTMLSelectElement): ListRow[] {
  const rows: ListRow[] = []
  const addOption = (o: HTMLOptionElement, groupDisabled: boolean) => {
    if (o.hidden) return
    rows.push({ kind: 'option', index: o.index, label: o.label || o.text, disabled: o.disabled || groupDisabled, selected: o.selected })
  }
  for (const child of Array.from(sel.children)) {
    if (child instanceof HTMLOptGroupElement) {
      rows.push({ kind: 'group', label: child.label })
      for (const o of Array.from(child.children)) if (o instanceof HTMLOptionElement) addOption(o, child.disabled)
    } else if (child instanceof HTMLOptionElement) addOption(child, false)
  }
  return rows
}

/** Where the list goes: below the select, above it when there is more room there; clamped. */
export function placeList(r: { left: number; top: number; bottom: number; width: number }, listW: number, listH: number, vw: number, vh: number) {
  const below = vh - r.bottom - MARGIN
  const above = r.top - MARGIN
  const up = listH > below && above > below
  const maxHeight = Math.max(80, Math.min(MAX_HEIGHT, up ? above : below))
  const h = Math.min(listH, maxHeight)
  return {
    left: Math.max(MARGIN, Math.min(r.left, vw - listW - MARGIN)),
    top: up ? Math.max(MARGIN, r.top - h - 2) : r.bottom + 2,
    maxHeight,
  }
}

/** Sets a select's value the way the user would: React and other listeners see `change`. */
export function chooseOption(sel: HTMLSelectElement, index: number) {
  if (sel.selectedIndex === index) return
  sel.selectedIndex = index
  sel.dispatchEvent(new Event('input', { bubbles: true }))
  sel.dispatchEvent(new Event('change', { bubbles: true }))
}

const coarsePointer = () => typeof window.matchMedia === 'function' && window.matchMedia('(pointer: coarse)').matches

function SelectList({ sel, onClose }: { sel: HTMLSelectElement; onClose: () => void }) {
  const ref = useRef<HTMLDivElement>(null)
  const [rows] = useState(() => listRows(sel))
  const [pos, setPos] = useState<{ left: number; top: number; maxHeight: number } | null>(null)
  const closeRef = useRef(onClose)
  useLayoutEffect(() => { closeRef.current = onClose })
  const options = () => Array.from(ref.current?.querySelectorAll<HTMLElement>('[role="option"]:not([aria-disabled="true"])') ?? [])

  useLayoutEffect(() => {
    const el = ref.current
    if (!el) return
    const r = sel.getBoundingClientRect()
    setPos(placeList(r, el.offsetWidth, el.scrollHeight, window.innerWidth, window.innerHeight))
  }, [sel])

  // the chosen option takes focus and is scrolled into view; focus goes back to the select
  const focused = useRef(false)
  useEffect(() => {
    if (!pos || focused.current) return
    focused.current = true
    const items = options()
    const current = items.find((i) => i.getAttribute('aria-selected') === 'true') ?? items[0]
    current?.focus({ preventScroll: true })
    current?.scrollIntoView?.({ block: 'nearest' })
  }, [pos])

  useEffect(() => {
    const inside = (t: EventTarget | null) => t instanceof Node && (!!ref.current?.contains(t) || t === sel)
    const onDown = (e: Event) => { if (!inside(e.target)) closeRef.current() }
    const onScroll = (e: Event) => { if (!inside(e.target)) closeRef.current() }
    const onResize = () => closeRef.current()
    document.addEventListener('pointerdown', onDown, true)
    document.addEventListener('scroll', onScroll, true)
    window.addEventListener('resize', onResize)
    window.addEventListener('blur', onResize)
    return () => {
      document.removeEventListener('pointerdown', onDown, true)
      document.removeEventListener('scroll', onScroll, true)
      window.removeEventListener('resize', onResize)
      window.removeEventListener('blur', onResize)
    }
  }, [sel])

  const choose = (index: number) => {
    sel.focus({ preventScroll: true })
    chooseOption(sel, index)
    closeRef.current()
  }

  const onKeyDown = (e: React.KeyboardEvent) => {
    e.stopPropagation() // Escape closes the list, not the drawer it is in
    const els = options()
    const i = els.indexOf(document.activeElement as HTMLElement)
    const focusAt = (j: number) => {
      const el = els[Math.max(0, Math.min(els.length - 1, j))]
      el?.focus({ preventScroll: true })
      el?.scrollIntoView?.({ block: 'nearest' })
    }
    const page = Math.max(1, Math.floor((ref.current?.clientHeight ?? 0) / Math.max(1, els[0]?.offsetHeight ?? 1)) - 1)
    switch (e.key) {
      case 'ArrowDown': e.preventDefault(); focusAt(i + 1); break
      case 'ArrowUp': e.preventDefault(); focusAt(i < 0 ? 0 : i - 1); break
      case 'PageDown': e.preventDefault(); focusAt(i + page); break
      case 'PageUp': e.preventDefault(); focusAt(i - page); break
      case 'Home': e.preventDefault(); focusAt(0); break
      case 'End': e.preventDefault(); focusAt(els.length - 1); break
      case 'Escape': e.preventDefault(); sel.focus({ preventScroll: true }); onClose(); break
      case 'Tab': e.preventDefault(); sel.focus({ preventScroll: true }); onClose(); break
      case 'Enter':
      case ' ': {
        e.preventDefault()
        const index = Number((document.activeElement as HTMLElement | null)?.dataset.index)
        if (Number.isInteger(index)) choose(index)
        break
      }
      default:
        if (e.key.length === 1 && !e.ctrlKey && !e.metaKey && !e.altKey) {
          const k = e.key.toLowerCase()
          const order = [...els.slice(i + 1), ...els.slice(0, i + 1)]
          const hit = order.find((el) => (el.textContent ?? '').trim().toLowerCase().startsWith(k))
          if (hit) { e.preventDefault(); focusAt(els.indexOf(hit)) }
        }
    }
  }

  const style = getComputedStyle(sel)
  const label = sel.getAttribute('aria-label') ?? sel.labels?.[0]?.textContent?.trim() ?? undefined
  return (
    <div ref={ref} className="kit-menu kit-select-list" role="listbox" aria-label={label} onKeyDown={onKeyDown}
      onContextMenu={(e) => e.preventDefault()}
      style={{
        left: pos?.left ?? 0, top: pos?.top ?? 0, maxHeight: pos?.maxHeight ?? MAX_HEIGHT,
        minWidth: sel.getBoundingClientRect().width, visibility: pos ? undefined : 'hidden',
        fontFamily: style.fontFamily, fontSize: style.fontSize,
      }}>
      {rows.map((row, n) => row.kind === 'group'
        ? <div key={`g${n}`} className="kit-select-list__group" role="presentation">{row.label}</div>
        : (
          <div key={row.index} role="option" tabIndex={-1} data-index={row.index} className="kit-menu__item"
            aria-selected={row.selected} aria-disabled={row.disabled || undefined}
            onClick={() => { if (!row.disabled) choose(row.index) }}
            onMouseEnter={(e) => { if (!row.disabled) e.currentTarget.focus({ preventScroll: true }) }}>
            <span className="kit-menu__check" aria-hidden="true">{row.selected ? '✓' : ''}</span>
            <span className="kit-menu__label">{row.label}</span>
          </div>
        ))}
    </div>
  )
}

/** Owns the open list of the window (at most one). */
export function SelectPopups({ target = window }: { target?: Window }) {
  const [open, setOpen] = useState<HTMLSelectElement | null>(null)
  const [opened, setOpened] = useState(0) // a fresh list (rows, place) for every opening
  const openRef = useRef(open)
  useLayoutEffect(() => { openRef.current = open })

  useEffect(() => {
    const show = (sel: HTMLSelectElement) => {
      sel.focus({ preventScroll: true })
      setOpen((cur) => (cur === sel ? null : sel))
      setOpened((n) => n + 1)
    }
    const onMouseDown = (e: MouseEvent) => {
      const sel = e.target instanceof Element ? e.target.closest('select') : null
      if (e.button !== 0 || !takesOver(sel) || coarsePointer()) return
      e.preventDefault() // no system list
      show(sel)
    }
    const onKeyDown = (e: KeyboardEvent) => {
      const sel = e.target as Element | null
      if (!takesOver(sel) || openRef.current === sel) return
      const opens = e.key === ' ' || e.key === 'Enter' || e.key === 'F4' || (e.altKey && (e.key === 'ArrowDown' || e.key === 'ArrowUp'))
      if (!opens) return
      e.preventDefault()
      e.stopPropagation()
      show(sel)
    }
    target.addEventListener('mousedown', onMouseDown, true)
    target.addEventListener('keydown', onKeyDown, true)
    return () => {
      target.removeEventListener('mousedown', onMouseDown, true)
      target.removeEventListener('keydown', onKeyDown, true)
    }
  }, [target])

  // the select went away (its form was closed): so does its list
  useEffect(() => {
    if (!open) return
    const t = window.setInterval(() => { if (!open.isConnected || open.disabled) setOpen(null) }, 250)
    return () => window.clearInterval(t)
  }, [open])

  return open ? <SelectList key={opened} sel={open} onClose={() => setOpen(null)} /> : null
}

/** Mounts `SelectPopups` for a window, beside the app's own root. Returns the cleanup. */
export function installSelectPopups(doc: Document = document): () => void {
  const host = doc.createElement('div')
  host.className = 'kit-select-host'
  doc.body.appendChild(host)
  const root = createRoot(host)
  root.render(<SelectPopups target={doc.defaultView ?? window} />)
  return () => { root.unmount(); host.remove() }
}
