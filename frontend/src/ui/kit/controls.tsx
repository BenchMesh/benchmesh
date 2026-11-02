/** Small presentational controls of the kit: readout, annunciators, switches, selectors. */
import React, { useState } from 'react'
import { SegmentDisplay } from './SegmentDisplay'
import { Sparkline } from './Sparkline'
import type { Tone } from './useCanvas'
import { NOT_CONFIRMED, useConfirmation } from './confirm'

/**
 * Measured value: tag, seven-segment number, unit, optional trend underneath. `tone` names
 * the quantity (volt, amp, watt, meas) so skins that colour-code readouts can do so.
 */
export function Readout({ tag, text, unit, size = 'big', history, label, tone }: {
  tag: string; text: string; unit: string; size?: 'big' | 'mid' | 'small'; history?: number[]; label?: string; tone?: Tone
}) {
  return (
    <div className={`kit-readout kit-readout--${size}`}>
      <span className="kit-readout__tag">{tag}</span>
      <div className="kit-readout__num"><SegmentDisplay text={text} label={label ?? tag} tone={tone} /></div>
      <span className="kit-readout__unit">{unit}</span>
      {history && <Sparkline data={history} tone={tone} />}
    </div>
  )
}

export interface Annunciator { label: string; on: boolean; alert?: boolean; title?: string }

/** State flags, always present and lit when active, so the layout never shifts. */
export function Annunciators({ items }: { items: Annunciator[] }) {
  return (
    <div className="kit-anns" role="list" aria-label="Status">
      {/* keyed by place, not text: the list is fixed, and a flag's text changes and can equal another's
          (sweep NORM beside acquisition NORM); a duplicate key left stale copies on the screen */}
      {items.map((a, i) => (
        <span key={i} role="listitem" className={`kit-ann${a.alert ? ' kit-ann--alert' : ''}`} data-on={a.on}
          title={a.title} aria-label={`${a.label} ${a.on ? 'on' : 'off'}`}>{a.label}</span>
      ))}
    </div>
  )
}

/** Discreet switch for secondary, non-energising settings. */
export function MicroSwitch({ label, checked, onChange, disabled, title }: {
  label: string; checked: boolean; onChange: (next: boolean) => Promise<void> | void; disabled?: boolean; title?: string
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const confirm = useConfirmation(checked)
  return (
    <button
      type="button" role="switch" aria-checked={checked} className="kit-micro" disabled={disabled || busy}
      data-confirm={confirm.state} aria-busy={confirm.state === 'waiting' || busy}
      title={error ?? (confirm.state === 'unconfirmed' ? NOT_CONFIRMED : title)}
      onClick={async () => {
        const next = !checked
        setBusy(true)
        setError(null)
        try {
          await onChange(next)
          confirm.begin(next)
        } catch (e) { setError(e instanceof Error ? e.message : String(e)) } finally { setBusy(false) }
      }}
    >
      <span className="kit-micro__trk" />
      {label}
      {error && <span className="kit-error" role="alert">!</span>}
    </button>
  )
}

export interface Choice { value: string; label: React.ReactNode; title?: string; disabled?: boolean }

/**
 * Mutually exclusive choice: `seg` for modes, `chips` for ranges and options. `value` is the
 * instrument's reported choice; the chosen option is marked as waiting until it is reported.
 * `onChange` may return a promise of `false` when the request failed (stops waiting).
 */
export function Selector({ options, value, onChange, label, variant = 'seg', disabled }: {
  options: Choice[]; value: string | null; onChange: (v: string) => void | Promise<boolean | void>
  label: string; variant?: 'seg' | 'chips'; disabled?: boolean
}) {
  const confirm = useConfirmation(value)
  const choose = (v: string) => {
    confirm.begin(v)
    const r = onChange(v)
    if (r instanceof Promise) r.then((ok) => { if (ok === false) confirm.cancel() }, () => confirm.cancel())
  }
  return (
    <div className={variant === 'seg' ? 'kit-seg' : 'kit-chips'} role="group" aria-label={label}>
      {options.map((o) => {
        const target = confirm.target === o.value
        return (
          <button key={o.value} type="button" aria-pressed={o.value === value} title={target && confirm.state === 'unconfirmed' ? NOT_CONFIRMED : o.title}
            data-confirm={target ? confirm.state : undefined} aria-busy={target && confirm.state === 'waiting'}
            disabled={disabled || o.disabled} onClick={() => o.value !== value && choose(o.value)}>
            {o.label}
          </button>
        )
      })}
    </div>
  )
}

/**
 * Momentary action (peak search, single sweep, correction, trigger): after the request it
 * pulses until `done(reported, reportedAtPress)` holds for the instrument's reported value,
 * and shows "not confirmed" after the confirmation timeout. `onPress` rejects to show why.
 */
export function ActionButton<T>({ label, title, reported, done, onPress, disabled, primary }: {
  label: React.ReactNode; title?: string; reported: T; done: (now: T, before: T) => boolean
  onPress: () => Promise<unknown> | unknown; disabled?: boolean; primary?: boolean
}) {
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState<string | null>(null)
  const confirm = useConfirmation<T>(reported, (now, before) => done(now, before))
  return (
    <button type="button" className={`bench-btn kit-action${primary ? ' bench-btn--primary' : ''}`} disabled={disabled || busy}
      data-confirm={confirm.state} aria-busy={busy || confirm.state === 'waiting'}
      title={error ?? (confirm.state === 'unconfirmed' ? NOT_CONFIRMED : title)}
      onClick={async () => {
        const before = reported
        setBusy(true)
        setError(null)
        try {
          await onPress()
          confirm.begin(before)
        } catch (e) { setError(e instanceof Error ? e.message : String(e)) } finally { setBusy(false) }
      }}>
      {label}
      {error && <span className="kit-error" role="alert">!</span>}
    </button>
  )
}
