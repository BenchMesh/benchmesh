/**
 * What a BenchMesh distribution may add to the UI (contract `ai_context/OPEN_CORE.md`). The
 * open-source build adds nothing (`index.ts`); the official distribution replaces `index.ts` at
 * build time with its own (app updates, the BenchMesh account). This file stays: it is the shape both agree on.
 */
import type { ComponentType } from 'react'
import type { ChartSeries } from '../ui/recording/chartModel'
import type { Recording } from '../ui/recording/types'

/** BenchMesh is about to stop and start again; the page the group is on waits and reloads. */
export interface Stopping {
  /** e.g. "Installing the update…" */
  title: string
  /** what happens meanwhile */
  hint: string
}

/** A group on an Instruments tab (System, About, Online, Sensors). */
export interface GroupProps {
  /** a view-only remote session: show, change nothing */
  viewOnly: boolean
  /** the group has asked BenchMesh to stop and start again */
  onStopping: (what: Stopping) => void
}

/** What a recording view draws: the same data the chart gets (recorder dock and Recordings drawer). */
export interface RecordingViewProps {
  recording: Recording
  /** every series with its colour, label, unit and points (`null` breaks a gap) */
  series: ChartSeries[]
  /** series the user switched off in the legend */
  hidden: Set<number>
  /** time range to show (ms); undefined: the data's own */
  min?: number
  max?: number
  /** the dock's live view (new samples keep arriving), not the history */
  live: boolean
  /** the view now shows this time range (ms): the drawer loads finer data for it */
  onRange?: (from: number, to: number) => void
}

/** Another way to draw a recording, offered next to the chart. */
export interface RecordingView {
  /** stable id, remembered as the user's choice */
  id: string
  /** switch label, e.g. "Trends" */
  label: string
  /** switch tooltip */
  title?: string
  component: ComponentType<RecordingViewProps>
}

/** The Instruments tabs a distribution adds groups to. */
export type ExtensionTab = 'system' | 'about' | 'online' | 'sensors'

/** What a title-bar badge or a top-bar status may do besides showing itself. */
export interface OpenTabProps {
  /** opens the Instruments drawer on this tab */
  open: (tab: ExtensionTab) => void
}

/** The instrument a panel-header button acts on. */
export interface PanelActionProps {
  /** the instrument's id (its config entry) */
  instrumentId: string
  /** the panel's class, e.g. `PSU` */
  klass: string
  /** the name the panel shows */
  name: string
  /** connected right now */
  online: boolean
}

/** What a section at the top of Instruments - configuration is given. */
export interface InstrumentsSectionProps {
  apiBase: string
  /** the saved config entries */
  configured: ReadonlyArray<{ id: string }>
  /** the section added or changed a config entry on the server */
  onChanged: () => void
}

/** An entry with `match` (found by who the instrument is) in the Instruments form. */
export interface IdentityFieldProps {
  /** the entry's `match` */
  match: Record<string, unknown>
  /** the port or device it was last seen on */
  path?: string
  /** drop `match`: the entry opens a fixed port again */
  onUseFixedPort: () => void
}

export interface UiExtensions {
  /** Rendered once in the main window, above the bench (dialogs such as an update offer). */
  root: ComponentType[]
  /** Groups at the end of Instruments - System (the computer BenchMesh runs on). */
  systemGroups: ComponentType<GroupProps>[]
  /** Groups at the end of Instruments - About (this BenchMesh: version, updates). */
  aboutGroups: ComponentType<GroupProps>[]
  /** Groups of Instruments - Online (services of benchmesh.online); with none, the tab is not shown. */
  onlineGroups: ComponentType<GroupProps>[]
  /**
   * Groups of Instruments - Sensors (sensors next to the instruments, e.g. the lab's environment); with none, the tab
   * is not shown. The server side of an environmental sensor is `Extension::environment`.
   */
  sensorsGroups: ComponentType<GroupProps>[]
  /** Statuses after API and Readings in the top bar (draw a `StatusPill`, or nothing). */
  topBarStatus: ComponentType<OpenTabProps>[]
  /** Views of a recording besides the chart; with none, the dock and the drawer show no switch. */
  recordingViews: RecordingView[]
  /**
   * Readouts on the right of the title bar, before Arrange (the official build: the lab's environment and the time);
   * each draws nothing when it has nothing to show. A press inside a button does not move the window.
   */
  titleReadouts: ComponentType<OpenTabProps>[]
  /** Small notices after the version in the title bar (e.g. "Update available"); each draws nothing when it has nothing to say. */
  titleBadges: ComponentType<OpenTabProps>[]
  /** Buttons in every instrument panel's header, before Park (not while arranging, not in a view-only session). */
  panelActions: ComponentType<PanelActionProps>[]
  /** Sections at the top of Instruments - configuration (the official build: Detect). */
  instrumentsSections: ComponentType<InstrumentsSectionProps>[]
  /**
   * Shows an entry's `match` instead of its port and makes the form keep it without a port (the
   * official build finds instruments by it). `null`: this build opens the port an entry names.
   */
  identityField: ComponentType<IdentityFieldProps> | null
}
