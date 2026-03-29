import { useEffect, useState } from "react";
import {
  ArrowLeft,
  Mic,
  HardDrive,
  Globe,
  Keyboard,
  Activity,
  Download,
} from "lucide-react";
import {
  getSettings,
  setSetting,
  getStorageStats,
  vaultStatus,
} from "../lib/tauri";

type Props = {
  onBack: () => void;
};

type SettingsData = Record<string, unknown>;
type StorageStats = { total_mb: number; meetings: number; memos: number };
type VaultInfo = { enabled: boolean; path?: string; strategy?: string };

const MODELS = [
  { id: "tiny", label: "Tiny", size: "75 MB", desc: "Fastest, lowest quality" },
  { id: "base", label: "Base", size: "142 MB", desc: "Basic quality" },
  { id: "small", label: "Small", size: "466 MB", desc: "Good balance" },
  {
    id: "medium",
    label: "Medium",
    size: "1.5 GB",
    desc: "Better accuracy, recommended",
  },
  {
    id: "large-v3",
    label: "Large v3",
    size: "3.1 GB",
    desc: "Best accuracy, slower",
  },
];

const CALL_APPS = [
  "Microsoft Teams",
  "zoom.us",
  "FaceTime",
  "Webex",
  "Slack",
];

export function Settings({ onBack }: Props) {
  const [settings, setSettings] = useState<SettingsData>({});
  const [storage, setStorage] = useState<StorageStats | null>(null);
  const [vault, setVault] = useState<VaultInfo | null>(null);

  useEffect(() => {
    getSettings().then(setSettings).catch(() => {});
    getStorageStats().then(setStorage).catch(() => {});
    vaultStatus().then(setVault).catch(() => {});
  }, []);

  const currentModel =
    (settings["transcription.model"] as string) || "medium";
  const callDetectionEnabled =
    (settings["call_detection.enabled"] as boolean) ?? true;

  const handleModelChange = async (model: string) => {
    await setSetting("transcription.model", model);
    setSettings((s) => ({ ...s, "transcription.model": model }));
  };

  const handleCallDetectionToggle = async () => {
    const newValue = !callDetectionEnabled;
    await setSetting("call_detection.enabled", String(newValue));
    setSettings((s) => ({
      ...s,
      "call_detection.enabled": newValue,
    }));
  };

  return (
    <div className="flex flex-col h-screen bg-bg view-enter">
      {/* Header */}
      <header className="flex items-center gap-sm px-md py-sm border-b border-border-subtle shrink-0">
        <button
          onClick={onBack}
          className="p-xs rounded-md hover:bg-hover transition-colors"
        >
          <ArrowLeft size={16} className="text-text-secondary" />
        </button>
        <h1 className="text-sm font-medium text-text">Settings</h1>
      </header>

      <div className="flex-1 overflow-y-auto px-md py-md space-y-lg">
        {/* Transcription Model */}
        <section>
          <div className="flex items-center gap-sm mb-sm">
            <Download size={14} className="text-text-tertiary" />
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
              Transcription Model
            </h2>
          </div>
          <div className="space-y-xs">
            {MODELS.map((m) => (
              <button
                key={m.id}
                onClick={() => handleModelChange(m.id)}
                className={`w-full flex items-center justify-between px-sm py-xs rounded-md text-left transition-colors ${
                  currentModel === m.id
                    ? "bg-accent-blue-bg border border-accent-blue/30"
                    : "hover:bg-hover border border-transparent"
                }`}
              >
                <div>
                  <span className="text-sm text-text">{m.label}</span>
                  <span className="text-xs text-text-tertiary ml-sm">
                    {m.size}
                  </span>
                </div>
                <span className="text-xs text-text-tertiary">{m.desc}</span>
              </button>
            ))}
          </div>
        </section>

        {/* Call Detection */}
        <section>
          <div className="flex items-center justify-between mb-sm">
            <div className="flex items-center gap-sm">
              <Activity size={14} className="text-text-tertiary" />
              <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
                Call Detection
              </h2>
            </div>
            <button
              onClick={handleCallDetectionToggle}
              className={`w-9 h-5 rounded-full transition-colors relative ${
                callDetectionEnabled ? "bg-accent-green" : "bg-border"
              }`}
            >
              <div
                className={`absolute top-0.5 w-4 h-4 rounded-full bg-white shadow-sm transition-transform ${
                  callDetectionEnabled ? "translate-x-4" : "translate-x-0.5"
                }`}
              />
            </button>
          </div>
          <div className="space-y-xs pl-[22px]">
            {CALL_APPS.map((app) => (
              <div
                key={app}
                className="flex items-center gap-sm text-xs text-text-secondary"
              >
                <div className="w-1.5 h-1.5 rounded-full bg-text-tertiary" />
                {app}
              </div>
            ))}
          </div>
        </section>

        {/* Vault Sync */}
        <section>
          <div className="flex items-center gap-sm mb-sm">
            <Globe size={14} className="text-text-tertiary" />
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
              Vault Sync
            </h2>
          </div>
          {vault ? (
            <div className="bg-elevated rounded-lg border border-border-subtle p-sm">
              <div className="flex items-center gap-sm mb-xs">
                <div
                  className={`w-2 h-2 rounded-full ${vault.enabled ? "bg-accent-green" : "bg-text-tertiary"}`}
                />
                <span className="text-xs text-text">
                  {vault.enabled ? "Connected" : "Not connected"}
                </span>
              </div>
              {vault.path && (
                <p className="text-xs text-text-tertiary truncate">
                  {vault.path}
                </p>
              )}
              {vault.strategy && (
                <p className="text-xs text-text-tertiary mt-xs">
                  Strategy: {vault.strategy}
                </p>
              )}
            </div>
          ) : (
            <p className="text-xs text-text-tertiary">Loading...</p>
          )}
        </section>

        {/* Storage */}
        <section>
          <div className="flex items-center gap-sm mb-sm">
            <HardDrive size={14} className="text-text-tertiary" />
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
              Storage
            </h2>
          </div>
          {storage ? (
            <div className="bg-elevated rounded-lg border border-border-subtle p-sm">
              <div className="flex justify-between text-xs mb-xs">
                <span className="text-text-secondary">Total</span>
                <span className="text-text">
                  {storage.total_mb.toFixed(1)} MB
                </span>
              </div>
              <div className="w-full h-1.5 bg-border rounded-full overflow-hidden">
                <div
                  className="h-full bg-accent-blue rounded-full"
                  style={{
                    width: `${Math.min(100, (storage.total_mb / 5000) * 100)}%`,
                  }}
                />
              </div>
              <div className="flex justify-between text-xs text-text-tertiary mt-xs">
                <span>{storage.meetings} meetings</span>
                <span>{storage.memos} memos</span>
              </div>
            </div>
          ) : (
            <p className="text-xs text-text-tertiary">Loading...</p>
          )}
        </section>

        {/* Keyboard Shortcuts */}
        <section>
          <div className="flex items-center gap-sm mb-sm">
            <Keyboard size={14} className="text-text-tertiary" />
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
              Shortcuts
            </h2>
          </div>
          <div className="space-y-xs">
            {[
              { action: "Toggle recording", shortcut: "Caps Lock" },
              { action: "Quick note", shortcut: "⌘ + Shift + N" },
              { action: "Search", shortcut: "⌘ + K" },
            ].map((s) => (
              <div
                key={s.action}
                className="flex items-center justify-between text-xs"
              >
                <span className="text-text-secondary">{s.action}</span>
                <kbd className="px-sm py-[1px] rounded bg-elevated border border-border-subtle text-text-tertiary font-mono text-[10px]">
                  {s.shortcut}
                </kbd>
              </div>
            ))}
          </div>
        </section>

        {/* Audio Devices */}
        <section>
          <div className="flex items-center gap-sm mb-sm">
            <Mic size={14} className="text-text-tertiary" />
            <h2 className="text-[11px] font-medium uppercase tracking-wider text-text-tertiary">
              Audio Input
            </h2>
          </div>
          <p className="text-xs text-text-tertiary">
            Uses system default audio input device.
          </p>
        </section>
      </div>
    </div>
  );
}
