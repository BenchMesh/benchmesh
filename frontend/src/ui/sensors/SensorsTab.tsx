/**
 * Instruments - Sensors: sensors next to the instruments that a distribution adds
 * (`extensions.sensorsGroups`: the official build's Environmental sensors). Shown only when there
 * are any; their readings go into every recording (`Extension::environment` on the server).
 */
import { useViewOnly } from '../remote/RemoteProvider'
import { ComebackNotice, useComeback } from '../system/comeback'
import { extensions } from '../../extensions'

export const hasSensorsTab = () => extensions.sensorsGroups.length > 0

export function SensorsTab() {
  const viewOnly = useViewOnly()
  const comeback = useComeback()
  return (
    <section className="bench-page" aria-label="Sensors" data-testid="sensors-tab">
      <h3 className="settings-view__section-title">Sensors</h3>
      {comeback.phase !== 'running'
        ? <ComebackNotice phase={comeback.phase} stopping={comeback.stopping} />
        : extensions.sensorsGroups.map((Group, i) => <Group key={i} viewOnly={viewOnly} onStopping={comeback.onStopping} />)}
    </section>
  )
}
