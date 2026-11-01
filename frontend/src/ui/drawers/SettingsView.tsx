/**
 * SettingsView - Application settings sidebar
 *
 * Tabs:
 * - Instruments: Instrument configuration
 * - Supported: instruments BenchMesh has definitions for, with each model's support status
 * - Miscellaneous: App settings (skin, start screen, history retention)
 * - Metrics: how busy each instrument's connection is
 * - Connectors: virtual SCPI instruments for VISA programs and the Python package (only in the app on this computer)
 * - Sensors: sensors a distribution adds (the official build: Environmental sensors; none: no tab)
 * - Online: services of benchmesh.online a distribution adds (the official build: the BenchMesh account; none: no tab)
 * - System: the computer's resources; restart, shut down and start with the computer when headless
 * - Remote access: control the bench from other computers (only in the app on this computer)
 * - About: this BenchMesh (build, version in the official build, its updates)
 */

import React, { useState, useEffect } from 'react';
import { VscTools, VscGear, VscChecklist, VscRemote, VscPulse, VscPlug, VscServer, VscGlobe, VscInfo, VscBeaker } from 'react-icons/vsc';
import { InstrumentConfigView } from './InstrumentConfigView';
import { SupportedView } from './SupportedView';
import { MetricsView } from './MetricsView';
import { SkinSetting } from './SkinSetting';
import { StartScreenSetting } from './StartScreenSetting';
import type { Instrument } from '../types';
import { RemoteSettings } from '../remote/RemoteSettings';
import { useRemote } from '../remote/RemoteProvider';
import { ConnectorsView } from '../connectors/ConnectorsView';
import { SystemView } from '../system/SystemView';
import { AboutTab } from '../about/AboutTab';
import { OnlineTab, hasOnlineTab } from '../online/OnlineTab';
import { SensorsTab, hasSensorsTab } from '../sensors/SensorsTab';
import './settings.css';

export type SettingsTab = 'instruments' | 'supported' | 'miscellaneous' | 'metrics' | 'connectors' | 'sensors' | 'online' | 'system' | 'remote' | 'about';

interface SettingsViewProps {
  apiBase: string;
  onConfigUpdated: () => void;
  autoAddInstrument?: boolean;
  onAutoAddComplete?: () => void;
  /** Configured instruments (names for the Metrics tab). */
  instruments?: Instrument[];
  /** The tab shown when the drawer opens. */
  initialTab?: SettingsTab;
}

// History retention options in days
const RETENTION_OPTIONS = [
  { value: 1, label: '1 day' },
  { value: 3, label: '3 days' },
  { value: 5, label: '5 days' },
  { value: 7, label: '7 days' },
] as const;

const DEFAULT_RETENTION_DAYS = 3;
const STORAGE_KEY_RETENTION = 'benchmesh:historyRetentionDays';

