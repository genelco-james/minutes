#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager,
};

mod commands;

fn main() {
    let recording = Arc::new(AtomicBool::new(false));
    let recording_clone = recording.clone();

    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_notification::init())
        .manage(commands::AppState {
            recording: recording.clone(),
        })
        .setup(move |app| {
            // Build tray menu
            let record_item = MenuItem::with_id(app, "record", "Start Recording", true, None::<&str>)?;
            let stop_item = MenuItem::with_id(app, "stop", "Stop Recording", true, None::<&str>)?;
            let separator = MenuItem::with_id(app, "sep1", "──────────", false, None::<&str>)?;
            let list_item = MenuItem::with_id(app, "list", "Recent Meetings", true, None::<&str>)?;
            let search_item = MenuItem::with_id(app, "search", "Search...", true, None::<&str>)?;
            let separator2 = MenuItem::with_id(app, "sep2", "──────────", false, None::<&str>)?;
            let setup_item = MenuItem::with_id(app, "setup", "Setup...", true, None::<&str>)?;
            let quit_item = MenuItem::with_id(app, "quit", "Quit Minutes", true, None::<&str>)?;

            let menu = Menu::with_items(
                app,
                &[
                    &record_item,
                    &stop_item,
                    &separator,
                    &list_item,
                    &search_item,
                    &separator2,
                    &setup_item,
                    &quit_item,
                ],
            )?;

            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Minutes — conversation memory")
                .on_menu_event(move |app, event| {
                    let recording = recording_clone.clone();
                    match event.id.as_ref() {
                        "record" => {
                            if recording.load(Ordering::Relaxed) {
                                eprintln!("Already recording");
                                return;
                            }
                            // Start recording in background thread
                            let app_handle = app.clone();
                            let rec = recording.clone();
                            std::thread::spawn(move || {
                                commands::start_recording(app_handle, rec);
                            });
                        }
                        "stop" => {
                            if !recording.load(Ordering::Relaxed) {
                                eprintln!("Not recording");
                                return;
                            }
                            recording.store(false, Ordering::Relaxed);
                        }
                        "list" => {
                            // Open meetings directory in Finder
                            let meetings_dir = dirs::home_dir()
                                .unwrap_or_default()
                                .join("meetings");
                            let _ = std::process::Command::new("open")
                                .arg(meetings_dir)
                                .spawn();
                        }
                        "quit" => {
                            if recording.load(Ordering::Relaxed) {
                                recording.store(false, Ordering::Relaxed);
                                std::thread::sleep(std::time::Duration::from_secs(2));
                            }
                            std::process::exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::cmd_status,
            commands::cmd_list_meetings,
            commands::cmd_search,
        ])
        .run(tauri::generate_context!())
        .expect("error while running minutes app");
}
