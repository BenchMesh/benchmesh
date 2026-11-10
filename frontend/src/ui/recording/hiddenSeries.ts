/**
 * Series hidden from a recording's chart (legend toggle), remembered per recording and per browser
 * (`benchmesh:recording-hidden`), the same in the recorder dock and the recordings drawer: collapsing
 * the dock or closing the drawer keeps them hidden. Only the newest recordings (highest ids) are kept.
 * The sets handed out are never changed (a toggle makes a new one).
 */
import { useCallback, useSyncExternalStore } from 'react'

export const HIDDEN_STORAGE_KEY = 'benchmesh:recording-hidden'
export const HIDDEN_MAX_RECORDINGS = 50

const EMPTY: Set<number> = new Set()

function load(): Map<number, Set<number>> {
  const out = new Map<number, Set<number>>()
  try {
    const v = JSON.parse(localStorage.getItem(HIDDEN_STORAGE_KEY) ?? 'null')
    if (!v || typeof v !== 'object' || Array.isArray(v)) return out
    for (const [k, ids] of Object.entries(v)) {
      const id = Number(k)
      if (!Number.isInteger(id) || !Array.isArray(ids)) continue
      const set = new Set(ids.filter((i): i is number => typeof i === 'number'))
      if (set.size) out.set(id, set)
    }
  } catch { /* storage unavailable or damaged: nothing hidden */ }
  return out
}

let store: Map<number, Set<number>> | null = null
const listeners = new Set<() => void>()
const current = () => (store ??= load())

/** Forget the in-memory copy (tests: read storage again). */
export function resetHiddenSeries() { store = null }

export function hiddenSeries(recordingId: number): Set<number> {
  return current().get(recordingId) ?? EMPTY
}

export function toggleHidden(recordingId: number, seriesId: number) {
  const all = current()
  const ids = new Set(all.get(recordingId) ?? [])
  if (ids.has(seriesId)) ids.delete(seriesId); else ids.add(seriesId)
  if (ids.size) all.set(recordingId, ids); else all.delete(recordingId)
  const keep = [...all.keys()].sort((a, b) => b - a).slice(0, HIDDEN_MAX_RECORDINGS)
  for (const k of [...all.keys()]) if (!keep.includes(k)) all.delete(k)
  const json = Object.fromEntries([...all].map(([k, v]) => [k, [...v]]))
  try { localStorage.setItem(HIDDEN_STORAGE_KEY, JSON.stringify(json)) } catch { /* storage unavailable: this session only */ }
  listeners.forEach((l) => l())
}

function subscribe(l: () => void) {
  listeners.add(l)
  return () => { listeners.delete(l) }
}

/** The recording's hidden series and a toggle. */
export function useHiddenSeries(recordingId: number): [Set<number>, (seriesId: number) => void] {
  const hidden = useSyncExternalStore(subscribe, () => hiddenSeries(recordingId))
  const toggle = useCallback((seriesId: number) => toggleHidden(recordingId, seriesId), [recordingId])
  return [hidden, toggle]
}
