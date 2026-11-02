import React from 'react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, act, waitFor } from '@testing-library/react'
import { DigitEditor } from '../DigitEditor'
import { ArmSwitch } from '../ArmSwitch'
import { Annunciators, MicroSwitch, Selector } from '../controls'
import { Stepper } from '../Stepper'

const spec = { min: 0, max: 30, intDigits: 2, decimals: 2 }

describe('DigitEditor', () => {
  it('steps the selected digit with the arrow keys and applies on Enter', async () => {
    const onCommit = vi.fn().mockResolvedValue(undefined)
    render(<DigitEditor label="Voltage setpoint" value={4.94} spec={spec} unit="V" onCommit={onCommit} />)
    const ed = screen.getByRole('spinbutton', { name: 'Voltage setpoint' })
    fireEvent.focus(ed)
    // the ones digit is selected first
    fireEvent.keyDown(ed, { key: 'ArrowUp' })
    expect(ed).toHaveAttribute('aria-valuenow', '5.94')
    expect(ed.className).toContain('kit-digits--pending')
    expect(onCommit).not.toHaveBeenCalled()
    fireEvent.keyDown(ed, { key: 'Enter' })
    await waitFor(() => expect(onCommit).toHaveBeenCalledWith(5.94))
    await waitFor(() => expect(ed.className).not.toContain('kit-digits--pending'))
  })

  it('selects a digit by clicking, types digits, and Escape reverts', () => {
    const onCommit = vi.fn()
    render(<DigitEditor label="V" value={4.94} spec={spec} unit="V" onCommit={onCommit} />)
    const ed = screen.getByRole('spinbutton')
    fireEvent.click(screen.getAllByTestId('digit')[0])
    fireEvent.keyDown(ed, { key: '1' })
    expect(ed).toHaveAttribute('aria-valuenow', '14.94')
    fireEvent.keyDown(ed, { key: 'Escape' })
    expect(ed).toHaveAttribute('aria-valuenow', '4.94')
    expect(onCommit).not.toHaveBeenCalled()
  })

  it('follows the instrument value while not being edited', () => {
    const { rerender } = render(<DigitEditor label="V" value={4.94} spec={spec} unit="V" onCommit={vi.fn()} />)
    rerender(<DigitEditor label="V" value={12.5} spec={spec} unit="V" onCommit={vi.fn()} />)
    expect(screen.getByRole('spinbutton')).toHaveAttribute('aria-valuenow', '12.5')
  })

  it('keeps the draft and shows the error when applying fails', async () => {
    const onCommit = vi.fn().mockRejectedValue(new Error('Device requires remote mode'))
    render(<DigitEditor label="V" value={4.94} spec={spec} unit="V" onCommit={onCommit} />)
    const ed = screen.getByRole('spinbutton')
    fireEvent.focus(ed)
    fireEvent.keyDown(ed, { key: 'ArrowDown' })
    fireEvent.keyDown(ed, { key: 'Enter' })
    expect(await screen.findByText('Device requires remote mode')).toBeInTheDocument()
    expect(ed).toHaveAttribute('aria-valuenow', '3.94')
    expect(ed.className).toContain('kit-digits--error')
  })

  it('does nothing while disabled', () => {
    render(<DigitEditor label="V" value={4.94} spec={spec} unit="V" onCommit={vi.fn()} disabled />)
    const ed = screen.getByRole('spinbutton')
    fireEvent.keyDown(ed, { key: 'ArrowUp' })
    expect(ed).toHaveAttribute('aria-valuenow', '4.94')
  })
})

describe('ArmSwitch', () => {
  afterEach(() => vi.useRealTimers())

  it('needs two presses to switch on, one to switch off', async () => {
    const onChange = vi.fn().mockResolvedValue(undefined)
    const { rerender } = render(<ArmSwitch label="Output" on={false} onChange={onChange} />)
    const btn = screen.getByRole('button', { name: /Output/ })
    fireEvent.click(btn)
    expect(btn).toHaveAttribute('data-state', 'armed')
    expect(onChange).not.toHaveBeenCalled()
    fireEvent.click(btn)
    await waitFor(() => expect(onChange).toHaveBeenCalledWith(true))
    rerender(<ArmSwitch label="Output" on={true} onChange={onChange} />)
    await waitFor(() => expect(btn).toHaveAttribute('data-state', 'on'))
    fireEvent.click(btn)
    await waitFor(() => expect(onChange).toHaveBeenLastCalledWith(false))
  })

  it('disarms after 3 seconds', () => {
    vi.useFakeTimers()
    render(<ArmSwitch label="Input" on={false} onChange={vi.fn()} />)
    const btn = screen.getByRole('button', { name: /Input/ })
    fireEvent.click(btn)
    expect(btn).toHaveAttribute('data-state', 'armed')
    act(() => { vi.advanceTimersByTime(3100) })
    expect(btn).toHaveAttribute('data-state', 'off')
  })
})

