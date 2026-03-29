//! Auto-detect video/voice calls and prompt the user to start recording.
//! Also monitors active call-triggered recordings and auto-stops when the
//! meeting ends (detected via meeting window closing or mic silence fallback).
//!
//! Detection strategy: poll for known call-app processes that are actively
//! using the microphone. Two signals together (process running + mic active)
//! give high confidence with minimal false positives.
//!
//! Currently macOS-only. The detection functions (`running_process_names`,
//! `is_mic_in_use`) use CoreAudio and `ps`. Windows/Linux would need
//! alternative implementations behind `cfg(target_os)` gates.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use minutes_core::config::CallDetectionConfig;
use tauri::Emitter;

/// State for the call detection background loop.
pub struct CallDetector {
    config: CallDetectionConfig,
    /// Cooldown: (app_name, last_notified_time)
    last_notified: Mutex<Vec<(String, Instant)>>,
}

/// Payload emitted to the frontend when a call is detected.
#[derive(Clone, serde::Serialize)]
pub struct CallDetectedPayload {
    pub app_name: String,
    pub process_name: String,
}

/// Shared state for call detection, accessible from Tauri commands.
/// Tracks which app triggered the current recording (if any) so the
/// detection loop can monitor for call end and auto-stop.
pub struct CallDetectState {
    pub call_triggered_app: Arc<Mutex<Option<String>>>,
    /// Meeting title extracted from the call app window (e.g. Teams meeting subject).
    /// Set when a call is detected; consumed by start_recording to name the output file.
    pub detected_meeting_title: Arc<Mutex<Option<String>>>,
}

/// Grace period for mic-silence fallback (apps without window detection).
const MIC_SILENCE_GRACE_SECS: u64 = 30;

impl CallDetector {
    pub fn new(config: CallDetectionConfig) -> Self {
        Self {
            config,
            last_notified: Mutex::new(Vec::new()),
        }
    }

