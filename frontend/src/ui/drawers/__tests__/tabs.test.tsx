/** Instruments tabs: About always (last), Online and Sensors only when the distribution adds groups to them. */
import React from 'react'
import { afterEach, describe, expect, it, vi } from 'vitest'
import { cleanup, fireEvent, render, screen } from '@testing-library/react'
import type { GroupProps } from '../../../extensions/contract'
import * as system from '../../system/client'

const ext = vi.hoisted(() => ({ extensions: { root: [], systemGroups: [], aboutGroups: [] as unknown[], onlineGroups: [] as unknown[], sensorsGroups: [] as unknown[], topBarStatus: [], recordingViews: [], titleBadges: [], titleReadouts: [], panelActions: [], instrumentsSections: [], identityField: null } }))
vi.mock('../../../extensions', () => ext)

import { SettingsView } from '../SettingsView'

const version = (distribution: string) => {
  global.fetch = vi.fn(async (url: string) => new Response(JSON.stringify(String(url).endsWith('/version') ? { version: '0.1.17', distribution } : []), { status: 200, headers: { 'Content-Type': 'application/json' } })) as any
}

afterEach(() => { cleanup(); vi.restoreAllMocks(); ext.extensions.aboutGroups = []; ext.extensions.onlineGroups = []; ext.extensions.sensorsGroups = [] })

const tabs = () => screen.getAllByTestId(/^settings-tab-/).map((t) => t.getAttribute('data-testid')!.replace('settings-tab-', ''))

describe('Instruments tabs', () => {
  it('community build: About last, no Online tab, no version', async () => {
    version('community')
    vi.spyOn(system, 'getSystem').mockRejectedValue(new Error('not needed'))
    render(<SettingsView apiBase="" onConfigUpdated={() => {}} initialTab="about" />)
    expect(tabs().at(-1)).toBe('about')
    expect(tabs()).not.toContain('online')
    expect(tabs()).not.toContain('sensors')
    expect(await screen.findByText(/A community build, made from the open-source code/)).toBeTruthy()
    expect(screen.queryByText(/0\.1\.17/)).toBeNull()
    expect(screen.getByRole('link', { name: 'Releases' })).toBeTruthy()
  })

  it('official build: the version, the groups it adds to About, and an Online tab with its groups', async () => {
    version('official')
    vi.spyOn(system, 'getSystem').mockRejectedValue(new Error('not needed'))
    ext.extensions.aboutGroups = [({ onStopping }: GroupProps) => <div role="group" aria-label="Updates"><button type="button" onClick={() => onStopping({ title: 'Installing the update…', hint: 'BenchMesh stops.' })}>Install</button></div>]
    ext.extensions.onlineGroups = [({ viewOnly }: GroupProps) => <div role="group" aria-label="BenchMesh account">{viewOnly ? 'view' : 'control'}</div>]
    render(<SettingsView apiBase="" onConfigUpdated={() => {}} initialTab="about" />)
    expect(await screen.findByText('BenchMesh 0.1.17')).toBeTruthy()
    expect(tabs()).toContain('online')
    expect(tabs().indexOf('online')).toBeLessThan(tabs().indexOf('system'))
    fireEvent.click(screen.getByRole('button', { name: 'Install' }))
    expect(await screen.findByText('Installing the update…')).toBeTruthy()
    expect(screen.getByText('BenchMesh stops. This page reloads by itself.')).toBeTruthy()
    fireEvent.click(screen.getByTestId('settings-tab-online'))
    expect(screen.getByRole('group', { name: 'BenchMesh account' }).textContent).toBe('control')
  })

  it('a distribution with sensors: a Sensors tab with its groups, after Metrics', () => {
    version('official')
    vi.spyOn(system, 'getSystem').mockRejectedValue(new Error('not needed'))
    ext.extensions.sensorsGroups = [({ viewOnly }: GroupProps) => <div role="group" aria-label="Environmental sensors">{viewOnly ? 'view' : 'control'}</div>]
    render(<SettingsView apiBase="" onConfigUpdated={() => {}} initialTab="sensors" />)
    expect(tabs().indexOf('sensors')).toBe(tabs().indexOf('metrics') + 1)
    expect(screen.getByRole('group', { name: 'Environmental sensors' }).textContent).toBe('control')
  })
})
