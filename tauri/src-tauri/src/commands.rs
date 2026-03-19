use minutes_core::{Config, ContentType};
use std::cmp::Reverse;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

pub struct AppState {
    pub recording: Arc<AtomicBool>,
    pub stop_flag: Arc<AtomicBool>,
    pub processing: Arc<AtomicBool>,
    pub processing_stage: Arc<Mutex<Option<String>>>,
    pub latest_output: Arc<Mutex<Option<OutputNotice>>>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeetingSection {
    pub heading: String,
    pub content: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct MeetingDetail {
    pub path: String,
    pub title: String,
    pub date: String,
    pub duration: String,
    pub content_type: String,
    pub status: Option<String>,
    pub context: Option<String>,
    pub attendees: Vec<String>,
    pub calendar_event: Option<String>,
    pub sections: Vec<MeetingSection>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct OutputNotice {
    pub kind: String,
    pub title: String,
    pub path: String,
    pub detail: String,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct RecoveryItem {
    pub kind: String,
    pub title: String,
    pub path: String,
    pub detail: String,
    pub retry_type: String,
}

fn preserve_failed_capture(wav_path: &std::path::Path, config: &Config) -> Option<PathBuf> {
    let metadata = wav_path.metadata().ok()?;
    if metadata.len() == 0 {
        return None;
    }

    let dir = config.output_dir.join("failed-captures");
    std::fs::create_dir_all(&dir).ok()?;
    let dest = dir.join(format!(
        "{}-capture.wav",
        chrono::Local::now().format("%Y-%m-%d-%H%M%S")
    ));

    std::fs::copy(wav_path, &dest).ok()?;
    std::fs::remove_file(wav_path).ok();
    Some(dest)
}

pub fn recording_active(recording: &Arc<AtomicBool>) -> bool {
    recording.load(Ordering::Relaxed) || minutes_core::pid::status().recording
}

pub fn request_stop(
    recording: &Arc<AtomicBool>,
    stop_flag: &Arc<AtomicBool>,
) -> Result<(), String> {
    match minutes_core::pid::check_recording() {
        Ok(Some(pid)) => {
            if pid == std::process::id() {
                stop_flag.store(true, Ordering::Relaxed);
                recording.store(true, Ordering::Relaxed);
                Ok(())
            } else {
                let rc = unsafe { libc::kill(pid as i32, libc::SIGTERM) };
                if rc != 0 {
                    return Err(std::io::Error::last_os_error().to_string());
                }
                Ok(())
            }
        }
        Ok(None) => {
            recording.store(false, Ordering::Relaxed);
            Err("Not recording".into())
        }
        Err(e) => Err(e.to_string()),
    }
}

fn wait_for_path_removal(path: &std::path::Path, timeout: Option<std::time::Duration>) -> bool {
    let start = std::time::Instant::now();
    while path.exists() {
        if let Some(timeout) = timeout {
            if start.elapsed() >= timeout {
                return false;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(250));
    }
    true
}

pub fn wait_for_recording_shutdown(timeout: std::time::Duration) -> bool {
    let pid_path = minutes_core::pid::pid_path();
    wait_for_path_removal(&pid_path, Some(timeout))
}

pub fn wait_for_recording_shutdown_forever() {
    let pid_path = minutes_core::pid::pid_path();
    let _ = wait_for_path_removal(&pid_path, None);
}

fn stage_label(stage: minutes_core::pipeline::PipelineStage) -> &'static str {
    match stage {
        minutes_core::pipeline::PipelineStage::Transcribing => "Transcribing audio",
        minutes_core::pipeline::PipelineStage::Diarizing => "Separating speakers",
        minutes_core::pipeline::PipelineStage::Summarizing => "Generating summary",
        minutes_core::pipeline::PipelineStage::Saving => "Saving meeting",
    }
}

fn set_processing_stage(stage: &Arc<Mutex<Option<String>>>, value: Option<&str>) {
    if let Ok(mut current) = stage.lock() {
        *current = value.map(String::from);
    }
}

fn set_latest_output(
    latest_output: &Arc<Mutex<Option<OutputNotice>>>,
    notice: Option<OutputNotice>,
) {
    if let Ok(mut current) = latest_output.lock() {
        *current = notice;
    }
}

fn parse_sections(body: &str) -> Vec<MeetingSection> {
    let mut sections = Vec::new();
    let mut current_heading: Option<String> = None;
    let mut current_lines: Vec<String> = Vec::new();

    for line in body.lines() {
        if let Some(heading) = line.strip_prefix("## ") {
            if let Some(existing_heading) = current_heading.take() {
                sections.push(MeetingSection {
                    heading: existing_heading,
                    content: current_lines.join("\n").trim().to_string(),
                });
            }
            current_heading = Some(heading.trim().to_string());
            current_lines.clear();
        } else if current_heading.is_some() {
            current_lines.push(line.to_string());
        }
    }

    if let Some(existing_heading) = current_heading.take() {
        sections.push(MeetingSection {
            heading: existing_heading,
            content: current_lines.join("\n").trim().to_string(),
        });
    }

    sections
}

fn recovery_title(path: &std::path::Path, fallback: &str) -> String {
    path.file_stem()
        .and_then(|stem| stem.to_str())
        .map(|stem| stem.replace('-', " "))
        .map(|stem| stem.trim().to_string())
        .filter(|stem| !stem.is_empty())
        .unwrap_or_else(|| fallback.to_string())
}

fn scan_recovery_items(config: &Config) -> Vec<RecoveryItem> {
    let mut found: Vec<(SystemTime, RecoveryItem)> = Vec::new();

    let current_wav = minutes_core::pid::current_wav_path();
    if current_wav.exists() && !minutes_core::pid::status().recording {
        if let Ok(metadata) = current_wav.metadata() {
            let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
            found.push((
                modified,
                RecoveryItem {
                    kind: "stale-recording".into(),
                    title: "Unprocessed live recording".into(),
                    path: current_wav.display().to_string(),
                    detail: "Minutes found an unfinished live capture that never made it through the pipeline.".into(),
                    retry_type: "meeting".into(),
                },
            ));
        }
    }

    let failed_captures = config.output_dir.join("failed-captures");
    if let Ok(entries) = std::fs::read_dir(&failed_captures) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                let modified = entry
                    .metadata()
                    .ok()
                    .and_then(|m| m.modified().ok())
                    .unwrap_or(SystemTime::UNIX_EPOCH);
                found.push((
                    modified,
                    RecoveryItem {
                        kind: "preserved-capture".into(),
                        title: recovery_title(&path, "Preserved capture"),
                        path: path.display().to_string(),
                        detail:
                            "A live recording was preserved because capture or processing failed."
                                .into(),
                        retry_type: "meeting".into(),
                    },
                ));
            }
        }
    }

    for watch_path in &config.watch.paths {
        let failed_dir = watch_path.join("failed");
        if let Ok(entries) = std::fs::read_dir(&failed_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let modified = entry
                        .metadata()
                        .ok()
                        .and_then(|m| m.modified().ok())
                        .unwrap_or(SystemTime::UNIX_EPOCH);
                    found.push((
                        modified,
                        RecoveryItem {
                            kind: "watch-failed".into(),
                            title: recovery_title(&path, "Failed watched file"),
                            path: path.display().to_string(),
                            detail: "A watched audio file failed to process and is waiting for manual retry.".into(),
                            retry_type: config.watch.r#type.clone(),
                        },
                    ));
                }
            }
        }
    }

    found.sort_by_key(|(modified, _)| Reverse(*modified));
    found.into_iter().map(|(_, item)| item).collect()
}