describe('waiting for the instrument to confirm a change', () => {
  afterEach(() => vi.useRealTimers())

  it('switch: pulses until the instrument reports the new state, then settles', async () => {
    const onChange = vi.fn().mockResolvedValue(undefined)
    const { rerender } = render(<ArmSwitch label="Output" on={true} onChange={onChange} />)
    const btn = screen.getByRole('button', { name: /Output/ })
    fireEvent.click(btn)
    await waitFor(() => expect(btn).toHaveAttribute('data-confirm', 'waiting'))
    expect(btn).toHaveTextContent('switching off…')
    rerender(<ArmSwitch label="Output" on={false} onChange={onChange} />)
    expect(btn).toHaveAttribute('data-confirm', 'idle')
  })

  it('switch: marks "not confirmed" when the instrument does not follow', async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true })
    const onChange = vi.fn().mockResolvedValue(undefined)
    render(<ArmSwitch label="Output" on={true} onChange={onChange} />)
    const btn = screen.getByRole('button', { name: /Output/ })
    fireEvent.click(btn)
    await waitFor(() => expect(btn).toHaveAttribute('data-confirm', 'waiting'))
    act(() => { vi.advanceTimersByTime(6100) })
    expect(btn).toHaveAttribute('data-confirm', 'unconfirmed')
    expect(btn).toHaveAttribute('title', expect.stringMatching(/Not confirmed/))
  })

  it('selector: the chosen option waits until it is reported; a failed request stops waiting', async () => {
    const { rerender } = render(<Selector label="Mode" value="CC" options={[{ value: 'CC', label: 'CC' }, { value: 'CV', label: 'CV' }]} onChange={vi.fn()} />)
    const cv = screen.getByRole('button', { name: 'CV' })
    fireEvent.click(cv)
    expect(cv).toHaveAttribute('data-confirm', 'waiting')
    expect(cv).toHaveAttribute('aria-pressed', 'false') // still the instrument's state
    rerender(<Selector label="Mode" value="CV" options={[{ value: 'CC', label: 'CC' }, { value: 'CV', label: 'CV' }]} onChange={vi.fn()} />)
    expect(cv).not.toHaveAttribute('data-confirm')
    expect(cv).toHaveAttribute('aria-pressed', 'true')

    const failing = vi.fn().mockResolvedValue(false)
    rerender(<Selector label="Mode" value="CV" options={[{ value: 'CC', label: 'CC' }, { value: 'CV', label: 'CV' }]} onChange={failing} />)
    const cc = screen.getByRole('button', { name: 'CC' })
    fireEvent.click(cc)
    await waitFor(() => expect(cc).not.toHaveAttribute('data-confirm'))
  })

  it('setpoint: shows the sent value, pulsing, until the instrument reports it', async () => {
    const onCommit = vi.fn().mockResolvedValue(undefined)
    const { rerender } = render(<DigitEditor label="V" value={4.94} spec={spec} unit="V" onCommit={onCommit} />)
    const ed = screen.getByRole('spinbutton')
    fireEvent.focus(ed)
    fireEvent.keyDown(ed, { key: 'ArrowUp' })
    fireEvent.keyDown(ed, { key: 'Enter' })
    await waitFor(() => expect(ed).toHaveAttribute('data-confirm', 'waiting'))
    expect(ed).toHaveAttribute('aria-valuenow', '5.94') // not the old 4.94
    rerender(<DigitEditor label="V" value={5.94} spec={spec} unit="V" onCommit={onCommit} />)
    expect(ed).toHaveAttribute('data-confirm', 'idle')
  })

  it('micro switch and stepper wait for the reported value', async () => {
    const { rerender } = render(<>
      <MicroSwitch label="Remote" checked={false} onChange={vi.fn()} />
      <Stepper label="Time/div" text="2 µs/div" value={2e-6} format={(v) => `${v * 1e6} µs/div`} onStep={() => 5e-6} />
    </>)
    const sw = screen.getByRole('switch', { name: /Remote/ })
    fireEvent.click(sw)
    await waitFor(() => expect(sw).toHaveAttribute('data-confirm', 'waiting'))
    const st = screen.getByRole('spinbutton', { name: 'Time/div' })
    fireEvent.click(screen.getByRole('button', { name: 'Time/div up' }))
    await waitFor(() => expect(st).toHaveAttribute('data-confirm', 'waiting'))
    expect(st).toHaveTextContent('5 µs/div') // the requested value, not the old one
    rerender(<>
      <MicroSwitch label="Remote" checked={true} onChange={vi.fn()} />
      <Stepper label="Time/div" text="5 µs/div" value={5e-6} onStep={() => 5e-6} />
    </>)
    expect(sw).toHaveAttribute('data-confirm', 'idle')
    expect(st).toHaveAttribute('data-confirm', 'idle')
  })
})

describe('Annunciators', () => {
  const scope = (sweep: string) => [
    { label: 'RUN', on: true },
    { label: sweep, on: true, title: 'Trigger sweep' },
    { label: 'NORM', on: true, title: 'Acquisition type' },
  ]
  it('replaces a flag whose text changes, also when it reads like another flag (sweep NORM, acquisition NORM)', () => {
    const { rerender } = render(<Annunciators items={scope('AUTO')} />)
    for (const s of ['NORM', 'SING', 'AUTO', 'NORM', 'SING']) rerender(<Annunciators items={scope(s)} />)
    expect(screen.getAllByRole('listitem').map((x) => x.textContent)).toEqual(['RUN', 'SING', 'NORM'])
  })
})
