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
    ) {
        if !self.config.enabled {
            eprintln!("[call-detect] disabled in config");
            return;
        }

        let interval = Duration::from_secs(self.config.poll_interval_secs.max(1));
        eprintln!(
            "[call-detect] started — polling every {}s for {:?}",
            interval.as_secs(),
            self.config.apps
        );

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

            loop {
                std::thread::sleep(interval);

                // Skip entirely while processing a previous recording
                if processing.load(Ordering::Relaxed) {
                    continue;
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

                    if let Some((display_name, process_name)) = self.detect_active_call() {
                        if !self.in_cooldown(&process_name) {
                            eprintln!(
                                "[call-detect] detected: {} ({})",
                                display_name, process_name
                            );
                            self.set_cooldown(&process_name);

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
    fn detect_active_call(&self) -> Option<(String, String)> {
        // Check mic first — it's the cheaper signal to short-circuit on
        if !is_mic_in_use() {
            return None;
        }

        let running = running_process_names();

        for config_app in &self.config.apps {
            let config_lower = config_app.to_lowercase();
            // Substring match: "zoom.us" matches process "zoom.us",
            // "Microsoft Teams" matches "Microsoft Teams Helper", etc.
            if running.iter().any(|p| {
                p.to_lowercase().contains(&config_lower) || config_lower.contains(&p.to_lowercase())
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
}

/// Friendly display name for a process name.
fn display_name_for(process: &str) -> String {
    match process {
        "zoom.us" => "Zoom".into(),
        "Microsoft Teams" | "Microsoft Teams (work or school)" | "MSTeams" => "Teams".into(),
        "FaceTime" => "FaceTime".into(),
        "Webex" => "Webex".into(),
        "Slack" => "Slack".into(),
        other => other.into(),
    }
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
