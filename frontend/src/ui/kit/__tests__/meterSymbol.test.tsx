import React from 'react'
import { describe, it, expect } from 'vitest'
import { render, screen } from '@testing-library/react'
import { MeterSymbol, MARKS, symbolText, type SymbolPart } from '../meterSymbol'

describe('meter symbols', () => {
  it('read as plain text for names and fallbacks', () => {
    expect(symbolText([{ text: 'V' }, { mark: 'dc' }])).toBe('V⎓')
    expect(symbolText([{ mark: 'beep' }])).toBe('·)))')
    expect(symbolText([{ text: 'Ω' }, { text: '4W', small: true }])).toBe('Ω4W')
  })

  it('every mark fits its box: cap height tall, its width wide', () => {
    for (const [name, m] of Object.entries(MARKS)) {
      const nums = `${m.stroke} ${m.fill ?? ''}`.match(/-?\d*\.?\d+/g)!.map(Number)
      expect(nums.every((n) => n > -1.1 && n < 1.2), name).toBe(true)
      expect(m.w, name).toBeGreaterThan(0.4)
    }
  })

  it('renders letters in the font and marks as vectors sized in cap heights', () => {
    const parts: SymbolPart[] = [{ text: 'A' }, { mark: 'ac' }]
    const { container } = render(<MeterSymbol parts={parts} label="AC current" />)
    expect(screen.getByRole('img', { name: 'AC current' })).toHaveTextContent('A')
    const svg = container.querySelector('svg[data-mark="ac"]')!
    expect(svg.getAttribute('viewBox')).toBe(`0 0 ${MARKS.ac.w} 1`)
    expect((svg as SVGElement).style.width).toBe(`${MARKS.ac.w}cap`)
  })
})
