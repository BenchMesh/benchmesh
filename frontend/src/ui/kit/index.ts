/**
 * BenchMesh instrument kit: the visual building blocks every instrument panel is made of
 * (skins in `kit.css`: Reflective LCD by default, LED segment, Cockpit glass; see `skin.tsx`).
 * Rules of use:
 * - Readout: measured values only (fixed digit count, ghost segments).
 * - DigitEditor: setpoints; nothing is sent until Enter.
 * - ArmSwitch: anything that energises (operations with `energizes`): two presses to switch on.
 * - MicroSwitch: secondary, non-energising settings.
 * - Annunciators: state flags, always present, lit when active.
 * - RotaryDial / Selector: mutually exclusive functions, modes and ranges.
 * - Bargraph / Sparkline: fraction of range, recent history.
 * - Dropdowns: plain `<select>` elements; `SelectPopup` draws their list in the skin.
 * - ContextMenu: secondary actions on a thing (right-click / context-menu key), e.g. recording a reading.
 * - Confirmation (`confirm.ts`): after a change is sent, the control pulses (`data-confirm="waiting"`)
 *   until the instrument reports the new value; after 6 s without it a red dot marks it "unconfirmed".
 */
import './kit.css'

export { Readout, Annunciators, MicroSwitch, Selector, ActionButton } from './controls'
export type { Annunciator, Choice } from './controls'
export { DigitEditor } from './DigitEditor'
export type { DigitEditorHandle } from './DigitEditor'
export { ArmSwitch } from './ArmSwitch'
export { RotaryDial } from './RotaryDial'
export type { DialOption } from './RotaryDial'
export { Knob } from './Knob'
export { Bargraph } from './Bargraph'
export { Sparkline } from './Sparkline'
export { SegmentDisplay } from './SegmentDisplay'
export type { Tone } from './useCanvas'
export { repaintCanvases } from './useCanvas'
export { SkinProvider, useSkin, loadSkin, SKINS, SKIN_STORAGE_KEY, DEFAULT_SKIN } from './skin'
export type { Skin } from './skin'
export { WavePreview, WaveGlyph, WAVE_SHAPES } from './WavePreview'
export * from './format'
export { ScopeScreen } from './ScopeScreen'
export type { ScopeTrace } from './ScopeScreen'
export { SpectrumScreen } from './SpectrumScreen'
export { freqAt, freqFraction, levelDivs, spectrumPoints, peakIndex, maxHold, freqText, dbText } from './spectrum'
export { Stepper } from './Stepper'
export { BrandMark } from './BrandMark'
export { brandSvg, BRAND_ON_DARK, BRAND_ON_LIGHT } from './brand'
export type { BrandColours } from './brand'
export { step125, screenDivs, tracePoints, siText } from './scope'
export { ContextMenu, useContextMenu, clampToViewport } from './ContextMenu'
export { SelectPopups, installSelectPopups } from './SelectPopup'
export type { MenuItem } from './ContextMenu'
export { digitModel } from './digitModel'
export type { DigitSpec } from './digitModel'
export { MeterSymbol, drawSymbol, symbolText, MARKS } from './meterSymbol'
export type { SymbolPart, MarkName } from './meterSymbol'