export const SettingsView: React.FC<SettingsViewProps> = ({ apiBase, onConfigUpdated, autoAddInstrument, onAutoAddComplete, instruments, initialTab = 'instruments' }) => {
  const [activeTab, setActiveTab] = useState<SettingsTab>(initialTab);
  const [historyRetention, setHistoryRetention] = useState<number>(() => {
    // Load from localStorage on mount
    try {
      const saved = localStorage.getItem(STORAGE_KEY_RETENTION);
      if (saved) {
        const parsed = parseInt(saved, 10);
        // Validate it's one of our allowed values
        if (RETENTION_OPTIONS.some(opt => opt.value === parsed)) {
          return parsed;
        }
      }
    } catch (e) {
      console.error('Failed to load history retention setting:', e);
    }
    return DEFAULT_RETENTION_DAYS;
  });

  // Save retention setting to localStorage when it changes
  useEffect(() => {
    try {
      localStorage.setItem(STORAGE_KEY_RETENTION, historyRetention.toString());

      // Dispatch custom event to notify RequestLogContext to cleanup
      window.dispatchEvent(new CustomEvent('benchmesh:historyRetentionChanged', {
        detail: { retentionDays: historyRetention }
      }));
    } catch (e) {
      console.error('Failed to save history retention setting:', e);
    }
  }, [historyRetention]);

  const handleRetentionChange = (event: React.ChangeEvent<HTMLSelectElement>) => {
    const value = parseInt(event.target.value, 10);
    setHistoryRetention(value);
  };

  // remote access settings are admin: only the local principal on a server that has them
  const remote = useRemote();
  const remoteSettings = remote?.principal === 'local' && remote.status !== null;
  // virtual SCPI ports listen on this computer: set up from here only
  const local = remote?.principal !== 'remote';
  const tabs = [
    { id: 'instruments', label: 'Instruments', icon: VscTools },
    { id: 'supported', label: 'Supported', icon: VscChecklist },
    { id: 'miscellaneous', label: 'Miscellaneous', icon: VscGear },
    { id: 'metrics', label: 'Metrics', icon: VscPulse },
    ...(hasSensorsTab() ? [{ id: 'sensors', label: 'Sensors', icon: VscBeaker }] : []),
    ...(local ? [{ id: 'connectors', label: 'Connectors', icon: VscPlug }] : []),
    ...(hasOnlineTab() ? [{ id: 'online', label: 'Online', icon: VscGlobe }] : []),
    { id: 'system', label: 'System', icon: VscServer },
    ...(remoteSettings ? [{ id: 'remote', label: 'Remote access', icon: VscRemote }] : []),
    { id: 'about', label: 'About', icon: VscInfo },
  ] as const;

  return (
    <div className="settings-view" data-testid="settings-view">
      {/* Tab Bar */}
      <div className="settings-view__tabs">
        {tabs.map((tab) => {
          const Icon = tab.icon;
          return (
            <button
              key={tab.id}
              className={`settings-view__tab ${activeTab === tab.id ? 'settings-view__tab--active' : ''}`}
              onClick={() => setActiveTab(tab.id as SettingsTab)}
              data-testid={`settings-tab-${tab.id}`}
            >
              <Icon size={16} />
              <span>{tab.label}</span>
            </button>
          );
        })}
      </div>

      {/* Tab Content */}
      <div className="settings-view__content">
        {/* kept mounted while hidden so unsaved instrument edits survive a tab switch */}
        <div hidden={activeTab !== 'instruments'}>
          <InstrumentConfigView
            apiBase={apiBase}
            onConfigUpdated={onConfigUpdated}
            autoAddNew={autoAddInstrument}
            onAutoAddComplete={onAutoAddComplete}
          />
        </div>

        {activeTab === 'supported' && <SupportedView apiBase={apiBase} />}

        {activeTab === 'miscellaneous' && (
          <div className="settings-view__sections">
            <SkinSetting />
            <StartScreenSetting />

            {/* History & Logs Section */}
            <div className="settings-view__section">
              <h3 className="settings-view__section-title">History & Logs</h3>

              <div className="settings-view__setting">
                <label htmlFor="history-retention" className="settings-view__label">
                  History Retention
                  <span className="settings-view__label-hint">
                    How long to keep request history
                  </span>
                </label>
                <select
                  id="history-retention"
                  className="settings-view__dropdown"
                  value={historyRetention}
                  onChange={handleRetentionChange}
                  data-testid="history-retention-dropdown"
                >
                  {RETENTION_OPTIONS.map((option) => (
                    <option key={option.value} value={option.value}>
                      {option.label}
                    </option>
                  ))}
                </select>
              </div>

              <p className="settings-view__section-description">
                Request history older than {historyRetention} {historyRetention === 1 ? 'day' : 'days'} will be automatically removed.
                History is stored locally in your browser.
              </p>
            </div>

            {/* Future sections can be added here */}
          </div>
        )}

        {activeTab === 'metrics' && <MetricsView instruments={instruments} />}

        {activeTab === 'connectors' && local && <ConnectorsView />}

        {activeTab === 'sensors' && hasSensorsTab() && <SensorsTab />}

        {activeTab === 'online' && hasOnlineTab() && <OnlineTab />}

        {activeTab === 'system' && <SystemView />}

        {activeTab === 'remote' && remoteSettings && <RemoteSettings />}

        {activeTab === 'about' && <AboutTab />}
      </div>
    </div>
  );
};