/// Start recording in a background thread.
pub fn start_recording(
    _app_handle: tauri::AppHandle,
    recording: Arc<AtomicBool>,
    stop_flag: Arc<AtomicBool>,
    processing: Arc<AtomicBool>,
    processing_stage: Arc<Mutex<Option<String>>>,
    latest_output: Arc<Mutex<Option<OutputNotice>>>,
) {
    recording.store(true, Ordering::Relaxed);
    stop_flag.store(false, Ordering::Relaxed);
    processing.store(false, Ordering::Relaxed);
    set_processing_stage(&processing_stage, None);
    set_latest_output(&latest_output, None);

    let config = Config::load();
    let wav_path = minutes_core::pid::current_wav_path();

    if let Err(e) = minutes_core::pid::create() {
        eprintln!("Failed to create PID: {}", e);
        recording.store(false, Ordering::Relaxed);
        return;
    }

    minutes_core::notes::save_recording_start().ok();
    eprintln!("Recording started...");

    let mut remove_current_wav = false;
    match minutes_core::capture::record_to_wav(&wav_path, stop_flag, &config) {
        Ok(()) => {
            recording.store(false, Ordering::Relaxed);
            processing.store(true, Ordering::Relaxed);
            let title = chrono::Local::now()
                .format("Recording %Y-%m-%d %H:%M")
                .to_string();
            match minutes_core::pipeline::process_with_progress(
                &wav_path,
                ContentType::Meeting,
                Some(&title),
                &config,
                |stage| set_processing_stage(&processing_stage, Some(stage_label(stage))),
            ) {
                Ok(result) => {
                    remove_current_wav = true;
                    set_latest_output(
                        &latest_output,
                        Some(OutputNotice {
                            kind: "saved".into(),
                            title: result.title.clone(),
                            path: result.path.display().to_string(),
                            detail: "Saved meeting markdown".into(),
                        }),
                    );
                    eprintln!(
                        "Saved: {} ({} words)",
                        result.path.display(),
                        result.word_count
                    );
                }
                Err(e) => {
                    if let Some(saved) = preserve_failed_capture(&wav_path, &config) {
                        set_latest_output(
                            &latest_output,
                            Some(OutputNotice {
                                kind: "preserved-capture".into(),
                                title: "Raw capture preserved".into(),
                                path: saved.display().to_string(),
                                detail:
                                    "Processing failed, but the raw audio capture was preserved."
                                        .into(),
                            }),
                        );
                        eprintln!(
                            "Pipeline error: {}. Raw audio preserved at {}",
                            e,
                            saved.display()
                        );
                    } else {
                        eprintln!(
                            "Pipeline error: {}. Raw audio left at {}",
                            e,
                            wav_path.display()
                        );
                    }
                }
            }
        }
        Err(e) => {
            recording.store(false, Ordering::Relaxed);
            if let Some(saved) = preserve_failed_capture(&wav_path, &config) {
                set_latest_output(
                    &latest_output,
                    Some(OutputNotice {
                        kind: "preserved-capture".into(),
                        title: "Partial capture preserved".into(),
                        path: saved.display().to_string(),
                        detail: "Recording failed before processing, but the captured audio was preserved.".into(),
                    }),
                );
                eprintln!(
                    "Capture error: {}. Partial audio preserved at {}",
                    e,
                    saved.display()
                );
            } else {
                eprintln!("Capture error: {}", e);
            }
        }
    }

    minutes_core::notes::cleanup();
    minutes_core::pid::remove().ok();
    if remove_current_wav && wav_path.exists() {
        std::fs::remove_file(&wav_path).ok();
    }
    processing.store(false, Ordering::Relaxed);
    set_processing_stage(&processing_stage, None);
    recording.store(false, Ordering::Relaxed);
}