    /// Start the background detection loop. Runs in its own thread.
    ///
    /// Two modes:
    /// - **Start-detection**: When not recording, polls for active calls and
    ///   shows a floating prompt to start recording.
    /// - **End-detection**: When recording was triggered by a call prompt,
    ///   monitors for meeting end and auto-stops the recording.
    pub fn start(
        self: Arc<Self>,
        app: tauri::AppHandle,
        recording: Arc<AtomicBool>,
        processing: Arc<AtomicBool>,
        _stop_flag: Arc<AtomicBool>,
        call_triggered_app: Arc<Mutex<Option<String>>>,
        detected_meeting_title: Arc<Mutex<Option<String>>>,
    ) {
        if !self.config.enabled {
            eprintln!("[call-detect] disabled in config");
            return;
        }

        let interval = Duration::from_secs(self.config.poll_interval_secs.max(1));

        // Try to launch event-driven mic monitor
        let mic_monitor_bin = find_mic_monitor_binary();
        if mic_monitor_bin.is_some() {
            eprintln!(
                "[call-detect] started (event-driven) for {:?}",
                self.config.apps
            );
        } else {
            eprintln!(
                "[call-detect] started (polling every {}s) for {:?}",
                interval.as_secs(),
                self.config.apps
            );
        }

        std::thread::spawn(move || {
            // Initial delay to let the app finish launching
            std::thread::sleep(Duration::from_secs(5));

            // Counter for mic-silence fallback (used for apps without window detection)
            let mut silence_miss_count: u32 = 0;
            let grace_checks =
                (MIC_SILENCE_GRACE_SECS / self.config.poll_interval_secs.max(1)) as u32;

            // Track previous recording state to detect recording→stopped transitions
            let mut was_recording = false;
            // Prevent re-showing countdown popup every poll cycle
            let mut countdown_shown = false;
            // Window count when we started monitoring (for delta-based detection)
            let mut initial_window_count: Option<u32> = None;
            // Track mic state from event-driven monitor
            let mut mic_active = false;

            // Spawn mic_monitor subprocess if available
            let has_monitor = mic_monitor_bin.is_some();
            let mic_monitor = mic_monitor_bin.and_then(|path| {
                std::process::Command::new(path)
                    .stdout(std::process::Stdio::piped())
                    .stderr(std::process::Stdio::null())
                    .spawn()
                    .ok()
            });

            // Set up line reader for mic_monitor stdout
            let mic_reader = mic_monitor.map(|mut child| {
                use std::io::BufRead;
                let stdout = child.stdout.take().unwrap();
                let reader = std::io::BufReader::new(stdout);
                // Read until READY marker, consuming initial state
                let mut lines = reader.lines();
                for line in lines.by_ref() {
                    match line {
                        Ok(ref l) if l == "READY" => break,
                        Ok(ref l) if l == "MIC_ON" => mic_active = true,
                        Ok(ref l) if l == "MIC_OFF" => mic_active = false,
                        _ => break,
                    }
                }
                eprintln!("[call-detect] mic_monitor ready, initial mic_active={}", mic_active);
                (lines, child)
            });

            // Event-driven mode: use a non-blocking reader thread
            let (mic_tx, mic_rx) = std::sync::mpsc::channel::<bool>();
            if let Some((lines, _child)) = mic_reader {
                let tx = mic_tx.clone();
                std::thread::spawn(move || {
                    for line in lines {
                        match line {
                            Ok(ref l) if l == "MIC_ON" => { tx.send(true).ok(); }
                            Ok(ref l) if l == "MIC_OFF" => { tx.send(false).ok(); }
                            _ => break,
                        }
                    }
                });
            }

            let event_driven = has_monitor;

            loop {
                if event_driven {
                    // Event-driven: wait for mic events or timeout for periodic checks
                    match mic_rx.recv_timeout(interval) {
                        Ok(state) => {
                            mic_active = state;
                            eprintln!("[call-detect] mic event: {}", if mic_active { "ON" } else { "OFF" });
                        }
                        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
                            // Periodic check for window state during recording
                        }
                        Err(_) => {
                            // Channel disconnected — mic_monitor died, fall back to polling
                            eprintln!("[call-detect] mic_monitor disconnected, falling back to polling");
                            std::thread::sleep(interval);
                            mic_active = is_mic_in_use();
                        }
                    }
                } else {
                    // Polling mode
                    std::thread::sleep(interval);
                    mic_active = is_mic_in_use();
                }

                // Skip entirely while processing a previous recording
                if processing.load(Ordering::Relaxed) {
                    continue;
                }

                // Periodic heartbeat for debugging (every ~30s)
                static TICK: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
                let tick = TICK.fetch_add(1, Ordering::Relaxed);
                if tick % 10 == 0 {
                    eprintln!(
                        "[call-detect] tick={} mic={} recording={} event_driven={}",
                        tick, mic_active, recording.load(Ordering::Relaxed), event_driven
                    );
                }

                let is_recording = recording.load(Ordering::Relaxed);
                let triggered_app = call_triggered_app.lock().ok().and_then(|g| g.clone());

                if is_recording {
                    was_recording = true;

                    // ── END-DETECTION MODE ─────────────────────────
                    // Only monitor for call end if THIS recording was started via call prompt
                    if let Some(ref app_name) = triggered_app {
                        let meeting_active = if has_window_detection(app_name) {
                            // Count-based detection: record initial window count, detect drops
                            let current_count = get_window_count(app_name);
                            let initial = *initial_window_count.get_or_insert(current_count);
                            eprintln!(
                                "[call-detect] {} windows: initial={}, current={}",
                                app_name, initial, current_count
                            );
                            // Meeting is still active if window count hasn't dropped
                            current_count >= initial
                        } else {
                            // Fallback: use mic activity with grace period
                            if is_mic_in_use() {
                                silence_miss_count = 0;
                                true
                            } else {
                                silence_miss_count += 1;
                                silence_miss_count < grace_checks
                            }
                        };

                        if !meeting_active && !countdown_shown {
                            eprintln!(
                                "[call-detect] meeting ended for {} — showing stop countdown",
                                app_name
                            );

                            // Show countdown popup (user gets 10s to click "Continue")
                            crate::show_stop_countdown(&app);
                            countdown_shown = true;

                            // Reset cooldown for this app so a new call can be detected later
                            self.clear_cooldown(app_name);
                            silence_miss_count = 0;
                        }
                    } else {
                        // call_triggered_app was cleared (user clicked "Continue")
                        countdown_shown = false;
                        initial_window_count = None;
                    }
                    // If recording was started manually (triggered_app is None), do nothing
                } else {
                    // ── START-DETECTION MODE ───────────────────────
                    // Reset silence counter when not recording
                    silence_miss_count = 0;

                    // Clear call_triggered_app only on a true recording→stopped transition,
                    // NOT when recording hasn't started yet (avoids race condition where
                    // cmd_mark_call_triggered runs before recording flag is set)
                    if was_recording {
                        if triggered_app.is_some() {
                            if let Ok(mut g) = call_triggered_app.lock() {
                                *g = None;
                            }
                        }
                        // Clear ALL cooldowns so back-to-back meetings are detected immediately
                        self.clear_all_cooldowns();
                        eprintln!("[call-detect] recording stopped — cooldowns cleared for back-to-back meetings");
                    }
                    was_recording = false;
                    countdown_shown = false;
                    initial_window_count = None;

                    // Detection strategy:
                    // 1. If mic is active: check running apps (original approach)
                    // 2. If mic is off: still check for Teams meeting windows
                    //    (user may be muted when joining)
                    let detection = if mic_active {
                        self.detect_active_call(true)
                    } else {
                        // Mic-off fallback: check Teams meeting window directly.
                        // Catches meetings where user joins muted.
                        let title = get_teams_meeting_title();
                        if title.is_some() {
                            eprintln!("[call-detect] mic off but Teams meeting window found: {:?}", title);
                            Some(("Teams".to_string(), "com.microsoft.teams2".to_string()))
                        } else {
                            None
                        }
                    };

                    if let Some((display_name, process_name)) = detection {
                        if !self.in_cooldown(&process_name) {
                            let is_teams = is_teams_app(&process_name);
                            let meeting_title = if is_teams {
                                get_teams_meeting_title()
                            } else {
                                None
                            };

                            if is_teams && meeting_title.is_none() {
                                // Teams process is running but no meeting window — false positive
                                eprintln!("[call-detect] Teams running but no meeting window — skipping");
                                continue;
                            }

                            eprintln!(
                                "[call-detect] detected: {} ({})",
                                display_name, process_name
                            );
                            self.set_cooldown(&process_name);

                            // Store meeting title for naming the output file
                            if let Ok(mut t) = detected_meeting_title.lock() {
                                *t = meeting_title;
                            }

                            // Show floating prompt window instead of macOS notification
                            crate::show_call_prompt(&app, &display_name);

                            // Emit event to frontend for potential in-app banner
                            app.emit(
                                "call:detected",
                                CallDetectedPayload {
                                    app_name: display_name,
                                    process_name,
                                },
                            )
                            .ok();
                        }
                    }
                }
            }
        });
    }

    /// Check if any configured call app is running AND the mic is active.
    /// Uses bundle IDs for reliable detection, with process name fallback.
    fn detect_active_call(&self, mic_active: bool) -> Option<(String, String)> {
        if !mic_active {
            return None;
        }

        // Try bundle ID matching first (reliable)
        let bundle_ids = running_bundle_ids();
        for config_app in &self.config.apps {
            // Check if this config entry IS a bundle ID (contains a dot with no spaces)
            if config_app.contains('.') && !config_app.contains(' ') {
                if bundle_ids.iter().any(|b| b == config_app) {
                    let display = display_name_for(config_app);
                    return Some((display, config_app.clone()));
                }
            }
        }

        // Fallback: process name substring matching (for old-style config entries)
        let running = running_process_names();
        for config_app in &self.config.apps {
            // Skip bundle IDs in this pass
            if config_app.contains('.') && !config_app.contains(' ') {
                continue;
            }
            let config_lower = config_app.to_lowercase();
            if running.iter().any(|p| {
                p.to_lowercase().contains(&config_lower)
                    || config_lower.contains(&p.to_lowercase())
            }) {
                let display = display_name_for(config_app);
                return Some((display, config_app.clone()));
            }
        }
        None
    }

    fn in_cooldown(&self, process_name: &str) -> bool {
        let cooldown = Duration::from_secs(self.config.cooldown_minutes * 60);
        let entries = self.last_notified.lock().unwrap();
        entries
            .iter()
            .any(|(name, time)| name == process_name && time.elapsed() < cooldown)
    }

    fn set_cooldown(&self, process_name: &str) {
        let mut entries = self.last_notified.lock().unwrap();
        // Remove old entry for this app if exists
        entries.retain(|(name, _)| name != process_name);
        entries.push((process_name.to_string(), Instant::now()));
        // Prune stale entries
        let cutoff = Duration::from_secs(self.config.cooldown_minutes * 60 * 2);
        entries.retain(|(_, time)| time.elapsed() < cutoff);
    }

    fn clear_cooldown(&self, process_name: &str) {
        let mut entries = self.last_notified.lock().unwrap();
        entries.retain(|(name, _)| name != process_name);
    }

    fn clear_all_cooldowns(&self) {
        let mut entries = self.last_notified.lock().unwrap();
        entries.clear();
    }

    /// Check if any ignored app (e.g., Wispr Flow dictation) is currently running.
    /// If so, mic activity is likely from dictation, not a meeting call.
    fn is_ignored_app_active(&self) -> bool {
        if self.config.ignore_apps.is_empty() {
            return false;
        }
        let running = running_process_names();
        for ignored in &self.config.ignore_apps {
            let ignored_lower = ignored.to_lowercase();
            if running
                .iter()
                .any(|p| p.to_lowercase().contains(&ignored_lower))
            {
                eprintln!("[call-detect] ignored app active: {}", ignored);
                return true;
            }
        }
        false
    }
}

