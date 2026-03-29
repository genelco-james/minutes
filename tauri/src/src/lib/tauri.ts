import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

// Recording commands
export const startRecording = (mode?: string) =>
  invoke("cmd_start_recording", { mode });
export const stopRecording = () => invoke("cmd_stop_recording");

// Status
export const getStatus = () =>
  invoke<{
    recording: boolean;
    processing: boolean;
    processing_stage?: string;
  }>("cmd_status");

// Meetings
export const listMeetings = () =>
  invoke<
    Array<{
      path: string;
      title: string;
      date: string;
      duration: string;
      attendees: string[];
      word_count: number;
      content_type: string;
      lifecycle?: string[];
    }>
  >("cmd_list_meetings");

export const getMeetingDetail = (path: string) =>
  invoke<{
    frontmatter: Record<string, unknown>;
    body: string;
    path: string;
  }>("cmd_get_meeting_detail", { path });

// Search
export const search = (query: string) =>
  invoke<Array<{ path: string; title: string; snippet: string }>>(
    "cmd_search",
    { query }
  );

// Settings
export const getSettings = () =>
  invoke<Record<string, unknown>>("cmd_get_settings");
export const setSetting = (key: string, value: string) =>
  invoke("cmd_set_setting", { key, value });

// Notes
export const addNote = (text: string) => invoke("cmd_add_note", { text });

// Vault
export const vaultStatus = () =>
  invoke<{ enabled: boolean; path?: string; strategy?: string }>(
    "cmd_vault_status"
  );

// Storage
export const getStorageStats = () =>
  invoke<{ total_mb: number; meetings: number; memos: number }>(
    "cmd_get_storage_stats"
  );

// Devices
export const listDevices = () =>
  invoke<Array<{ name: string; sample_rate: number; channels: number }>>(
    "cmd_list_voices"
  );

// Events
export const onRecordingStatus = (
  cb: (payload: { recording: boolean; elapsed_secs?: number }) => void
): Promise<UnlistenFn> =>
  listen("recording-status", (event) =>
    cb(event.payload as { recording: boolean; elapsed_secs?: number })
  );

export const onProcessingStatus = (
  cb: (payload: { stage: string; progress?: number }) => void
): Promise<UnlistenFn> =>
  listen("processing-status", (event) =>
    cb(event.payload as { stage: string; progress?: number })
  );

export const onLatestArtifact = (
  cb: (payload: { path: string; title: string }) => void
): Promise<UnlistenFn> =>
  listen("latest-artifact", (event) =>
    cb(event.payload as { path: string; title: string })
  );
