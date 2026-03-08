use std::path::Path;
use std::process::Command;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::Emitter;

use livemux_core::batch::{self, BatchConfig, BatchProgress};
use livemux_core::exiftool::ExifTool;
use livemux_core::ffmpeg;
use livemux_core::muxer::{Muxer, MuxerConfig};

const IMAGE_EXTENSIONS: &[&str] = &["heic", "heif", "avif", "jpg", "jpeg"];

pub struct AppState {
    pub exiftool: Arc<Mutex<ExifTool>>,
}

#[derive(Clone, Serialize)]
struct PhaseEvent {
    phase: String, // "scanning" | "processing"
    total: Option<usize>,
}

#[derive(Clone, Serialize)]
struct AdbPushProgress {
    current: usize,
    total: usize,
    file: String,
    status: String, // "pushing" | "done" | "error"
    error: Option<String>,
}

#[tauri::command]
fn check_exiftool() -> Result<String, String> {
    ExifTool::spawn()
        .map(|_| "ok".into())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn check_ffmpeg() -> bool {
    ffmpeg::check_ffmpeg()
}

#[tauri::command]
fn check_adb() -> bool {
    let output = Command::new("adb").arg("devices").output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            // "adb devices" output: first line is header, subsequent lines are devices
            // A connected device line looks like: "SERIAL\tdevice"
            stdout
                .lines()
                .skip(1)
                .any(|line| line.contains("\tdevice"))
        }
        Err(_) => false,
    }
}

#[tauri::command]
fn count_image_files(directory: String) -> Result<usize, String> {
    let entries = std::fs::read_dir(&directory)
        .map_err(|e| format!("Failed to read directory: {e}"))?;
    let count = entries
        .flatten()
        .filter(|entry| {
            let path = entry.path();
            path.is_file()
                && path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
                    .unwrap_or(false)
        })
        .count();
    Ok(count)
}

#[tauri::command]
async fn mux_single(
    state: tauri::State<'_, AppState>,
    config: MuxerConfig,
) -> Result<String, String> {
    let et = state.exiftool.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut et = et.lock().map_err(|e| e.to_string())?;
        let mut muxer = Muxer::new(config, &mut et).map_err(|e| e.to_string())?;
        muxer.mux().map_err(|e| e.to_string())?;
        Ok("ok".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn mux_directory(
    state: tauri::State<'_, AppState>,
    window: tauri::Window,
    config: BatchConfig,
) -> Result<String, String> {
    let _ = window.emit(
        "mux-phase",
        &PhaseEvent {
            phase: "scanning".into(),
            total: None,
        },
    );

    let et = state.exiftool.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let mut et = et.lock().map_err(|e| e.to_string())?;
        batch::mux_directory(&config, &mut et, |progress: BatchProgress| {
            let _ = window.emit("mux-progress", &progress);
        })
        .map_err(|e| e.to_string())?;
        Ok("ok".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn adb_push_directory(
    window: tauri::Window,
    source_dir: String,
    target_dir: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Create target directory on device
        let mkdir = Command::new("adb")
            .args(["shell", "mkdir", "-p", &target_dir])
            .output()
            .map_err(|e| format!("Failed to run adb: {e}"))?;

        if !mkdir.status.success() {
            return Err(format!(
                "Failed to create directory on device: {}",
                String::from_utf8_lossy(&mkdir.stderr)
            ));
        }

        // Emit scanning phase
        let _ = window.emit(
            "adb-push-phase",
            &PhaseEvent {
                phase: "scanning".into(),
                total: None,
            },
        );

        // Collect image files from source directory
        let source_path = Path::new(&source_dir);
        let mut files: Vec<std::path::PathBuf> = Vec::new();

        let entries = std::fs::read_dir(source_path)
            .map_err(|e| format!("Failed to read directory: {e}"))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if IMAGE_EXTENSIONS.contains(&ext.to_lowercase().as_str()) {
                        files.push(path);
                    }
                }
            }
        }

        files.sort();
        let total = files.len();

        for (i, file) in files.iter().enumerate() {
            let file_str = file.to_string_lossy();

            // Emit "pushing" before starting transfer
            let _ = window.emit(
                "adb-push-progress",
                &AdbPushProgress {
                    current: i + 1,
                    total,
                    file: file_str.to_string(),
                    status: "pushing".into(),
                    error: None,
                },
            );

            let result = Command::new("adb")
                .args(["push", &file_str, &format!("{}/", target_dir)])
                .output();

            let (status, error) = match result {
                Ok(out) if out.status.success() => {
                    // Trigger Android MediaStore scanner so the file is indexed
                    let file_name = file.file_name().unwrap_or_default().to_string_lossy();
                    let device_path = format!(
                        "file://{}{}",
                        target_dir.trim_end_matches('/'),
                        if target_dir.ends_with('/') { "" } else { "/" }
                    );
                    let _ = Command::new("adb")
                        .args([
                            "shell",
                            "am",
                            "broadcast",
                            "-a",
                            "android.intent.action.MEDIA_SCANNER_SCAN_FILE",
                            "-d",
                            &format!("{}{}", device_path, file_name),
                        ])
                        .output();
                    ("done".to_string(), None)
                }
                Ok(out) => (
                    "error".to_string(),
                    Some(String::from_utf8_lossy(&out.stderr).trim().to_string()),
                ),
                Err(e) => ("error".to_string(), Some(e.to_string())),
            };

            let _ = window.emit(
                "adb-push-progress",
                &AdbPushProgress {
                    current: i + 1,
                    total,
                    file: file_str.to_string(),
                    status,
                    error,
                },
            );
        }

        Ok("ok".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let et = ExifTool::spawn().expect("Failed to start ExifTool. Is exiftool installed?");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(AppState {
            exiftool: Arc::new(Mutex::new(et)),
        })
        .invoke_handler(tauri::generate_handler![
            check_exiftool,
            check_ffmpeg,
            check_adb,
            count_image_files,
            mux_single,
            mux_directory,
            adb_push_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
