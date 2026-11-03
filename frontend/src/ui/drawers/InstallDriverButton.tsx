/**
 * "Install driver" for a Windows USBTMC instrument that has no usable driver
 * (`POST /usbtmc-devices/driver`, ai_context/migration/IDENTITY.md): the server gives it
 * Microsoft's generic WinUSB driver after Windows asks the user for administrator approval.
 * The request waits for that answer, so the button stays busy meanwhile. Admin only: a remote
 * browser does not get the button (the prompt would appear on the bench computer). A distribution
 * gives other USB devices the driver the same way with its own `install` (the official build: the
 * sensor adapter).
 */

import { useState } from 'react';
import { apiError } from './apiError';
import { useIsRemote } from '../remote/RemoteProvider';

/** Windows re-enumerates the instrument after the driver change; lists are re-read after this. */
export const REENUMERATE_MS = 2000;

export async function installDriver(apiBase: string, device: string): Promise<void> {
  const resp = await fetch(`${apiBase}/usbtmc-devices/driver`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ device }),
  });
  if (!resp.ok) throw new Error(await apiError(resp));
}

/** The instrument at `device`, or another device with its own request. */
type Target = { apiBase: string; device: string } | { install: () => Promise<void>; title: string };

type Props = Target & {
  /** The driver is installed; called once Windows had time to re-enumerate the device. */
  onInstalled: () => void;
};

const INSTRUMENT_TITLE = "Give this instrument Microsoft's generic WinUSB driver (Windows asks for administrator approval once)";

export function InstallDriverButton({ onInstalled, ...target }: Props) {
  const [run, what, title] = 'install' in target
    ? [target.install, target.title, target.title]
    : [() => installDriver(target.apiBase, target.device), target.device, INSTRUMENT_TITLE];
  const [state, setState] = useState<'idle' | 'busy' | 'done'>('idle');
  const [error, setError] = useState<string | null>(null);
  const remote = useIsRemote();

  async function install() {
    setState('busy');
    setError(null);
    try {
      await run();
      setState('done');
      setTimeout(onInstalled, REENUMERATE_MS);
    } catch (e: any) {
      console.error(`Driver installation for ${what} failed:`, e);
      setError(e?.message || String(e));
      setState('idle');
    }
  }

  if (remote) return null;
  return (
    <div className="install-driver" data-testid="install-driver">
      <button
        type="button" className="bench-btn" onClick={install} disabled={state !== 'idle'}
        title={title}
      >
        {state === 'busy' ? 'Waiting for Windows approval…' : state === 'done' ? '✓ Driver installed' : 'Install driver'}
      </button>
      {error && <div className="instrument-config__inline-error">{error}</div>}
    </div>
  );
}