#[tauri::command]
pub fn cmd_start_recording(
    app: tauri::AppHandle,
    state: tauri::State<AppState>,
) -> Result<(), String> {
    if recording_active(&state.recording) {
        return Err("Already recording".into());
    }
    state.recording.store(true, Ordering::Relaxed);
    let rec = state.recording.clone();
    let stop = state.stop_flag.clone();
    let processing = state.processing.clone();
    let processing_stage = state.processing_stage.clone();
    let latest_output = state.latest_output.clone();
    crate::update_tray_state(&app, true);
    let app_done = app.clone();
    std::thread::spawn(move || {
        start_recording(app, rec, stop, processing, processing_stage, latest_output);
        crate::update_tray_state(&app_done, false);
    });
    Ok(())
}

#[tauri::command]
pub fn cmd_stop_recording(state: tauri::State<AppState>) -> Result<(), String> {
    request_stop(&state.recording, &state.stop_flag)
}

#[tauri::command]
pub fn cmd_add_note(text: String) -> Result<String, String> {
    minutes_core::notes::add_note(&text)
}

#[tauri::command]
pub fn cmd_status(state: tauri::State<AppState>) -> serde_json::Value {
    let recording = state.recording.load(Ordering::Relaxed);
    let processing = state.processing.load(Ordering::Relaxed);
    let status = minutes_core::pid::status();
    let processing_stage = state
        .processing_stage
        .lock()
        .ok()
        .and_then(|stage| stage.clone());
    let latest_output = state
        .latest_output
        .lock()
        .ok()
        .and_then(|notice| notice.clone());

    // Get elapsed time if recording
    let elapsed = if recording || (status.recording && !processing) {
        let start_path = minutes_core::notes::recording_start_path();
        if start_path.exists() {
            if let Ok(s) = std::fs::read_to_string(&start_path) {
                if let Ok(start) = s.trim().parse::<u64>() {
                    let now = std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap_or_default()
                        .as_secs();
                    let e = now.saturating_sub(start);
                    Some(format!("{}:{:02}", e / 60, e % 60))
                } else {
                    None
                }
            } else {
                None
            }
        } else {
            None
        }
    } else {
        None
    };

    let audio_level = if recording || (status.recording && !processing) {
        minutes_core::capture::audio_level()
    } else {
        0
    };

    serde_json::json!({
        "recording": recording || (status.recording && !processing),
        "processing": processing,
        "processingStage": processing_stage,
        "latestOutput": latest_output,
        "pid": status.pid,
        "elapsed": elapsed,
        "audioLevel": audio_level,
    })
}

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
            let limited: Vec<_> = results.into_iter().take(limit.unwrap_or(20)).collect();
            serde_json::to_value(&limited).unwrap_or(serde_json::json!([]))
        }
        Err(_) => serde_json::json!([]),
    }
}

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