/// Friendly display name for a process name.
fn display_name_for(process: &str) -> String {
    match process {
        // Bundle IDs
        "us.zoom.xos" => "Zoom".into(),
        "com.microsoft.teams2" | "com.microsoft.teams" => "Teams".into(),
        "com.apple.FaceTime" => "FaceTime".into(),
        "com.cisco.webexmeetingsapp" => "Webex".into(),
        "com.tinyspeck.slackmacgap" | "com.slack.Slack" => "Slack".into(),
        // Process names (backward compat)
        "zoom.us" => "Zoom".into(),
        "Microsoft Teams" | "Microsoft Teams (work or school)" | "MSTeams" => "Teams".into(),
        "FaceTime" => "FaceTime".into(),
        "Webex" => "Webex".into(),
        "Slack" => "Slack".into(),
        other => other.into(),
    }
}

/// Check if a config entry refers to a Teams app (bundle ID or process name).
fn is_teams_app(app: &str) -> bool {
    matches!(
        app,
        "com.microsoft.teams2"
            | "com.microsoft.teams"
            | "Microsoft Teams"
            | "Microsoft Teams (work or school)"
            | "MSTeams"
            | "Teams"
    )
}

// ── Meeting window detection ────────────────────────────────

/// Whether we have a window-based detection method for this app.
fn has_window_detection(app_name: &str) -> bool {
    matches!(
        app_name,
        "Microsoft Teams"
            | "Microsoft Teams (work or school)"
            | "MSTeams"
            | "Teams"
            | "zoom.us"
            | "Zoom"
            | "FaceTime"
            | "Slack"
    )
}

