use std::fs::File;
use std::io::Read;
use std::path::Path;
use std::process::Command;

use tracing::{debug, info};

use crate::error::{LiveMuxError, Result};

/// Known MP4-compatible ftyp major brands.
const MP4_BRANDS: &[&[u8; 4]] = &[
    b"isom", b"iso2", b"mp41", b"mp42", b"mp71", b"avc1", b"mmp4", b"msnv",
    b"ndas", b"ndsc", b"ndsh", b"ndsm", b"ndsp", b"ndss", b"ndxc", b"ndxh",
    b"ndxm", b"ndxp", b"ndxs", b"M4V ", b"M4A ",
];

/// Check if ffmpeg is available on the system.
pub fn check_ffmpeg() -> bool {
    Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Check if the video needs remuxing to MP4.
///
/// Returns true if the video uses a QuickTime-only ftyp brand (e.g. `qt  `)
/// that is not recognized by Google Motion Photo readers.
pub fn needs_remux(video_path: &Path) -> bool {
    let mut buf = [0u8; 12];
    let Ok(mut f) = File::open(video_path) else {
        return false;
    };
    if f.read_exact(&mut buf).is_err() {
        return false;
    }

    // ftyp box: bytes 4..8 = "ftyp", bytes 8..12 = major brand
    if &buf[4..8] != b"ftyp" {
        return false;
    }

    let brand = &buf[8..12];
    let is_mp4 = MP4_BRANDS.iter().any(|b| brand == b.as_slice());
    if !is_mp4 {
        debug!(
            "Video ftyp brand {:?} is not MP4-compatible, remux needed",
            std::str::from_utf8(brand).unwrap_or("????")
        );
    }
    !is_mp4
}

/// Remux a video file to MP4 format using ffmpeg.
///
/// Only copies video and audio streams (no re-encoding), strips auxiliary
/// tracks (depth maps, segmentation mattes, etc.) that iPhone MOVs contain.
pub fn remux_to_mp4(input: &Path, output: &Path) -> Result<()> {
    info!("Remuxing video to MP4: {}", input.display());

    let status = Command::new("ffmpeg")
        .args([
            "-y",
            "-i",
            &input.to_string_lossy(),
            "-c",
            "copy",
            "-map",
            "0:v:0",
            "-map",
            "0:a:0?",
            "-movflags",
            "+faststart",
            &output.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map_err(|e| LiveMuxError::FFmpeg {
            message: format!("Failed to run ffmpeg: {e}"),
        })?;

    if !status.success() {
        return Err(LiveMuxError::FFmpeg {
            message: format!("ffmpeg exited with status {}", status),
        });
    }

    debug!("Remuxed to: {}", output.display());
    Ok(())
}
