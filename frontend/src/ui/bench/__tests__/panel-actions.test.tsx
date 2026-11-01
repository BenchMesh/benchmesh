import React from 'react'
import { describe, it, expect, vi, beforeEach } from 'vitest'
import { render, screen } from '@testing-library/react'
import type { PanelActionProps } from '../../../extensions/contract'
import { InstrumentPanel } from '../InstrumentPanel'
import { RequestLogProvider } from '../../RequestLogContext'
import { Instrument } from '../../types'

const seen: PanelActionProps[] = []

vi.mock('../../../extensions', () => ({
  extensions: {
    root: [], systemGroups: [], aboutGroups: [], onlineGroups: [], topBarStatus: [], recordingViews: [], titleBadges: [], titleReadouts: [], sensorsGroups: [],
    panelActions: [(p: PanelActionProps) => { seen.push(p); return <button type="button" aria-label="Report a problem" /> }],
  },
}))

const tenma: Instrument = { id: 'tenma', name: 'Tenma', IDN: 'TENMA 72-2540 V2.1', classes: [{ class: 'PSU', channels: ['/instruments/PSU/tenma/1'] }] }
const registry = { tenma: { IDN: 'TENMA 72-2540 V2.1', PSU: { status_ch1: { OUTPUT: 'OFF' } } } }

beforeEach(() => {
  seen.length = 0
  global.fetch = vi.fn(async () => new Response('{}', { status: 200, headers: { 'Content-Type': 'application/json' } })) as any
})

const panel = (arrange?: any) => render(
  <RequestLogProvider>
    <InstrumentPanel instrument={tenma} klass="PSU" registry={registry} arrange={arrange} win={{ folded: false, onFold: vi.fn() }} />
  </RequestLogProvider>,
)

describe("a distribution's panel actions", () => {
  it('sit in the header before Park and know the instrument', () => {
    panel()
    const buttons = screen.getAllByRole('button').map((b) => b.getAttribute('aria-label'))
    expect(buttons.indexOf('Report a problem')).toBe(buttons.indexOf('Park instrument') - 1)
    expect(seen.at(-1)).toEqual({ instrumentId: 'tenma', klass: 'PSU', name: 'Tenma', online: expect.any(Boolean) })
  })

  it('are not shown while arranging', () => {
    panel({ size: 'M', onMove: vi.fn(), onSize: vi.fn(), onHide: vi.fn(), onStack: vi.fn() })
    expect(screen.queryByRole('button', { name: 'Report a problem' })).toBeNull()
  })
})
