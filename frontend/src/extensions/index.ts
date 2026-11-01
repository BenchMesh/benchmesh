/**
 * The open-source build adds nothing to the UI. The official distribution replaces this file at
 * build time (contract `contract.ts`, `ai_context/OPEN_CORE.md`).
 */
import type { UiExtensions } from './contract'

export const extensions: UiExtensions = { root: [], systemGroups: [], aboutGroups: [], onlineGroups: [], sensorsGroups: [], topBarStatus: [], recordingViews: [], titleBadges: [], titleReadouts: [], panelActions: [], instrumentsSections: [], identityField: null }