/// Get the number of windows for a specific call app's process.
/// Used for delta-based detection: record count at start, detect drops.
///
/// Performance: ~5-10ms per call. Runs every 3s only while recording.
fn get_window_count(app_name: &str) -> u32 {
    let script = match app_name {
        // Teams: check both v1 and v2 process names
        "Microsoft Teams" | "Microsoft Teams (work or school)" | "MSTeams" | "Teams" => {
            r#"tell application "System Events"
    set teamNames to {"Microsoft Teams", "MSTeams"}
    repeat with t in teamNames
        if exists process t then return count of windows of process t
    end repeat
    return 0
end tell"#
        }
        "zoom.us" | "Zoom" => {
            r#"tell application "System Events"
    if exists process "zoom.us" then return count of windows of process "zoom.us"
    return 0
end tell"#
        }
        "FaceTime" => {
            r#"tell application "System Events"
    if exists process "FaceTime" then return count of windows of process "FaceTime"
    return 0
end tell"#
        }
        "Slack" => {
            r#"tell application "System Events"
    if exists process "Slack" then return count of windows of process "Slack"
    return 0
end tell"#
        }
        _ => return u32::MAX, // Unknown app: return high count so delta never triggers
    };

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            String::from_utf8_lossy(&out.stdout)
                .trim()
                .parse::<u32>()
                .unwrap_or(u32::MAX)
        }
        _ => {
            eprintln!("[call-detect] window count check failed for {}", app_name);
            u32::MAX // Assume high count to avoid false stops
        }
    }
}

