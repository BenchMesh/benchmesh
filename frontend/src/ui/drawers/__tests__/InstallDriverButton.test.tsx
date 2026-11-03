import React from 'react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, fireEvent, waitFor } from '@testing-library/react'
import { InstallDriverButton, installDriver, REENUMERATE_MS } from '../InstallDriverButton'

function reply(status: number, body: unknown) {
  return Promise.resolve(new Response(JSON.stringify(body), { status, headers: { 'Content-Type': 'application/json' } }))
}

afterEach(() => { vi.restoreAllMocks(); vi.useRealTimers() })

describe('installDriver', () => {
  it('posts the device path', async () => {
    const fetchMock = vi.spyOn(globalThis, 'fetch').mockImplementation(() => reply(200, { device: 'usb:1ab1:044d:X', installed: true }))
    await installDriver('/api', 'usb:1ab1:044d:X')
    const [url, init] = fetchMock.mock.calls[0]
    expect(url).toBe('/api/usbtmc-devices/driver')
    expect(init?.method).toBe('POST')
    expect(JSON.parse(String(init?.body))).toEqual({ device: 'usb:1ab1:044d:X' })
  })

  it('throws the server detail', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => reply(403, { detail: 'the administrator approval was declined' }))
    await expect(installDriver('/api', 'usb:1ab1:044d:X')).rejects.toThrow('approval was declined')
  })
})

describe('InstallDriverButton', () => {
  it('waits for the approval, then refreshes once Windows re-enumerated the instrument', async () => {
    let answer: (r: Response) => void = () => {}
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => new Promise<Response>((res) => { answer = res }))
    const onInstalled = vi.fn()
    render(<InstallDriverButton apiBase="/api" device="usb:1ab1:044d:X" onInstalled={onInstalled} />)
    vi.useFakeTimers({ shouldAdvanceTime: true })
    fireEvent.click(screen.getByRole('button', { name: 'Install driver' }))
    expect(screen.getByRole('button').textContent).toBe('Waiting for Windows approval…')
    expect(screen.getByRole('button')).toBeDisabled()
    answer(new Response(JSON.stringify({ installed: true }), { status: 200 }))
    await waitFor(() => expect(screen.getByRole('button').textContent).toBe('✓ Driver installed'))
    expect(onInstalled).not.toHaveBeenCalled()
    vi.advanceTimersByTime(REENUMERATE_MS)
    expect(onInstalled).toHaveBeenCalledOnce()
  })

  it('shows why it failed and can be tried again', async () => {
    vi.spyOn(globalThis, 'fetch').mockImplementation(() => reply(403, { detail: 'the administrator approval was declined; the driver was not installed' }))
    const onInstalled = vi.fn()
    render(<InstallDriverButton apiBase="/api" device="usb:1ab1:044d:X" onInstalled={onInstalled} />)
    fireEvent.click(screen.getByRole('button', { name: 'Install driver' }))
    await screen.findByText(/approval was declined/)
    expect(screen.getByRole('button', { name: 'Install driver' })).not.toBeDisabled()
    expect(onInstalled).not.toHaveBeenCalled()
  })

  it('runs the install of another device', async () => {
    const install = vi.fn(() => Promise.resolve())
    render(<InstallDriverButton install={install} title="Give the adapter WinUSB" onInstalled={() => {}} />)
    expect(screen.getByRole('button', { name: 'Install driver' }).title).toBe('Give the adapter WinUSB')
    fireEvent.click(screen.getByRole('button', { name: 'Install driver' }))
    await waitFor(() => expect(screen.getByRole('button').textContent).toBe('✓ Driver installed'))
    expect(install).toHaveBeenCalledOnce()
  })
})
