import React from 'react'
import { describe, it, expect, vi, afterEach } from 'vitest'
import { render, screen, within, fireEvent, waitFor } from '@testing-library/react'
import { AboutView } from '../AboutView'
import { PageLink, PAGE_URLS, SUPPORT_EMAIL } from '../links'
import { fetchVersion } from '../version'

const answer = (body: unknown, status = 200) => vi.spyOn(globalThis, 'fetch').mockResolvedValue(new Response(JSON.stringify(body), { status }))

afterEach(() => {
  vi.restoreAllMocks()
  delete (window as { __TAURI__?: unknown }).__TAURI__
})

const OFFICIAL = { releaseVersion: '0.1.7', version: '0.1.7', displayVersion: 'BenchMesh 0.1.7', distribution: 'official' }
const COMMUNITY = { ...OFFICIAL, distribution: 'community' }

describe('version', () => {
  it('an official build has a release version', async () => {
    answer(OFFICIAL)
    expect(await fetchVersion()).toEqual({ official: true, version: '0.1.7', display: 'BenchMesh 0.1.7' })
  })
  it('a community build says so (the version: the last release it was built from)', async () => {
    answer(COMMUNITY)
    expect(await fetchVersion()).toEqual({ official: false, version: '0.1.7', display: 'BenchMesh 0.1.7 (community build)' })
    // a server that does not say counts as a community build
    answer({ releaseVersion: '0.1.7', version: '0.1.7' })
    expect(await fetchVersion()).toEqual({ official: false, version: '0.1.7', display: 'BenchMesh 0.1.7 (community build)' })
  })
  it('is null when the server does not answer', async () => {
    answer({}, 500)
    expect(await fetchVersion()).toBeNull()
    vi.spyOn(globalThis, 'fetch').mockRejectedValue(new Error('offline'))
    expect(await fetchVersion()).toBeNull()
  })
})

describe('About and feedback', () => {
  it('official: shows the version, Copy version and where releases are', async () => {
    answer(OFFICIAL)
    render(<AboutView />)
    expect(await screen.findByTestId('app-version')).toHaveTextContent('BenchMesh 0.1.7')
    expect(screen.getByRole('button', { name: 'Copy version' })).toBeInTheDocument()
    expect(screen.getByText(/looks for a newer release/)).toBeInTheDocument()
    expect(screen.getByRole('link', { name: 'GitHub releases' })).toBeInTheDocument()
    expect(screen.getAllByText(/include the version above/)).toHaveLength(2)
  })

  it('community: no version, no release or update text', async () => {
    answer(COMMUNITY)
    render(<AboutView />)
    expect(await screen.findByText(/you use a community build/)).toBeInTheDocument()
    expect(screen.queryByRole('region', { name: 'Version' })).toBeNull()
    expect(screen.queryByTestId('app-version')).toBeNull()
    expect(screen.queryByRole('button', { name: 'Copy version' })).toBeNull()
    expect(screen.queryByText(/newer release|version above|0\.1\.7/)).toBeNull()
    expect(screen.queryByRole('link', { name: 'GitHub releases' })).toBeNull()
  })

  it('official: app problems go to GitHub, instrument problems to the panel flag of a linked BenchMesh', async () => {
    answer(OFFICIAL)
    render(<AboutView />)
    await screen.findByTestId('app-version')
    const app = screen.getByRole('region', { name: 'Problems with the app' })
    expect(app).toHaveTextContent(/crashes or will not start/)
    expect(within(app).getByRole('link', { name: 'Report an app issue' })).toHaveAttribute('href', PAGE_URLS['new-issue'])
    const instrument = screen.getByTestId('instrument-problems')
    expect(instrument).toHaveTextContent('Report a problem')
    expect(instrument).toHaveTextContent('not as a GitHub issue')
    expect(instrument).toHaveTextContent(/linked to your BenchMesh account/)
    expect(within(instrument).queryByRole('link')).toBeNull()
  })

  it('community: instrument problems as a GitHub issue with the console text', async () => {
    answer(COMMUNITY)
    render(<AboutView />)
    const instrument = await screen.findByTestId('instrument-problems')
    expect(instrument).toHaveTextContent(/open a GitHub issue instead/)
    expect(instrument).toHaveTextContent(/press Copy or Save/)
    expect(instrument).not.toHaveTextContent(/linked to your BenchMesh account/)
  })

  it('links to the GitHub issues', async () => {
    answer(COMMUNITY)
    render(<AboutView />)
    expect(screen.getByRole('link', { name: 'Report an app issue' })).toHaveAttribute('href', 'https://github.com/BenchMesh/benchmesh/issues/new')
    expect(screen.getByRole('link', { name: 'See open issues' })).toHaveAttribute('href', PAGE_URLS.issues)
    expect(screen.getByText(PAGE_URLS.issues)).toBeInTheDocument()
  })

  it('offers the support email: a mail link and Copy email', async () => {
    answer(COMMUNITY)
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, { clipboard: { writeText } })
    render(<AboutView />)
    const mail = screen.getByRole('link', { name: 'support@benchmesh.online' })
    expect(mail).toHaveAttribute('href', 'mailto:support@benchmesh.online')
    expect(mail).not.toHaveAttribute('target')
    fireEvent.click(screen.getByRole('button', { name: 'Copy email' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith(SUPPORT_EMAIL))
    expect(await screen.findByRole('button', { name: 'Copied' })).toBeInTheDocument()
  })

  it('copies the version (official)', async () => {
    answer(OFFICIAL)
    const writeText = vi.fn().mockResolvedValue(undefined)
    Object.assign(navigator, { clipboard: { writeText } })
    render(<AboutView />)
    fireEvent.click(await screen.findByRole('button', { name: 'Copy version' }))
    await waitFor(() => expect(writeText).toHaveBeenCalledWith('BenchMesh 0.1.7'))
  })

  it('in the desktop app a link opens the system browser by page name', async () => {
    const invoke = vi.fn().mockResolvedValue(undefined)
    ;(window as { __TAURI__?: unknown }).__TAURI__ = { core: { invoke } }
    render(<PageLink page="new-issue">Report</PageLink>)
    const link = screen.getByRole('link', { name: 'Report' })
    const ev = new MouseEvent('click', { bubbles: true, cancelable: true })
    link.dispatchEvent(ev)
    expect(ev.defaultPrevented).toBe(true)
    expect(invoke).toHaveBeenCalledWith('open_page', { page: 'new-issue' })
  })

  it('in a browser the link is followed in a new tab', () => {
    render(<PageLink page="issues">Issues</PageLink>)
    const link = screen.getByRole('link', { name: 'Issues' })
    expect(link).toHaveAttribute('target', '_blank')
    const ev = new MouseEvent('click', { bubbles: true, cancelable: true })
    link.dispatchEvent(ev)
    expect(ev.defaultPrevented).toBe(false)
  })
})
