use minutes_core::{Config, ContentType};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::AppHandle;

pub struct AppState {
    pub recording: Arc<AtomicBool>,
}

/// Start recording in a background thread.
pub fn start_recording(app: AppHandle, recording: Arc<AtomicBool>) {
    recording.store(true, Ordering::Relaxed);

    let config = Config::load();
    let wav_path = minutes_core::pid::current_wav_path();

    // Create PID file
    if let Err(e) = minutes_core::pid::create() {
        eprintln!("Failed to create PID: {}", e);
        recording.store(false, Ordering::Relaxed);
        return;
    }

    // Send notification
    #[cfg(feature = "notification")]
    {
        let _ = app.notification().title("Minutes").body("Recording started...").send();
    }

    // Record audio (blocks until recording flag is set to false)
    let stop_flag = recording.clone();
    match minutes_core::capture::record_to_wav(&wav_path, stop_flag, &config) {
        Ok(()) => {
            // Run pipeline
            match minutes_core::process(&wav_path, ContentType::Meeting, None, &config) {
                Ok(result) => {
                    eprintln!("Saved: {}", result.path.display());

                    #[cfg(feature = "notification")]
                    {
                        let _ = app.notification()
                            .title("Minutes")
                            .body(&format!("Meeting saved: {} ({} words)", result.title, result.word_count))
                            .send();
                    }
                }
                Err(e) => {
                    eprintln!("Pipeline error: {}", e);
                }
            }
        }
        Err(e) => {
            eprintln!("Capture error: {}", e);
        }
    }

    // Clean up
    minutes_core::pid::remove().ok();
    if wav_path.exists() {
        std::fs::remove_file(&wav_path).ok();
    }
    recording.store(false, Ordering::Relaxed);
}

/// Tauri command: get recording status
#[tauri::command]
pub fn cmd_status(state: tauri::State<AppState>) -> serde_json::Value {
    let recording = state.recording.load(Ordering::Relaxed);
    let status = minutes_core::pid::status();
    serde_json::json!({
        "recording": recording || status.recording,
        "pid": status.pid,
    })
}

/// Tauri command: list recent meetings
#[tauri::command]
pub fn cmd_list_meetings(limit: Option<usize>) -> serde_json::Value {
    let config = Config::load();
    let filters = minutes_core::search::SearchFilters {
        content_type: None,
        since: None,
        attendee: None,
    };

    match minutes_core::search::search("", &config, &filters) {
        Ok(results) => {
            let limited: Vec<_> = results.into_iter().take(limit.unwrap_or(10)).collect();
            serde_json::to_value(&limited).unwrap_or(serde_json::json!([]))
        }
        Err(_) => serde_json::json!([]),
    }
}

/// Tauri command: search meetings
#[tauri::command]
pub fn cmd_search(query: String) -> serde_json::Value {
    let config = Config::load();
    let filters = minutes_core::search::SearchFilters {
        content_type: None,
        since: None,
        attendee: None,
    };

    match minutes_core::search::search(&query, &config, &filters) {
        Ok(results) => serde_json::to_value(&results).unwrap_or(serde_json::json!([])),
        Err(_) => serde_json::json!([]),
    }
}
