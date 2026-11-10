import { describe, it, expect, beforeEach } from 'vitest'
import { HIDDEN_MAX_RECORDINGS, HIDDEN_STORAGE_KEY, hiddenSeries, resetHiddenSeries, toggleHidden } from '../hiddenSeries'

beforeEach(() => {
  localStorage.clear()
  resetHiddenSeries()
})

describe('hidden series', () => {
  it('are remembered per recording across reloads', () => {
    toggleHidden(1, 11)
    toggleHidden(1, 12)
    toggleHidden(2, 11)
    resetHiddenSeries()
    expect([...hiddenSeries(1)]).toEqual([11, 12])
    expect([...hiddenSeries(2)]).toEqual([11])
    expect(hiddenSeries(3).size).toBe(0)
  })

  it('a second toggle shows the series again and forgets an empty recording', () => {
    toggleHidden(1, 11)
    toggleHidden(1, 11)
    expect(hiddenSeries(1).size).toBe(0)
    expect(JSON.parse(localStorage.getItem(HIDDEN_STORAGE_KEY)!)).toEqual({})
  })

  it('hands out a new set on every change', () => {
    const before = hiddenSeries(1)
    toggleHidden(1, 11)
    expect(hiddenSeries(1)).not.toBe(before)
    expect(before.size).toBe(0)
  })

  it('keeps only the newest recordings', () => {
    for (let id = 1; id <= HIDDEN_MAX_RECORDINGS + 5; id++) toggleHidden(id, 7)
    const kept = Object.keys(JSON.parse(localStorage.getItem(HIDDEN_STORAGE_KEY)!)).map(Number)
    expect(kept).toHaveLength(HIDDEN_MAX_RECORDINGS)
    expect(Math.min(...kept)).toBe(6)
  })

  it('survives unreadable storage', () => {
    for (const bad of ['{nope', '[1,2]', '{"x":[1],"3":"no","4":[1,"a",2]}']) {
      localStorage.setItem(HIDDEN_STORAGE_KEY, bad)
      resetHiddenSeries()
      expect(hiddenSeries(3).size).toBe(0)
    }
    expect([...hiddenSeries(4)]).toEqual([1, 2])
  })
})