// ── macOS-specific detection ──────────────────────────────────

/// Get bundle IDs of all running apps via `lsappinfo`. Fast (~5ms).
/// More reliable than process name matching for app identification.
fn running_bundle_ids() -> Vec<String> {
    let output = std::process::Command::new("lsappinfo")
        .args(["list"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("bundleID=") && !trimmed.contains("NULL") {
                        // Extract bundle ID from: bundleID="com.example.app"
                        trimmed
                            .strip_prefix("bundleID=\"")
                            .and_then(|s| s.strip_suffix('"'))
                            .map(|s| s.to_string())
                    } else {
                        None
                    }
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Get list of running process names via `ps`. Fast (~2ms), no permissions
/// needed, no osascript overhead.
fn running_process_names() -> Vec<String> {
    let output = std::process::Command::new("ps")
        .args(["-eo", "comm="])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines()
                .filter_map(|line| {
                    // ps returns full paths like /Applications/zoom.us.app/Contents/MacOS/zoom.us
                    // Extract just the binary name
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        return None;
                    }
                    Some(trimmed.rsplit('/').next().unwrap_or(trimmed).to_string())
                })
                .collect()
        }
        _ => Vec::new(),
    }
}

/// Check if the default audio input device is currently being used.
///
/// Uses a pre-compiled Swift helper that calls CoreAudio
/// `kAudioDevicePropertyDeviceIsRunningSomewhere` on the default input device.
/// Works on both Intel and Apple Silicon Macs.
///
/// Falls back to an inline `swift` invocation if the helper binary is missing.
fn is_mic_in_use() -> bool {
    // Try the pre-compiled helper first (fast: ~5ms)
    let helper = find_mic_check_binary();
    if let Some(path) = &helper {
        if let Ok(out) = std::process::Command::new(path).output() {
            if out.status.success() {
                let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
                return text == "1";
            }
        }
    }

    // Fallback: inline swift (slower: ~200ms, but always works)
    let script = r#"
import CoreAudio
var id = AudioObjectID(kAudioObjectSystemObject)
var pa = AudioObjectPropertyAddress(mSelector: kAudioHardwarePropertyDefaultInputDevice, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
var sz = UInt32(MemoryLayout<AudioObjectID>.size)
guard AudioObjectGetPropertyData(AudioObjectID(kAudioObjectSystemObject), &pa, 0, nil, &sz, &id) == noErr else { print("0"); exit(0) }
var r: UInt32 = 0
var ra = AudioObjectPropertyAddress(mSelector: kAudioDevicePropertyDeviceIsRunningSomewhere, mScope: kAudioObjectPropertyScopeGlobal, mElement: kAudioObjectPropertyElementMain)
sz = UInt32(MemoryLayout<UInt32>.size)
guard AudioObjectGetPropertyData(id, &ra, 0, nil, &sz, &r) == noErr else { print("0"); exit(0) }
print(r > 0 ? "1" : "0")
"#;

    let output = std::process::Command::new("swift")
        .arg("-e")
        .arg(script)
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim() == "1",
        _ => false,
    }
}

/// Find the pre-compiled mic_monitor binary (event-driven mic listener).
fn find_mic_monitor_binary() -> Option<std::path::PathBuf> {
    if let Ok(exe) = std::env::current_exe() {
        let beside_exe = exe.parent().unwrap_or(exe.as_ref()).join("mic_monitor");
        if beside_exe.exists() {
            return Some(beside_exe);
        }
    }
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin/mic_monitor");
    if dev_path.exists() {
        return Some(dev_path);
    }
    None
}

/// Find the pre-compiled mic_check binary.
/// Checks next to the app binary first, then the source tree location.
fn find_mic_check_binary() -> Option<std::path::PathBuf> {
    // In the bundled app: same directory as the main binary
    if let Ok(exe) = std::env::current_exe() {
        let beside_exe = exe.parent().unwrap_or(exe.as_ref()).join("mic_check");
        if beside_exe.exists() {
            return Some(beside_exe);
        }
    }

    // In development: check the source tree
    let dev_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("bin/mic_check");
    if dev_path.exists() {
        return Some(dev_path);
    }

    None
}

