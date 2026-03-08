use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;

use serde::Serialize;
use tauri::Emitter;
use tauri::Manager;

use livemux_core::batch::{self, BatchConfig, BatchProgress};
use livemux_core::exiftool::ExifTool;
use livemux_core::ffmpeg;
use livemux_core::muxer::{Muxer, MuxerConfig};

const IMAGE_EXTENSIONS: &[&str] = &["heic", "heif", "avif", "jpg", "jpeg"];

/// Shared state holding the resolved path to the bundled exiftool directory.
pub struct AppState {
    pub exiftool_dir: Option<Arc<PathBuf>>,
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

fn spawn_exiftool(state: &AppState) -> Result<ExifTool, String> {
    let dir = state.exiftool_dir.as_deref().map(|p| p.as_path());
    ExifTool::spawn_with_path(dir).map_err(|e| e.to_string())
}

#[tauri::command]
fn check_exiftool(state: tauri::State<'_, AppState>) -> Result<String, String> {
    spawn_exiftool(&state).map(|_| "ok".into())
}

#[tauri::command]
fn check_ffmpeg() -> bool {
    ffmpeg::check_ffmpeg()
}

#[derive(Clone, Serialize)]
struct AdbDevice {
    serial: String,
    model: String,
}

#[tauri::command]
fn list_adb_devices() -> Vec<AdbDevice> {
    let output = Command::new("adb").arg("devices").arg("-l").output();
    match output {
        Ok(out) => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            stdout
                .lines()
                .skip(1)
                .filter(|line| line.contains(" device ") || line.ends_with("\tdevice"))
                .map(|line| {
                    let serial = line.split_whitespace().next().unwrap_or("").to_string();
                    let model = line
                        .split_whitespace()
                        .find_map(|tok| tok.strip_prefix("model:"))
                        .unwrap_or(&serial)
                        .to_string();
                    AdbDevice { serial, model }
                })
                .collect()
        }
        Err(_) => vec![],
    }
}

#[derive(Clone, Serialize)]
struct DeviceDir {
    name: String,
    has_children: bool,
}

#[tauri::command]
fn list_device_dirs(serial: String, path: String) -> Result<Vec<DeviceDir>, String> {
    let output = Command::new("adb")
        .args(["-s", &serial, "shell", &format!(
            "ls -1p {} 2>/dev/null", shell_escape(&path)
        )])
        .output()
        .map_err(|e| format!("Failed to run adb: {e}"))?;

    if !output.status.success() {
        return Err("Failed to list directory".into());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let dirs: Vec<DeviceDir> = stdout
        .lines()
        .filter(|line| line.ends_with('/'))
        .map(|line| {
            let name = line.trim_end_matches('/').to_string();
            DeviceDir {
                name,
                has_children: true,
            }
        })
        .collect();
    Ok(dirs)
}

/// Minimal shell escaping for adb shell arguments.
fn shell_escape(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
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
    let et_dir = state.exiftool_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = et_dir.as_deref().map(|p| p.as_path());
        let mut et = ExifTool::spawn_with_path(dir).map_err(|e| e.to_string())?;
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

    let et_dir = state.exiftool_dir.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let dir = et_dir.as_deref().map(|p| p.as_path());
        let mut et = ExifTool::spawn_with_path(dir).map_err(|e| e.to_string())?;
        let window2 = window.clone();
        batch::mux_directory(
            &config,
            &mut et,
            |total| {
                let _ = window2.emit(
                    "mux-phase",
                    &PhaseEvent {
                        phase: "processing".into(),
                        total: Some(total),
                    },
                );
            },
            |progress: BatchProgress| {
                let _ = window.emit("mux-progress", &progress);
            },
        )
        .map_err(|e| e.to_string())?;
        Ok("ok".into())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[tauri::command]
async fn adb_push_directory(
    window: tauri::Window,
    serial: String,
    source_dir: String,
    target_dir: String,
) -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        // Create target directory on device
        let mkdir = Command::new("adb")
            .args(["-s", &serial, "shell", "mkdir", "-p", &target_dir])
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

        // Collect all files from source directory
        let source_path = Path::new(&source_dir);
        let mut files: Vec<PathBuf> = Vec::new();

        let entries = std::fs::read_dir(source_path)
            .map_err(|e| format!("Failed to read directory: {e}"))?;

        for entry in entries.flatten() {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if path.file_name().map_or(false, |n| n.to_string_lossy().starts_with('.')) {
                continue;
            }
            files.push(path);
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
                .args(["-s", &serial, "push", &file_str, &format!("{}/", target_dir)])
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
                            "-s", &serial,
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
    // Set up tracing for debug logging (visible in cargo tauri dev console)
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
        )
        .with_target(false)
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            // Resolve bundled exiftool resource path
            let resource_dir = app.path().resource_dir().ok();
            let exiftool_dir = resource_dir.and_then(|dir| {
                // Windows: check for exiftool.exe
                let win_dir = dir.join("resources").join("exiftool-win");
                if win_dir.join("exiftool.exe").exists() {
                    return Some(win_dir);
                }
                // macOS/Linux: check for Perl script
                let unix_dir = dir.join("resources").join("exiftool");
                if unix_dir.join("exiftool").exists() {
                    return Some(unix_dir);
                }
                None
            });

            app.manage(AppState {
                exiftool_dir: exiftool_dir.map(|p| Arc::new(p)),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            check_exiftool,
            check_ffmpeg,
            list_adb_devices,
            list_device_dirs,
            count_image_files,
            mux_single,
            mux_directory,
            adb_push_directory,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
