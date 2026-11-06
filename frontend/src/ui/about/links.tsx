/**
 * Web pages BenchMesh links to. The desktop app's webview does not follow external links, so
 * there the app opens the page in the system browser by name (`open_page`, a fixed list in
 * crates/benchmesh-app/src/links.rs); a browser opens it in a new tab.
 */
import React from 'react'
import { invoker } from '../desktop'

export type Page = 'issues' | 'new-issue' | 'releases' | 'docs' | 'docs-scpi' | 'docs-python' | 'headless' | 'linux-usb' | 'linux-serial' | 'macos-local-network' | 'macos-network-settings' | 'email'

export const PAGE_URLS: Record<Page, string> = {
  issues: 'https://github.com/BenchMesh/benchmesh/issues',
  'new-issue': 'https://github.com/BenchMesh/benchmesh/issues/new',
  releases: 'https://github.com/BenchMesh/benchmesh/releases/latest',
  docs: 'https://benchmesh.online/docs',
  'docs-scpi': 'https://benchmesh.online/docs/scpi',
  'docs-python': 'https://benchmesh.online/docs/python',
  // BenchMesh without a window (Instruments - System)
  headless: 'https://benchmesh.online/docs/headless',
  // the help links in the server's "no permission" messages (bm-transport access.rs)
  'linux-usb': 'https://benchmesh.online/docs/linux#usb-permissions',
  'linux-serial': 'https://benchmesh.online/docs/linux#serial-permissions',
  'macos-local-network': 'https://benchmesh.online/docs/macos#local-network',
  // System Settings → Privacy & Security → Local Network (the desktop app on a Mac only)
  'macos-network-settings': 'x-apple.systempreferences:com.apple.preference.security?Privacy_LocalNetwork',
  email: 'mailto:support@benchmesh.online',
}

export const SUPPORT_EMAIL = 'support@benchmesh.online'

/** A link to `page`: the system browser in the desktop app, a new tab elsewhere. */
export function PageLink({ page, className, children }: { page: Page; className?: string; children: React.ReactNode }) {
  const onClick = (e: React.MouseEvent) => {
    const invoke = invoker()
    if (!invoke) return // the browser follows the link
    e.preventDefault()
    void invoke('open_page', { page }).catch(() => window.open(PAGE_URLS[page], '_blank', 'noopener'))
  }
  // web pages in a new tab; the mail link hands over to the mail app (a new tab would stay empty)
  const tab = PAGE_URLS[page].startsWith('http') ? { target: '_blank', rel: 'noreferrer' } : {}
  return <a href={PAGE_URLS[page]} {...tab} className={className} onClick={onClick}>{children}</a>
}
