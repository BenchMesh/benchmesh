import { useState } from 'react'
import { describe, it, expect, vi } from 'vitest'
import { fireEvent, render, screen, within } from '@testing-library/react'
import { SelectPopups, listRows, placeList, takesOver, chooseOption } from '../SelectPopup'

function Host({ onPick = vi.fn(), onKey = vi.fn() }: { onPick?: (v: string) => void; onKey?: (k: string) => void }) {
  const [v, setV] = useState('b')
  return (
    <div onKeyDown={(e) => onKey(e.key)}>
      <label htmlFor="s">Letter</label>
      <select id="s" value={v} onChange={(e) => { setV(e.target.value); onPick(e.target.value) }}>
        <option value="a">Alpha</option>
        <option value="b">Beta</option>
        <option value="c" disabled>Gamma</option>
        <optgroup label="More">
          <option value="d">Delta</option>
        </optgroup>
      </select>
      <button type="button">Outside</button>
      <SelectPopups />
    </div>
  )
}

const opt = (name: RegExp) => within(screen.getByRole('listbox')).getByRole('option', { name })
const select = () => document.getElementById('s') as HTMLSelectElement

describe('SelectPopup', () => {
  it('a press opens the kit list instead of the system one, the current option focused', () => {
    render(<Host />)
    const down = fireEvent.mouseDown(select(), { button: 0 })
    expect(down).toBe(false) // default prevented: no system list
    const list = screen.getByRole('listbox', { name: 'Letter' })
    expect(list).toHaveClass('kit-menu')
    expect(within(screen.getByRole('listbox')).getAllByRole('option').map((o) => o.textContent)).toEqual(['Alpha', '✓Beta', 'Gamma', 'Delta'])
    expect(opt(/Beta/)).toHaveAttribute('aria-selected', 'true')
    expect(opt(/Beta/)).toHaveFocus()
    expect(opt(/Gamma/)).toHaveAttribute('aria-disabled', 'true')
    expect(screen.getByText('More')).toHaveClass('kit-select-list__group')
  })

  it('choosing sets the value through a change event and closes the list', () => {
    const onPick = vi.fn()
    render(<Host onPick={onPick} />)
    fireEvent.mouseDown(select(), { button: 0 })
    fireEvent.click(opt(/Delta/))
    expect(onPick).toHaveBeenCalledWith('d')
    expect(select().value).toBe('d')
    expect(screen.queryByRole('listbox')).toBeNull()
    expect(select()).toHaveFocus()
  })

  it('a disabled option cannot be chosen', () => {
    const onPick = vi.fn()
    render(<Host onPick={onPick} />)
    fireEvent.mouseDown(select(), { button: 0 })
    fireEvent.click(opt(/Gamma/))
    expect(onPick).not.toHaveBeenCalled()
    expect(screen.getByRole('listbox')).toBeInTheDocument()
  })

  it('keyboard: Space opens, arrows skip disabled options, Enter chooses', () => {
    const onPick = vi.fn()
    render(<Host onPick={onPick} />)
    select().focus()
    fireEvent.keyDown(select(), { key: ' ' })
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'ArrowDown' })
    expect(opt(/Delta/)).toHaveFocus()
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Home' })
    expect(opt(/Alpha/)).toHaveFocus()
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'd' })
    expect(opt(/Delta/)).toHaveFocus()
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Enter' })
    expect(onPick).toHaveBeenCalledWith('d')
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('Escape closes the list only (not the drawer around it); a press outside closes it too', () => {
    const onKey = vi.fn()
    render(<Host onKey={onKey} />)
    fireEvent.mouseDown(select(), { button: 0 })
    fireEvent.keyDown(screen.getByRole('listbox'), { key: 'Escape' })
    expect(screen.queryByRole('listbox')).toBeNull()
    expect(onKey).not.toHaveBeenCalledWith('Escape')
    expect(select()).toHaveFocus()
    fireEvent.mouseDown(select(), { button: 0 })
    fireEvent.pointerDown(screen.getByText('Outside'))
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('a second press on the select closes the list', () => {
    render(<Host />)
    fireEvent.mouseDown(select(), { button: 0 })
    fireEvent.pointerDown(select())
    fireEvent.mouseDown(select(), { button: 0 })
    expect(screen.queryByRole('listbox')).toBeNull()
  })

  it('leaves multiple, list-box and disabled selects to the browser', () => {
    const multi = document.createElement('select'); multi.multiple = true
    const box = document.createElement('select'); box.size = 4
    const off = document.createElement('select'); off.disabled = true
    expect([multi, box, off].map(takesOver)).toEqual([false, false, false])
    expect(takesOver(document.createElement('select'))).toBe(true)
    expect(takesOver(document.createElement('input'))).toBe(false)
  })

  it('rows leave hidden options out and keep group labels', () => {
    const sel = document.createElement('select')
    sel.innerHTML = '<option value="x" hidden>X</option><option value="y">Y</option><optgroup label="G" disabled><option>Z</option></optgroup>'
    expect(listRows(sel)).toEqual([
      { kind: 'option', index: 1, label: 'Y', disabled: false, selected: false },
      { kind: 'group', label: 'G' },
      { kind: 'option', index: 2, label: 'Z', disabled: true, selected: false },
    ])
  })

  it('places the list below, above when there is more room there, inside the viewport', () => {
    expect(placeList({ left: 10, top: 100, bottom: 130, width: 200 }, 220, 200, 800, 600)).toEqual({ left: 10, top: 132, maxHeight: 320 })
    const up = placeList({ left: 700, top: 500, bottom: 530, width: 200 }, 220, 300, 800, 600)
    expect(up.left).toBe(800 - 220 - 4)
    expect(up.top).toBe(500 - 300 - 2)
    expect(up.maxHeight).toBe(320)
  })

  it('chooseOption fires change only when the value changes', () => {
    const sel = document.createElement('select')
    sel.innerHTML = '<option>a</option><option>b</option>'
    const onChange = vi.fn()
    sel.addEventListener('change', onChange)
    chooseOption(sel, 0)
    expect(onChange).not.toHaveBeenCalled()
    chooseOption(sel, 1)
    expect(onChange).toHaveBeenCalledTimes(1)
    expect(sel.value).toBe('b')
  })
})
