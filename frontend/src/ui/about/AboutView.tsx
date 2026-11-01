/**
 * About and feedback drawer: where each kind of problem goes - the app's own problems to GitHub
 * issues, an instrument's to its panel's Report a problem (official build, linked to a BenchMesh
 * account; a community build: a GitHub issue with the console text) - and the contact; the
 * official build also its version and releases (a community build is not versioned: `version.ts`).
 */
import React, { useState } from 'react'
import { PageLink, PAGE_URLS, SUPPORT_EMAIL } from './links'
import { useAppVersion } from './version'

export function AboutView() {
  const v = useAppVersion()
  const [copied, setCopied] = useState<'version' | 'email' | null>(null)
  const copy = async (what: 'version' | 'email', text: string) => {
    try {
      await navigator.clipboard.writeText(text)
      setCopied(what)
      setTimeout(() => setCopied(null), 2000)
    } catch { /* the text stays selectable */ }
  }
  return (
    <div className="bench-page" data-testid="about-view">
      {v?.official && (
        <section className="bench-group" aria-label="Version">
          <span className="kit-label">Version</span>
          <div className="bench-group__row">
            <p className="bench-about__version" data-testid="app-version">{v.display}</p>
            <button type="button" className="bench-btn" onClick={() => void copy('version', v.display)}>{copied === 'version' ? 'Copied' : 'Copy version'}</button>
          </div>
          <p className="bench-hint">The desktop app looks for a newer release at every start and offers it. All releases: <PageLink page="releases">GitHub releases</PageLink>.</p>
        </section>
      )}
      <section className="bench-group" aria-label="Problems with the app">
        <span className="kit-label">Problems with the app</span>
        <p className="bench-hint">
          Something wrong with BenchMesh itself - it crashes or will not start, a window, drawer or control misbehaves, an
          update or installation fails on Windows, macOS or Linux - or an idea for it: open a GitHub issue (it needs a free
          GitHub account).
        </p>
        <div className="bench-group__row">
          <PageLink page="new-issue" className="bench-btn bench-btn--primary">Report an app issue</PageLink>
          <PageLink page="issues" className="bench-btn">See open issues</PageLink>
        </div>
        <p className="bench-hint"><code>{PAGE_URLS.issues}</code></p>
        <p className="bench-hint">{v?.official ? 'Please include the version above.' : v ? 'Please mention that you use a community build.' : ''}</p>
      </section>
      <section className="bench-group" aria-label="Problems with an instrument" data-testid="instrument-problems">
        <span className="kit-label">Problems with an instrument</span>
        {v?.official ? (
          <>
            <p className="bench-hint">
              One instrument does not connect, shows a wrong reading or a control does nothing: report it from that
              instrument, not as a GitHub issue. Press the flag (<b>Report a problem</b>) in its panel header: BenchMesh
              reads the instrument and sends what the maintainers need to fix its definition.
            </p>
            <p className="bench-hint">
              Reports are sent only from a BenchMesh linked to your BenchMesh account (free; Instruments - Online): the
              report becomes a ticket of your account, so the maintainers can ask you questions and tell you which version
              fixes it. Your reports and their state are in Instruments - Online.
            </p>
          </>
        ) : (
          <p className="bench-hint">
            One instrument does not connect, shows a wrong reading or a control does nothing: the official BenchMesh
            (benchmesh.online) reports it from the instrument's panel. In this build, open a GitHub issue instead: open the
            panel's console (the console icon in the panel header), press Copy or Save and add the text to the issue - it
            carries the instrument, its model, its identify reply and what was sent and received.
          </p>
        )}
      </section>
      <section className="bench-group" aria-label="Contact">
        <span className="kit-label">Contact</span>
        <div className="bench-group__row">
          <PageLink page="email" className="bench-about__email">{SUPPORT_EMAIL}</PageLink>
          <button type="button" className="bench-btn" onClick={() => void copy('email', SUPPORT_EMAIL)}>{copied === 'email' ? 'Copied' : 'Copy email'}</button>
        </div>
        <p className="bench-hint">Questions and anything that does not fit an issue report.{v?.official && ' Please include the version above.'}</p>
      </section>
    </div>
  )
}