/// Extract the current meeting title from Microsoft Teams via AppleScript.
///
/// Teams window titles follow the pattern: `{Tab/Meeting} | {Section} | Microsoft Teams`.
/// Non-meeting windows have known tab names (Calendar, Chat, Activity, etc.) as the
/// first segment. Meeting windows have the meeting subject as the first segment.
/// Returns the meeting subject, or None if no meeting window is found.
pub fn get_teams_meeting_title() -> Option<String> {
    let script = r#"tell application "System Events"
    set teamNames to {"MSTeams", "Microsoft Teams"}
    repeat with t in teamNames
        if exists process t then
            tell process t
                set windowNames to name of every window
                repeat with wn in windowNames
                    set wn to contents of wn
                    set AppleScript's text item delimiters to " | "
                    set parts to text items of wn
                    set AppleScript's text item delimiters to ""
                    if (count of parts) ≥ 2 then
                        set lastPart to item (count of parts) of parts
                        set firstPart to item 1 of parts
                        if lastPart is "Microsoft Teams" then
                            set knownTabs to {"Calendar", "Chat", "Activity", "Teams", "Calls", "Files", "Apps", "Help", "Settings", "Search", "Notifications"}
                            set isMeeting to true
                            repeat with tab in knownTabs
                                if firstPart is (contents of tab) then
                                    set isMeeting to false
                                    exit repeat
                                end if
                            end repeat
                            if isMeeting then return firstPart
                        end if
                    end if
                end repeat
            end tell
            exit repeat
        end if
    end repeat
    return ""
end tell"#;

    let output = std::process::Command::new("osascript")
        .arg("-e")
        .arg(script)
        .output();

    match output {
        Ok(out) if out.status.success() => {
            let title = String::from_utf8_lossy(&out.stdout).trim().to_string();
            if title.is_empty() {
                None
            } else {
                eprintln!("[call-detect] extracted Teams meeting title: {:?}", title);
                Some(title)
            }
        }
        Ok(out) => {
            eprintln!(
                "[call-detect] Teams title extraction failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            None
        }
        Err(e) => {
            eprintln!("[call-detect] Teams title extraction error: {}", e);
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cooldown_tracking() {
        let detector = CallDetector::new(CallDetectionConfig {
            enabled: true,
            poll_interval_secs: 1,
            cooldown_minutes: 5,
            apps: vec!["zoom.us".into()],
            ignore_apps: vec![],
        });

        assert!(!detector.in_cooldown("zoom.us"));
        detector.set_cooldown("zoom.us");
        assert!(detector.in_cooldown("zoom.us"));
        assert!(!detector.in_cooldown("FaceTime"));
    }

    #[test]
    fn cooldown_clear() {
        let detector = CallDetector::new(CallDetectionConfig {
            enabled: true,
            poll_interval_secs: 1,
            cooldown_minutes: 5,
            apps: vec!["zoom.us".into()],
            ignore_apps: vec![],
        });

        detector.set_cooldown("zoom.us");
        assert!(detector.in_cooldown("zoom.us"));
        detector.clear_cooldown("zoom.us");
        assert!(!detector.in_cooldown("zoom.us"));
    }

    #[test]
    fn display_names() {
        assert_eq!(display_name_for("zoom.us"), "Zoom");
        assert_eq!(display_name_for("Microsoft Teams"), "Teams");
        assert_eq!(display_name_for("MSTeams"), "Teams");
        assert_eq!(display_name_for("FaceTime"), "FaceTime");
        assert_eq!(display_name_for("SomeOtherApp"), "SomeOtherApp");
    }

    #[test]
    fn window_detection_mapping() {
        assert!(has_window_detection("Microsoft Teams"));
        assert!(has_window_detection("MSTeams"));
        assert!(has_window_detection("Teams"));
        assert!(has_window_detection("zoom.us"));
        assert!(has_window_detection("FaceTime"));
        assert!(has_window_detection("Slack"));
        assert!(!has_window_detection("SomeOtherApp"));
    }

    #[test]
    fn process_list_returns_real_results() {
        let procs = running_process_names();
        // ps should always return at least a few processes
        assert!(!procs.is_empty(), "process list should not be empty");
    }

    #[test]
    fn mic_check_does_not_panic() {
        // Just verify the function returns without crashing.
        // Will return false unless something is using the mic right now.
        let _result = is_mic_in_use();
    }
}
