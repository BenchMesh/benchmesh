/**
 * Run panel: starts the edited flow (saved: by id with the parameter form as overrides;
 * unsaved: the document itself), asks once to arm a flow that can switch outputs on, and
 * shows the active run: state, current step, prompt + Continue, variables, the live log and
 * a big Abort (Escape in this panel aborts too).
 */
import React, { useEffect, useRef, useState } from 'react'
import { createPortal } from 'react-dom'
import { ApiError, errorText } from './client'
import { paramValues } from './flowModel'
import { abortLabel, STATE_TEXT, useRun } from './RunProvider'
import { IssueList } from './StepCard'
import type { Energize, FlowDoc, Issue, RunRequest } from './types'

interface Props {
  doc: FlowDoc
  flowId: string | null
  dirty: boolean
  blocked: string | null
  onHighlight: (path: string) => void
  onOpenRecordings?: () => void
}

const timeText = (t: number) => {
  const d = new Date(t)
  return `${d.toLocaleTimeString([], { hour12: false })}.${String(d.getMilliseconds()).padStart(3, '0')}`
}

const valueText = (v: unknown) => (typeof v === 'number' ? String(Math.round(v * 1e6) / 1e6) : typeof v === 'string' ? v : JSON.stringify(v))

export function RunPanel({ doc, flowId, dirty, blocked, onHighlight, onOpenRecordings }: Props) {
  const run = useRun()!
  const [arm, setArm] = useState<{ req: RunRequest; list: Energize[] } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const [issues, setIssues] = useState<Issue[]>([])
  const [starting, setStarting] = useState(false)
  const logRef = useRef<HTMLOListElement>(null)
  const { status, events, active } = run

  useEffect(() => run.watch(), [run.watch]) // eslint-disable-line react-hooks/exhaustive-deps
  useEffect(() => {
    const el = logRef.current
    if (el) el.scrollTop = el.scrollHeight
  }, [events.length])

  const start = async (req: RunRequest) => {
    setError(null)
    setIssues([])
    setStarting(true)
    try {
      await run.start(req)
      setArm(null)
    } catch (e) {
      if (e instanceof ApiError && e.status === 409 && e.energizes) setArm({ req, list: e.energizes })
      else {
        setArm(null)
        setError(errorText(e))
        if (e instanceof ApiError) setIssues(e.issues)
      }
    } finally { setStarting(false) }
  }
  const runNow = () => start(flowId && !dirty ? { flow_id: flowId, params: paramValues(doc) } : { flow: doc, params: paramValues(doc) })
  const abort = () => { setError(null); run.abort().catch((e) => setError(errorText(e))) }

  const params = doc.params ?? []
  const vars = Object.entries(status.vars ?? {})
  return (
    <section className="auto-run" aria-label="Run" tabIndex={-1}
      onKeyDown={(e) => { if (e.key === 'Escape' && active) { e.stopPropagation(); e.preventDefault(); abort() } }}>
      <div className="auto-run__bar">
        <h4 className="auto-list__title">Run</h4>
        <span className="auto-state" data-state={status.state}>{STATE_TEXT[status.state] ?? status.state}</span>
        {status.flow?.name && status.state !== 'idle' && <span className="auto-run__flow">{status.flow.name}</span>}
      </div>
      {!active && (
        <div className="auto-run__start">
          {params.length > 0 && (
            <span className="bench-hint">With {params.map((p) => `${p.name} = ${valueText(p.value)}${p.unit ? ` ${p.unit}` : ''}`).join(', ')}</span>
          )}
          <button type="button" className="bench-btn bench-btn--primary auto-run__go" onClick={runNow} disabled={!!blocked || starting} aria-busy={starting}
            title={blocked ?? undefined}>
            ▶ {flowId && !dirty ? 'Run' : 'Run unsaved document'}
          </button>
          {blocked && <span className="bench-hint">{blocked}</span>}
        </div>
      )}
      {active && (
        <button type="button" className="bench-btn auto-btn--stop auto-run__abort" onClick={abort}
          title="Abort the run (Escape). Cleanup still runs; press again during cleanup to skip it.">
          ■ {abortLabel(status.state)}
        </button>
      )}
      {error && <span className="kit-error" role="alert">{error}</span>}
      <IssueList issues={issues} />

      {status.state === 'waiting' && status.prompt && (
        <div className="auto-prompt" role="alert">
          <span>{status.prompt}</span>
          <button type="button" className="bench-btn bench-btn--primary" onClick={() => run.resume().catch((e) => setError(errorText(e)))}>Continue</button>
        </div>
      )}
      {status.state !== 'idle' && (
        <dl className="auto-run__facts">
          {active && status.path && (
            <div><dt>Step</dt><dd><button type="button" className="auto-path" onClick={() => onHighlight(status.path!)}>{status.path}</button></dd></div>
          )}
          {status.error && <div><dt>Error</dt><dd className="kit-error">{status.error}</dd></div>}
          {status.recording_id != null && (
            <div><dt>Recording</dt><dd>
              {onOpenRecordings
                ? <button type="button" className="auto-path" onClick={onOpenRecordings}>#{status.recording_id} - open Recordings</button>
                : `#${status.recording_id}`}
            </dd></div>
          )}
        </dl>
      )}
      {vars.length > 0 && (
        <table className="auto-vars" aria-label="Variables">
          <thead><tr><th scope="col">Variable</th><th scope="col">Value</th></tr></thead>
          <tbody>{vars.map(([k, v]) => <tr key={k}><td>{k}</td><td>{valueText(v)}</td></tr>)}</tbody>
        </table>
      )}
      {events.length > 0 && (
        <ol className="auto-log" ref={logRef} aria-label="Run log">
          {events.map((e) => (
            <li key={e.seq} data-level={e.level}>
              <time>{timeText(e.t)}</time>
              {e.path ? <button type="button" className="auto-path" onClick={() => onHighlight(e.path)} title="Show this step in the editor">{e.path}</button> : <span />}
              <span className="auto-log__msg">{e.msg}</span>
            </li>
          ))}
        </ol>
      )}

      {/* drawn at the end of <body>, not inside the drawer: the Run panel's column is sticky and
          scrolls (overflow: auto), and WebKitGTK (the Linux app) then left the fixed dialog box out
          while its scrim still covered the window - the app looked hung */}
      {arm && createPortal(
        <div className="bench-modal" role="dialog" aria-modal="true" aria-label="Arm the run"
          onKeyDown={(e) => { if (e.key === 'Escape') e.stopPropagation() }}>
          {/* only Cancel closes it (a modal: a press beside it or Escape does nothing; Escape must not
              reach the drawer behind it either) */}
          <div className="bench-modal__scrim" />
          <div className="bench-modal__box">
            <h2>Arm the run?</h2>
            <p>This flow can switch outputs or inputs on:</p>
            <ul className="auto-arm__list">
              {arm.list.map((x, i) => (
                <li key={i}><b>{x.device}</b> {x.class} CH{x.channel} · {x.op} <span className="auto-step__path">{x.path}</span></li>
              ))}
            </ul>
            <p className="bench-hint">When the run fails or is aborted, everything it switched on is switched off again after the cleanup.</p>
            <div className="bench-modal__actions">
              <button type="button" className="bench-btn" onClick={() => setArm(null)} autoFocus>Cancel</button>
              <button type="button" className="bench-btn bench-btn--primary" disabled={starting} onClick={() => start({ ...arm.req, arm: true })}>⚡ Arm and run</button>
            </div>
          </div>
        </div>,
        document.body,
      )}
    </section>
  )
}
