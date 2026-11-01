/** Recordings v2 wire types (ai_context/migration/RECORDINGS.md). All times: integer ms since the epoch (UTC). */

export type RecordingState = 'recording' | 'paused' | 'stopped'

/** One reading of one instrument: what a series records. */
export interface Signal {
  device_id: string
  class: string
  channel: number
  field: string
  label: string
  unit: string
}

export interface Series extends Signal {
  id: number
  added_at: number
  removed_at: number | null
  /** an environmental reading the server keeps in every recording while the sensor reports: cannot be removed */
  locked?: boolean
}

export interface Recording {
  id: number
  name: string
  description: string | null
  interval_s: number
  state: RecordingState
  started_at: number
  stopped_at: number | null
  paused_at: number | null
  samples: number
  last_t: number | null
  series: Series[]
}

export interface NewRecording { name: string; description?: string; interval_s: number }

export interface SeriesData { id: number; t: number[]; v: (number | null)[] }
export interface RecordingData { recording_id: number; from: number | null; to: number | null; series: SeriesData[] }

export type RecordingsMessage =
  | { type: 'snapshot'; recordings: Recording[] }
  | { type: 'samples'; recording_id: number; t: number; values: Record<string, number | null> }
  | { type: 'recording'; recording: Recording }
  | { type: 'deleted'; recording_id: number }

/** Same instrument reading (label and unit do not matter). */
export const sameSignal = (a: Pick<Signal, 'device_id' | 'class' | 'channel' | 'field'>, b: Pick<Signal, 'device_id' | 'class' | 'channel' | 'field'>) =>
  a.device_id === b.device_id && a.class === b.class && a.channel === b.channel && a.field === b.field

export const isActive = (r: Recording) => r.state !== 'stopped'
