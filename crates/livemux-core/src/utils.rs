use std::path::{Path, PathBuf};

use regex::Regex;

use crate::constants::{NOT_VIDEO_SIGS, VIDEO_SIGS};
use crate::error::Result;
use crate::exiftool::ExifTool;

/// Extract the track number whose StillImageTime == -1 from ExifTool XML output.
pub fn extract_track_number(metadata: &str) -> Option<String> {
    let re = Regex::new(r"\s*<Track(\d+):StillImageTime>-1<").ok()?;
    let caps = re.captures(metadata)?;
    Some(caps[1].to_string())
}

/// Extract track duration in microseconds from ExifTool XML output.
pub fn extract_track_duration(track_number: &str, metadata: &str) -> Option<String> {
    let pattern = format!(
        r"\s*<Track{}:TrackDuration>(\d+\.?\d*)<",
        track_number
    );
    let re = Regex::new(&pattern).ok()?;
    let caps = re.captures(metadata)?;
    let duration_secs: f64 = caps[1].parse().ok()?;
    let microseconds = (duration_secs * 1_000_000.0).round() as i64;
    Some(microseconds.to_string())
}

/// Insert a label before the file extension: `IMG.HEIC` → `IMG.LIVE.HEIC`
pub fn enrich_fname(path: &Path, label: &str) -> PathBuf {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path.extension().unwrap_or_default().to_string_lossy();
    let new_name = if ext.is_empty() {
        format!("{}.{}", stem, label)
    } else {
        format!("{}.{}.{}", stem, label, ext)
    };
    path.with_file_name(new_name)
}

/// Check if bytes look like a video file (searching first 15 bytes for ftyp/wide signatures).
pub fn verify_video_in_image(data: &[u8]) -> bool {
    if data.len() < 15 {
        return false;
    }
    let header = &data[..15];

    let has_video_sig = VIDEO_SIGS.iter().any(|sig| {
        header.windows(sig.len()).any(|w| w == *sig)
    });
    if !has_video_sig {
        return false;
    }

    // Disqualify if it matches a non-video signature
    let is_not_video = NOT_VIDEO_SIGS.iter().any(|sig| {
        header.windows(sig.len()).any(|w| w == *sig)
    });
    !is_not_video
}

/// Check if a file is already a motion photo by trying to extract embedded video.
pub fn is_motion_photo(path: &Path, et: &mut ExifTool) -> Result<bool> {
    if let Some(video_data) = et.extract_embedded_video(path)? {
        return Ok(verify_video_in_image(&video_data));
    }
    Ok(false)
}

/// Binary compare: check if the video file's contents exist within the image file.
/// Uses memory-mapped files for efficiency with large files.
pub fn binary_compare(video_path: &Path, image_path: &Path) -> bool {
    if !video_path.exists() || !image_path.exists() {
        return false;
    }

    let result = (|| -> std::io::Result<bool> {
        let video_file = std::fs::File::open(video_path)?;
        let image_file = std::fs::File::open(image_path)?;

        let video_mmap = unsafe { memmap2::Mmap::map(&video_file)? };
        let image_mmap = unsafe { memmap2::Mmap::map(&image_file)? };

        Ok(image_mmap
            .windows(video_mmap.len())
            .any(|w| w == &video_mmap[..]))
    })();

    result.unwrap_or(false)
}

/// Extract the ContentIdentifier value from metadata if present.
pub fn get_content_id(metadata: &serde_json::Value, key: &str) -> Option<String> {
    metadata.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enrich_fname() {
        let p = Path::new("/photos/IMG_001.HEIC");
        assert_eq!(enrich_fname(p, "LIVE"), Path::new("/photos/IMG_001.LIVE.HEIC"));
    }

    #[test]
    fn test_enrich_fname_no_ext() {
        let p = Path::new("/photos/IMG_001");
        assert_eq!(enrich_fname(p, "LIVE"), Path::new("/photos/IMG_001.LIVE"));
    }

    #[test]
    fn test_verify_video_ftyp() {
        let mut data = vec![0u8; 20];
        data[4..8].copy_from_slice(b"ftyp");
        assert!(verify_video_in_image(&data));
    }

    #[test]
    fn test_verify_video_reject_ftypheic() {
        let mut data = vec![0u8; 20];
        data[4..12].copy_from_slice(b"ftypheic");
        assert!(!verify_video_in_image(&data));
    }

    #[test]
    fn test_verify_video_too_short() {
        assert!(!verify_video_in_image(&[0u8; 5]));
    }

    #[test]
    fn test_extract_track_number() {
        let xml = r#"  <Track2:StillImageTime>-1</Track2:StillImageTime>"#;
        assert_eq!(extract_track_number(xml), Some("2".to_string()));
    }

    #[test]
    fn test_extract_track_duration() {
        let xml = r#"  <Track2:TrackDuration>1.5</Track2:TrackDuration>"#;
        assert_eq!(
            extract_track_duration("2", xml),
            Some("1500000".to_string())
        );
    }

    #[test]
    fn test_extract_track_duration_integer() {
        let xml = r#"  <Track1:TrackDuration>3</Track1:TrackDuration>"#;
        assert_eq!(
            extract_track_duration("1", xml),
            Some("3000000".to_string())
        );
    }
}
