import React from 'react'
import { describe, it, expect, beforeEach } from 'vitest'
import { act, fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { RecordingsProvider } from '../RecordingsProvider'
import { DOCK_STORAGE_KEY, RecorderDock, elapsedOf, loadDockPrefs } from '../RecorderDock'
import { resetHiddenSeries } from '../hiddenSeries'
import { fakeClient, MockSocket, recording, series, T0 } from './fakes'

beforeEach(() => {
  localStorage.clear()
  resetHiddenSeries()
  MockSocket.sockets = []
  ;(globalThis as any).WebSocket = MockSocket
})

const renderDock = (client: ReturnType<typeof fakeClient>) =>
  render(<RecordingsProvider client={client}><RecorderDock /></RecordingsProvider>)

describe('RecorderDock', () => {
  it('lists active recordings in the bar', async () => {
    renderDock(fakeClient([
      recording({ id: 1, name: 'Ramp', series: [series(), series({ id: 12, field: 'IOUT' }), series({ id: 13, removed_at: T0 })] }),
      recording({ id: 2, name: 'Soak', state: 'paused', started_at: T0, paused_at: T0 + 65_000 }),
      recording({ id: 3, name: 'Done', state: 'stopped' }),
    ]))
    const bar = screen.getByRole('list', { name: 'Active recordings' })
    await waitFor(() => expect(within(bar).getAllByRole('listitem')).toHaveLength(2))
    expect(within(bar).getByRole('button', { name: /^Ramp, recording, .*, 2 series$/ })).toBeInTheDocument()
    expect(within(bar).getByRole('button', { name: 'Soak, paused, 0:01:05, 0 series' })).toBeInTheDocument()
    expect(within(bar).queryByText('Done')).toBeNull()
  })

  it('validates the new-recording form and shows server errors', async () => {
    const client = fakeClient([recording({ id: 1, name: 'Taken' })])
    renderDock(client)
    fireEvent.click(screen.getByRole('button', { name: '+ New recording' }))
    const form = screen.getByRole('form', { name: 'New recording' })
    const name = within(form).getByLabelText('Name')
    const interval = within(form).getByLabelText('Interval (s)')
    expect(name).toHaveFocus()

    fireEvent.click(within(form).getByRole('button', { name: 'Start recording' }))
    expect(within(form).getByText('Name is required')).toBeInTheDocument()
    expect(name).toHaveAttribute('aria-invalid', 'true')
    expect(client.create).not.toHaveBeenCalled()

    fireEvent.change(name, { target: { value: 'Taken' } })
    for (const bad of ['0', '1.5', 'abc', '3601', '']) {
      fireEvent.change(interval, { target: { value: bad } })
      expect(interval).toHaveAttribute('aria-invalid', 'true')
    }
    fireEvent.change(interval, { target: { value: '5' } })
    expect(interval).toHaveAttribute('aria-invalid', 'false')

    await act(async () => { fireEvent.click(within(form).getByRole('button', { name: 'Start recording' })) })
    expect(client.create).toHaveBeenCalledWith({ name: 'Taken', description: undefined, interval_s: 5 })
    expect(within(form).getByRole('alert')).toHaveTextContent("Recording name 'Taken' is taken")

    fireEvent.change(name, { target: { value: 'Fresh' } })
    fireEvent.change(within(form).getByLabelText('Description'), { target: { value: 'overnight' } })
    await act(async () => { fireEvent.click(within(form).getByRole('button', { name: 'Start recording' })) })
    expect(client.create).toHaveBeenLastCalledWith({ name: 'Fresh', description: 'overnight', interval_s: 5 })
    expect(screen.queryByRole('form', { name: 'New recording' })).toBeNull()
    // the new recording is selected and the dock opens on it
    expect(screen.getByRole('heading', { name: 'Fresh' })).toBeInTheDocument()
    expect(JSON.parse(localStorage.getItem(DOCK_STORAGE_KEY)!)).toMatchObject({ expanded: true, selected: 100 })
  })

  it('stop asks for confirmation in the dock', async () => {
    const client = fakeClient([recording({ id: 1, name: 'Ramp' })])
    localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify({ expanded: true, height: 240, selected: 1 }))
    renderDock(client)
    await screen.findByRole('heading', { name: 'Ramp' })
    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))
    expect(client.stop).not.toHaveBeenCalled()
    const ask = screen.getByRole('group', { name: /Stop for good/ })
    fireEvent.click(within(ask).getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('group', { name: /Stop for good/ })).toBeNull()
    expect(client.stop).not.toHaveBeenCalled()

    fireEvent.click(screen.getByRole('button', { name: 'Stop' }))
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Stop recording' })) })
    expect(client.stop).toHaveBeenCalledWith(1)
    // stopped: no longer an active recording
    await waitFor(() => expect(screen.getByText('No active recordings')).toBeInTheDocument())
  })

  it('pauses and resumes; the legend removes a series', async () => {
    const client = fakeClient([recording({ id: 1, name: 'Ramp', series: [series({ id: 11, label: 'Tenma · VOUT' })] })])
    localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify({ expanded: true, height: 240, selected: 1 }))
    renderDock(client)
    const pause = await screen.findByRole('button', { name: 'Pause' })
    await act(async () => { fireEvent.click(pause) })
    expect(client.pause).toHaveBeenCalledWith(1)
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Resume' })) })
    expect(client.resume).toHaveBeenCalledWith(1)
    // live values show in the legend
    act(() => MockSocket.last().emit({ type: 'samples', recording_id: 1, t: T0 + 1000, values: { 11: 4.93 } }))
    expect(screen.getByRole('button', { name: /Tenma · VOUT.*4\.93 V/ })).toBeInTheDocument()
    await act(async () => { fireEvent.click(screen.getByRole('button', { name: 'Remove Tenma · VOUT' })) })
    expect(client.removeSeries).toHaveBeenCalledWith(1, 11)
    await waitFor(() => expect(screen.queryByRole('button', { name: 'Remove Tenma · VOUT' })).toBeNull())
  })

  it('a hidden series stays hidden when the dock is collapsed and opened again', async () => {
    const client = fakeClient([recording({ id: 1, name: 'Ramp', series: [
      series({ id: 11, label: 'Tenma · VOUT' }), series({ id: 12, device_id: 'environment', label: 'Environment · Temperature', unit: '°C' }),
    ] })])
    localStorage.setItem(DOCK_STORAGE_KEY, JSON.stringify({ expanded: true, height: 240, selected: 1 }))
    renderDock(client)
    const toggle = () => screen.getByRole('button', { name: /^Environment · Temperature/ })
    fireEvent.click(await waitFor(toggle))
    expect(toggle()).toHaveAttribute('aria-pressed', 'false')
    fireEvent.click(screen.getByRole('button', { name: 'Collapse recorder' }))
    expect(screen.queryByRole('button', { name: /^Environment · Temperature/ })).toBeNull()
    fireEvent.click(screen.getByRole('button', { name: 'Expand recorder' }))
    expect(await waitFor(toggle)).toHaveAttribute('aria-pressed', 'false')
    expect(screen.getByRole('button', { name: /^Tenma · VOUT/ })).toHaveAttribute('aria-pressed', 'true')
  })

  it('collapses and expands, remembering it', async () => {
    renderDock(fakeClient([]))
    fireEvent.click(screen.getByRole('button', { name: 'Expand recorder' }))
    expect(screen.getByRole('button', { name: 'Collapse recorder' })).toHaveAttribute('aria-expanded', 'true')
    expect(loadDockPrefs().expanded).toBe(true)
    fireEvent.keyDown(screen.getByRole('separator', { name: 'Recorder height' }), { key: 'ArrowUp' })
    expect(loadDockPrefs().height).toBe(320)
    fireEvent.click(screen.getByRole('button', { name: 'Collapse recorder' }))
    expect(loadDockPrefs().expanded).toBe(false)
  })

  it('survives unreadable stored preferences', () => {
    localStorage.setItem(DOCK_STORAGE_KEY, '{nope')
    expect(loadDockPrefs()).toEqual({ expanded: false, height: 300, selected: null })
  })

  it('elapsed time stops at the pause', () => {
    expect(elapsedOf(recording({ state: 'paused', paused_at: T0 + 5000 }), T0 + 99_000)).toBe(5000)
    expect(elapsedOf(recording({ state: 'recording' }), T0 + 7000)).toBe(7000)
  })
})
