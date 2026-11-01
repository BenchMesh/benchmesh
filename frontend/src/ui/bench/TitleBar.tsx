import React, { useEffect, useState } from 'react'
import { BrandMark } from '../kit'
import type { AppVersion } from '../about/version'
import { closeWindow, hasOwnWindowControls, minimizeWindow, startWindowDrag, toggleMaximizeWindow, watchMaximized } from '../desktop'

interface Props {
  version?: AppVersion | null
  onAbout: () => void
  arranging: boolean
  canArrange: boolean
  onArrange: () => void
  /** notices after the version (a distribution's `titleBadges`) */
  badges?: React.ReactNode
  /** readouts on the right before Arrange (a distribution's `titleReadouts`) */
  readouts?: React.ReactNode
}

const isControl = (e: React.SyntheticEvent) => (e.target as HTMLElement).closest('button') != null

/**
 * Row 1 of the bench: brand and version on the left ("v0.1.16"; a community build "v0.1.16 community") with any
 * badges a distribution adds after it, the readouts a distribution adds and the app's own title-bar
 * controls (Arrange) on the right and, in the desktop app on Windows and Linux (no OS title bar), the window controls
 * after them. The bar moves the window; a double press maximizes or restores it.
 */
export function TitleBar({ version, onAbout, arranging, canArrange, onArrange, badges, readouts }: Props) {
  const own = hasOwnWindowControls()
  const [maximized, setMaximized] = useState(false)
  useEffect(() => (own ? watchMaximized(setMaximized) : undefined), [own])

  return (
    <div className="bench-title" data-own-controls={own || undefined}
      onMouseDown={own ? (e) => {
        if (e.button !== 0 || isControl(e)) return
        if (e.detail === 2) toggleMaximizeWindow()
        else startWindowDrag()
      } : undefined}>
      <span className="bench-title__brand">
        <BrandMark size={33} />
        <span className="bench-title__name">BenchMesh</span>
        {version?.version && (
          <button type="button" className="bench-title__version" onClick={onAbout} title="About and feedback">
            v{version.version}{!version.official && <span className="bench-title__build"> community</span>}
          </button>
        )}
        {badges}
      </span>
      <span className="bench-title__drag" />
      {readouts && <div className="bench-title__readouts">{readouts}</div>}
      <div className="bench-title__app">
        <button type="button" className="bench-title__tool" aria-pressed={arranging} onClick={onArrange} disabled={!canArrange}
          title="Arrange the panels">
          <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="1.5" y="2.5" width="5.5" height="5" rx="1" /><rect x="9" y="2.5" width="5.5" height="11" rx="1" /><rect x="1.5" y="9.5" width="5.5" height="4" rx="1" /></svg>
          {/* both words in one cell: the button keeps the wider one's width, so nothing beside it moves */}
          <span className="bench-title__swap">
            <span aria-hidden={arranging || undefined} data-on={!arranging}>Arrange</span>
            <span aria-hidden={!arranging || undefined} data-on={arranging}>Done</span>
          </span>
        </button>
      </div>
      {own && (
        <div className="bench-title__win" role="group" aria-label="Window">
          <button type="button" aria-label="Minimize" title="Minimize" onClick={minimizeWindow}>
            <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 5.5h10" /></svg>
          </button>
          <button type="button" aria-label={maximized ? 'Restore' : 'Maximize'} title={maximized ? 'Restore' : 'Maximize'} onClick={toggleMaximizeWindow}>
            {maximized
              ? <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M2.5 2.5V.5h7v7h-2" /><rect x=".5" y="2.5" width="7" height="7" /></svg>
              : <svg viewBox="0 0 10 10" aria-hidden="true"><rect x=".5" y=".5" width="9" height="9" /></svg>}
          </button>
          <button type="button" className="bench-title__close" aria-label="Close" title="Close" onClick={closeWindow}>
            <svg viewBox="0 0 10 10" aria-hidden="true"><path d="M0 0l10 10M10 0L0 10" /></svg>
          </button>
        </div>
      )}
    </div>
  )
}