#[tauri::command]
pub fn cmd_open_file(path: String) -> Result<(), String> {
    std::process::Command::new("open")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn cmd_clear_latest_output(state: tauri::State<AppState>) {
    set_latest_output(&state.latest_output, None);
}

#[tauri::command]
pub fn cmd_recovery_items() -> serde_json::Value {
    let config = Config::load();
    serde_json::to_value(scan_recovery_items(&config)).unwrap_or(serde_json::json!([]))
}

#[tauri::command]
pub fn cmd_retry_recovery(
    state: tauri::State<AppState>,
    path: String,
    content_type: String,
) -> Result<OutputNotice, String> {
    if recording_active(&state.recording) || state.processing.load(Ordering::Relaxed) {
        return Err("Finish the current recording before retrying recovery items.".into());
    }

    let config = Config::load();
    let audio_path = PathBuf::from(&path);
    if !audio_path.exists() {
        return Err(format!("Recovery item not found: {}", path));
    }

    let ct = match content_type.as_str() {
        "meeting" => ContentType::Meeting,
        "memo" => ContentType::Memo,
        other => return Err(format!("Unsupported recovery type: {}", other)),
    };

    let result =
        minutes_core::pipeline::process_with_progress(&audio_path, ct, None, &config, |_| {})
            .map_err(|e| e.to_string())?;

    let notice = OutputNotice {
        kind: "saved".into(),
        title: result.title.clone(),
        path: result.path.display().to_string(),
        detail: "Recovery item was processed successfully.".into(),
    };
    set_latest_output(&state.latest_output, Some(notice.clone()));
    Ok(notice)
}

#[tauri::command]
pub fn cmd_get_meeting_detail(path: String) -> Result<MeetingDetail, String> {
    let config = Config::load();
    let meeting_path = std::path::PathBuf::from(&path);
    minutes_core::notes::validate_meeting_path(&meeting_path, &config.output_dir)?;

    let content = std::fs::read_to_string(&meeting_path).map_err(|e| e.to_string())?;
    let (frontmatter_str, body) = minutes_core::markdown::split_frontmatter(&content);
    let frontmatter: minutes_core::markdown::Frontmatter =
        serde_yaml::from_str(&format!("---\n{}\n---", frontmatter_str))
            .map_err(|e| e.to_string())?;

    let content_type = match frontmatter.r#type {
        ContentType::Meeting => "meeting",
        ContentType::Memo => "memo",
    }
    .to_string();

    let status = frontmatter.status.map(|status| {
        match status {
            minutes_core::markdown::OutputStatus::Complete => "complete",
            minutes_core::markdown::OutputStatus::NoSpeech => "no-speech",
            minutes_core::markdown::OutputStatus::TranscriptOnly => "transcript-only",
        }
        .to_string()
    });

    Ok(MeetingDetail {
        path,
        title: frontmatter.title,
        date: frontmatter.date.to_rfc3339(),
        duration: frontmatter.duration,
        content_type,
        status,
        context: frontmatter.context,
        attendees: frontmatter.attendees,
        calendar_event: frontmatter.calendar_event,
        sections: parse_sections(body),
    })
}

