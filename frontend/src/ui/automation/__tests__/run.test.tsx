import React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { fireEvent, render, screen, waitFor, within } from '@testing-library/react'
import { AutomationDrawer, resetDraft } from '../AutomationDrawer'
import { RunIndicator, RunProvider } from '../RunProvider'
import type { RunStatus } from '../types'
import { flowDoc, routeFetch } from './fakes'

afterEach(() => { vi.restoreAllMocks(); resetDraft() })

const T = 1792445884311
const ev = (seq: number, msg: string, path = '', level = 'info') => ({ seq, t: T + seq * 100, level, path, msg })

const savedFlow = {
  'GET /automation/flows': () => ({ body: [{ id: 'ramp', name: 'Ramp', description: '', updated_at: null }] }),
  'GET /automation/flows/ramp': () => ({ body: flowDoc() }),
}

async function openRamp() {
  render(<RunProvider><RunIndicator onOpen={() => {}} /><AutomationDrawer /></RunProvider>)
  fireEvent.click(await screen.findByRole('button', { name: 'Ramp' }))
  const run = await screen.findByRole('button', { name: '▶ Run' })
  await waitFor(() => expect(run).not.toBeDisabled())
  return run
}

describe('run panel', () => {
  it('asks to arm a flow that energises, then resends with arm: true', async () => {
    const posts: any[] = []
    const running = { id: 1, flow: { id: 'ramp', name: 'Ramp' }, state: 'running', path: '', vars: { target: 5 }, events: [], last_seq: 0 }
    routeFetch({
      ...savedFlow,
      'GET /automation/run': () => ({ body: posts.some((p) => p.arm) ? running : { state: 'idle', events: [] } }),
      'POST /automation/run': (b) => {
        posts.push(b)
        if (!b.arm) return { status: 409, body: { detail: 'this flow can switch outputs on', energizes: [{ path: 'setup[0]', device: 'psu', class: 'PSU', channel: 1, op: 'output' }] } }
        return { status: 202, body: running }
      },
    })
    fireEvent.click(await openRamp())
    const dialog = await screen.findByRole('dialog', { name: 'Arm the run' })
    expect(within(dialog).getByText(/CH1 · output/)).toBeTruthy()
    expect(within(dialog).getByText('setup[0]')).toBeTruthy()
    expect(posts[0]).toEqual({ flow_id: 'ramp', params: { target: 5 } })
    fireEvent.click(within(dialog).getByRole('button', { name: /Arm and run/ }))
    await waitFor(() => expect(screen.queryByRole('dialog', { name: 'Arm the run' })).toBeNull())
    expect(posts[1]).toEqual({ flow_id: 'ramp', params: { target: 5 }, arm: true })
    expect(await screen.findByRole('button', { name: /■ Abort/ })).toBeTruthy()
  })

  it('cancel in the arm dialog sends nothing more; busy (409) is shown', async () => {
    let n = 0
    routeFetch({
      ...savedFlow,
      'POST /automation/run': () => (n++ === 0
        ? { status: 409, body: { detail: 'x', energizes: [{ path: 'steps[0]', device: 'psu', class: 'PSU', channel: 1, op: 'output' }] } }
        : { status: 409, body: { detail: 'a flow is already running - abort it first' } }),
    })
    const run = await openRamp()
    fireEvent.click(run)
    const dialog = await screen.findByRole('dialog')
    // only Cancel closes it: a press beside it or Escape does not
    fireEvent.click(document.querySelector('.bench-modal__scrim')!)
    fireEvent.keyDown(dialog, { key: 'Escape' })
    expect(screen.getByRole('dialog', { name: 'Arm the run' })).toBeTruthy()
    fireEvent.click(within(dialog).getByRole('button', { name: 'Cancel' }))
    expect(screen.queryByRole('dialog')).toBeNull()
    expect(n).toBe(1)
    fireEvent.click(run)
    expect(await screen.findByText(/already running/)).toBeTruthy()
  })

  it('polls the log with after=<last_seq>, highlights steps, continues a prompt and aborts', async () => {
    const statuses: RunStatus[] = [
      { id: 7, flow: { id: 'ramp', name: 'Ramp' }, state: 'running', path: 'steps[0]', vars: { target: 5 }, last_seq: 2, events: [ev(1, 'run started'), ev(2, 'set voltage = 5', 'steps[0]')] },
      { id: 7, flow: { id: 'ramp', name: 'Ramp' }, state: 'waiting', path: 'steps[1]', prompt: 'Connect the battery', vars: { target: 5, v: 4.99 }, last_seq: 3, events: [ev(3, 'VOUT = 4.99', 'steps[1]')] },
      { id: 7, flow: { id: 'ramp', name: 'Ramp' }, state: 'cleanup', path: 'cleanup[0]', vars: { target: 5, v: 4.99 }, last_seq: 4, events: [ev(4, 'run aborted', '', 'warn')] },
    ]
    let i = 0
    let current: RunStatus = { state: 'idle', events: [] }
    const gets: string[] = []
    const calls = routeFetch({
      ...savedFlow,
      'POST /automation/run': () => { current = statuses[0]; return { status: 202, body: { ...statuses[0], events: [] } } },
      'GET /automation/run': (_b, path) => {
        gets.push(path)
        if (current.state === 'idle') return { body: current }
        const s = statuses[Math.min(i, statuses.length - 1)]
        if (i < 2 || s.state === 'cleanup') i++
        return { body: s }
      },
      'POST /automation/run/continue': () => { i = 1; return { status: 204 } },
      'POST /automation/run/abort': () => { i = 2; return { status: 204 } },
    })
    fireEvent.click(await openRamp())
    expect(await screen.findByText('set voltage = 5')).toBeTruthy()
    // the editor shows which step runs
    await waitFor(() => expect(screen.getByRole('group', { name: /^Step steps\[0\]:/ }).dataset.running).toBe('true'))
    expect(await screen.findByText('Connect the battery', {}, { timeout: 3000 })).toBeTruthy()
    expect(screen.getByText('VOUT = 4.99')).toBeTruthy()
    expect(gets.some((p) => p === '/automation/run?after=2')).toBe(true)
    expect(within(screen.getByRole('table', { name: 'Variables' })).getByText('4.99')).toBeTruthy()
    // top-bar indicator while active
    expect(screen.getByRole('status', { name: 'Automation run' }).textContent).toMatch(/▶ Ramp - Waiting for you/)

    // a log path highlights its step
    fireEvent.click(within(screen.getByRole('list', { name: 'Run log' })).getByRole('button', { name: 'steps[1]' }))
    expect(screen.getByRole('group', { name: /^Step steps\[1\]:/ }).dataset.highlight).toBe('true')

    fireEvent.click(screen.getByRole('button', { name: 'Continue' }))
    await waitFor(() => expect(calls.some((c) => c.path === '/automation/run/continue')).toBe(true))

    // Escape in the run panel aborts
    fireEvent.keyDown(screen.getByRole('region', { name: 'Run' }), { key: 'Escape' })
    await waitFor(() => expect(calls.some((c) => c.path === '/automation/run/abort')).toBe(true))
    expect(await screen.findByText('run aborted', {}, { timeout: 3000 })).toBeTruthy()
    // during cleanup the abort button skips the cleanup too
    expect(screen.getByRole('button', { name: /■ Stop cleanup too/ })).toBeTruthy()
  })

  it('runs an unsaved document as {flow}', async () => {
    const posts: any[] = []
    routeFetch({ 'POST /automation/run': (b) => { posts.push(b); return { status: 422, body: { detail: 'parameter target = 99 is outside [0, 30]' } } } })
    render(<RunProvider><AutomationDrawer /></RunProvider>)
    fireEvent.change(await screen.findByLabelText('Add step at the start of steps'), { target: { value: 'wait' } })
    const run = screen.getByRole('button', { name: '▶ Run unsaved document' })
    await waitFor(() => expect(run).not.toBeDisabled())
    fireEvent.click(run)
    expect(await screen.findByText(/outside \[0, 30\]/)).toBeTruthy()
    expect(posts[0].flow.steps).toEqual([{ do: 'wait', seconds: 1 }])
    expect(posts[0].flow_id).toBeUndefined()
  })
})

describe('run indicator', () => {
  it('shows the active run with a Stop that aborts, and nothing when idle', async () => {
    let state: RunStatus = { id: 3, flow: { id: null, name: 'Soak' }, state: 'running', path: 'steps[0]', vars: {}, last_seq: 0, events: [] }
    const calls = routeFetch({
      'GET /automation/run': () => ({ body: state }),
      'POST /automation/run/abort': () => { state = { ...state, state: 'aborted' }; return { status: 204 } },
    })
    const onOpen = vi.fn()
    render(<RunProvider><RunIndicator onOpen={onOpen} /></RunProvider>)
    const ind = await screen.findByRole('status', { name: 'Automation run' })
    expect(ind.textContent).toMatch(/▶ Soak - Running/)
    fireEvent.click(within(ind).getByTitle('Open the run in Automation'))
    expect(onOpen).toHaveBeenCalled()
    fireEvent.click(screen.getByRole('button', { name: 'Stop the automation run' }))
    await waitFor(() => expect(screen.queryByRole('status', { name: 'Automation run' })).toBeNull())
    expect(calls.filter((c) => c.path === '/automation/run/abort')).toHaveLength(1)
  })
})
