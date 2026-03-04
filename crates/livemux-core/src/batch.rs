use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};

use tracing::{error, info, warn};
use walkdir::WalkDir;

use crate::error::Result;
use crate::exiftool::ExifTool;
use crate::muxer::{Muxer, MuxerConfig};
use crate::utils;

const IMAGE_EXTENSIONS: &[&str] = &["heic", "heif", "avif", "jpg", "jpeg"];
const VIDEO_EXTENSIONS: &[&str] = &["mov", "mp4"];

pub fn is_image(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| IMAGE_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| VIDEO_EXTENSIONS.contains(&e.to_lowercase().as_str()))
        .unwrap_or(false)
}

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct BatchConfig {
    pub directory: PathBuf,
    pub output_dir: Option<PathBuf>,
    pub recursive: bool,
    pub exif_match: bool,
    pub incremental: bool,
    pub copy_unmuxed: bool,
    pub delete_video: bool,
    pub delete_temp: bool,
    pub overwrite: bool,
}

#[derive(Clone, serde::Serialize)]
pub struct BatchProgress {
    pub current: usize,
    pub total: usize,
    pub file: String,
    pub success: bool,
    pub error: Option<String>,
}

/// Run batch mux on a directory. Calls `on_progress` for each file processed.
pub fn mux_directory<F>(
    config: &BatchConfig,
    et: &mut ExifTool,
    mut on_progress: F,
) -> Result<()>
where
    F: FnMut(BatchProgress),
{
    if config.overwrite && config.output_dir.is_some() {
        return Err(crate::error::LiveMuxError::ArgConflict(
            "--overwrite cannot be used with --output-dir".into(),
        ));
    }

    if let Some(ref out_dir) = config.output_dir {
        fs::create_dir_all(out_dir)?;
    }

    let pairs = if config.exif_match {
        find_pairs_by_exif(&config.directory, config.recursive, et)?
    } else {
        find_pairs_by_filename(&config.directory, config.recursive)?
    };

    let total = pairs.len();
    info!("Found {} image/video pairs", total);

    let mut matched_images: HashSet<PathBuf> = HashSet::new();

    for (i, (image, video)) in pairs.iter().enumerate() {
        info!("==[{}/{}]", i + 1, total);
        matched_images.insert(image.clone());

        if config.incremental {
            let out_path = if let Some(ref out_dir) = config.output_dir {
                out_dir.join(image.file_name().unwrap())
            } else if config.overwrite {
                image.clone()
            } else {
                utils::enrich_fname(image, "LIVE")
            };

            if out_path.exists() && utils::binary_compare(video, &out_path) {
                info!("Skipping (already muxed): {}", image.display());
                on_progress(BatchProgress {
                    current: i + 1,
                    total,
                    file: image.display().to_string(),
                    success: true,
                    error: Some("Skipped (already muxed)".into()),
                });
                continue;
            }
        }

        let mux_config = MuxerConfig {
            image_path: image.clone(),
            video_path: video.clone(),
            output_path: None,
            output_directory: config.output_dir.clone(),
            delete_video: config.delete_video,
            delete_temp: config.delete_temp,
            overwrite: config.overwrite,
            no_xmp: false,
        };

        let (success, err_msg) = match Muxer::new(mux_config, et) {
            Ok(mut muxer) => match muxer.mux() {
                Ok(()) => (true, None),
                Err(e) => {
                    error!("Failed to mux {} + {}: {:#}", image.display(), video.display(), e);
                    (false, Some(e.to_string()))
                }
            },
            Err(e) => {
                error!("Failed to create muxer for {} + {}: {:#}", image.display(), video.display(), e);
                (false, Some(e.to_string()))
            }
        };

        on_progress(BatchProgress {
            current: i + 1,
            total,
            file: image.display().to_string(),
            success,
            error: err_msg,
        });
    }

    // Copy unmuxed images
    if config.copy_unmuxed {
        if let Some(ref out_dir) = config.output_dir {
            copy_unmatched_images(&config.directory, config.recursive, &matched_images, out_dir)?;
        } else {
            warn!("--copy-unmuxed requires --output-dir");
        }
    }

    Ok(())
}

/// Find image/video pairs by matching filenames (same stem).
pub fn find_pairs_by_filename(dir: &Path, recursive: bool) -> Result<Vec<(PathBuf, PathBuf)>> {
    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    let mut images: HashMap<String, PathBuf> = HashMap::new();
    let mut videos: HashMap<String, PathBuf> = HashMap::new();

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        let stem = path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase();

        if is_image(path) {
            images.insert(stem.clone(), path.to_path_buf());
        } else if is_video(path) {
            videos.insert(stem, path.to_path_buf());
        }
    }

    let mut pairs = Vec::new();
    for (stem, image_path) in &images {
        if let Some(video_path) = videos.get(stem) {
            pairs.push((image_path.clone(), video_path.clone()));
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

/// Find image/video pairs by matching EXIF ContentIdentifier.
pub fn find_pairs_by_exif(
    dir: &Path,
    recursive: bool,
    et: &mut ExifTool,
) -> Result<Vec<(PathBuf, PathBuf)>> {
    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    let mut all_files: Vec<PathBuf> = Vec::new();
    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && (is_image(path) || is_video(path)) {
            all_files.push(path.to_path_buf());
        }
    }

    if all_files.is_empty() {
        return Ok(Vec::new());
    }

    let path_refs: Vec<&Path> = all_files.iter().map(|p| p.as_path()).collect();
    let all_metadata = et.get_metadata(&path_refs)?;

    let mut image_by_id: HashMap<String, PathBuf> = HashMap::new();
    let mut video_by_id: HashMap<String, PathBuf> = HashMap::new();

    for (path, meta) in all_files.iter().zip(all_metadata.iter()) {
        if is_image(path) {
            if let Some(id) = utils::get_content_id(meta, "MakerNotes:ContentIdentifier") {
                image_by_id.insert(id, path.clone());
            }
        } else if is_video(path) {
            if let Some(id) = utils::get_content_id(meta, "QuickTime:ContentIdentifier") {
                video_by_id.insert(id, path.clone());
            }
        }
    }

    let mut pairs = Vec::new();
    for (id, image_path) in &image_by_id {
        if let Some(video_path) = video_by_id.get(id) {
            pairs.push((image_path.clone(), video_path.clone()));
        }
    }
    pairs.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(pairs)
}

fn copy_unmatched_images(
    dir: &Path,
    recursive: bool,
    matched: &HashSet<PathBuf>,
    output_dir: &Path,
) -> Result<()> {
    let walker = if recursive {
        WalkDir::new(dir)
    } else {
        WalkDir::new(dir).max_depth(1)
    };

    for entry in walker.into_iter().filter_map(|e| e.ok()) {
        let path = entry.path();
        if path.is_file() && is_image(path) && !matched.contains(path) {
            let dest = output_dir.join(path.file_name().unwrap());
            if !dest.exists() {
                info!("Copying unmuxed: {}", path.display());
                fs::copy(path, dest)?;
            }
        }
    }
    Ok(())
}