#[tauri::command]
pub fn cmd_upcoming_meetings() -> serde_json::Value {
    let events = minutes_core::calendar::upcoming_events(120); // 2 hour lookahead
    serde_json::to_value(&events).unwrap_or(serde_json::json!([]))
}

#[tauri::command]
pub fn cmd_needs_setup() -> serde_json::Value {
    let config = Config::load();
    let model_name = &config.transcription.model;
    let model_dir = &config.transcription.model_path;
    let model_file = model_dir.join(format!("ggml-{}.bin", model_name));
    let has_model = model_file.exists();

    let meetings_dir = config.output_dir.clone();
    let has_meetings_dir = meetings_dir.exists();

    serde_json::json!({
        "needsSetup": !has_model,
        "hasModel": has_model,
        "modelName": model_name,
        "hasMeetingsDir": has_meetings_dir,
    })
}

#[tauri::command]
pub fn cmd_download_model(model: String) -> Result<String, String> {
    let config = Config::load();
    let model_dir = &config.transcription.model_path;
    let model_file = model_dir.join(format!("ggml-{}.bin", model));

    if model_file.exists() {
        return Ok(format!("Model '{}' already downloaded", model));
    }

    std::fs::create_dir_all(model_dir).map_err(|e| e.to_string())?;

    let url = format!(
        "https://huggingface.co/ggerganov/whisper.cpp/resolve/main/ggml-{}.bin",
        model
    );

    eprintln!("[minutes] Downloading model: {} from {}", model, url);

    let status = std::process::Command::new("curl")
        .args([
            "-L",
            "-o",
            &model_file.to_string_lossy(),
            &url,
            "--progress-bar",
        ])
        .status()
        .map_err(|e| format!("curl failed: {}", e))?;

    if !status.success() {
        return Err("Download failed".into());
    }

    let size = std::fs::metadata(&model_file)
        .map(|m| m.len() / (1024 * 1024))
        .unwrap_or(0);

    Ok(format!("Downloaded '{}' model ({} MB)", model, size))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn preserve_failed_capture_moves_audio_into_failed_captures() {
        let dir = TempDir::new().unwrap();
        let config = Config {
            output_dir: dir.path().join("meetings"),
            ..Config::default()
        };
        let wav = dir.path().join("current.wav");
        std::fs::write(&wav, vec![1_u8; 256]).unwrap();

        let preserved = preserve_failed_capture(&wav, &config).unwrap();

        assert!(!wav.exists());
        assert!(preserved.exists());
        assert!(preserved.starts_with(config.output_dir.join("failed-captures")));
    }

    #[test]
    fn wait_for_path_removal_returns_false_after_timeout() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("still-there.pid");
        std::fs::write(&path, "123").unwrap();

        let removed = wait_for_path_removal(&path, Some(std::time::Duration::from_millis(50)));

        assert!(!removed);
        assert!(path.exists());
    }

    #[test]
    fn wait_for_path_removal_returns_true_when_file_disappears() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("gone-soon.pid");
        std::fs::write(&path, "123").unwrap();

        let path_for_thread = path.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(50));
            std::fs::remove_file(path_for_thread).unwrap();
        });

        let removed = wait_for_path_removal(&path, Some(std::time::Duration::from_secs(1)));

        assert!(removed);
        assert!(!path.exists());
    }

    #[test]
    fn stage_label_maps_pipeline_stage_to_user_facing_copy() {
        assert_eq!(
            stage_label(minutes_core::pipeline::PipelineStage::Transcribing),
            "Transcribing audio"
        );
        assert_eq!(
            stage_label(minutes_core::pipeline::PipelineStage::Saving),
            "Saving meeting"
        );
    }

    #[test]
    fn set_latest_output_replaces_previous_notice() {
        let latest_output = Arc::new(Mutex::new(None));
        set_latest_output(
            &latest_output,
            Some(OutputNotice {
                kind: "saved".into(),
                title: "Demo".into(),
                path: "/tmp/demo.md".into(),
                detail: "Saved".into(),
            }),
        );

        let current = latest_output.lock().unwrap().clone().unwrap();
        assert_eq!(current.title, "Demo");
        assert_eq!(current.path, "/tmp/demo.md");
    }

    #[test]
    fn scan_recovery_items_finds_failed_capture_and_watch_file() {
        let dir = tempfile::TempDir::new().unwrap();
        let watch_dir = dir.path().join("watch");
        let failed_dir = watch_dir.join("failed");
        let output_dir = dir.path().join("meetings");
        let failed_captures = output_dir.join("failed-captures");
        std::fs::create_dir_all(&failed_dir).unwrap();
        std::fs::create_dir_all(&failed_captures).unwrap();

        let failed_watch = failed_dir.join("idea.m4a");
        let failed_capture = failed_captures.join("capture.wav");
        std::fs::write(&failed_watch, "watch").unwrap();
        std::fs::write(&failed_capture, "capture").unwrap();

        let config = Config {
            output_dir: output_dir.clone(),
            watch: minutes_core::config::WatchConfig {
                paths: vec![watch_dir],
                ..Config::default().watch
            },
            ..Config::default()
        };

        let items = scan_recovery_items(&config);
        assert_eq!(items.len(), 2);
        assert!(items.iter().any(|item| item.kind == "watch-failed"));
        assert!(items.iter().any(|item| item.kind == "preserved-capture"));
    }

    #[test]
    fn parse_sections_preserves_top_level_order() {
        let body = "## Summary\n\nHello\n\n## Notes\n\n- One\n\n## Transcript\n\n[0:00] Hi\n";
        let sections = parse_sections(body);

        assert_eq!(sections.len(), 3);
        assert_eq!(sections[0].heading, "Summary");
        assert_eq!(sections[1].heading, "Notes");
        assert_eq!(sections[2].heading, "Transcript");
        assert!(sections[2].content.contains("[0:00] Hi"));
    }
}
