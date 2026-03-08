use std::fs;
use std::path::{Path, PathBuf};

use tracing::{debug, info, warn};

use crate::error::{LiveMuxError, Result};
use crate::exiftool::ExifTool;
use crate::ffmpeg;
use crate::samsung_tags::{ImageType, SamsungTags};
use crate::utils::{enrich_fname, extract_track_duration, extract_track_number};
use crate::xmp::XmpDocument;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
pub struct MuxerConfig {
    pub image_path: PathBuf,
    pub video_path: PathBuf,
    pub output_path: Option<PathBuf>,
    pub output_directory: Option<PathBuf>,
    pub delete_video: bool,
    pub delete_temp: bool,
    pub overwrite: bool,
    pub no_xmp: bool,
}

pub struct Muxer<'et> {
    config: MuxerConfig,
    exiftool: &'et mut ExifTool,
    output_fpath: PathBuf,
    org_outfpath: PathBuf,
    xmp: XmpDocument,
}

impl<'et> Muxer<'et> {
    pub fn new(config: MuxerConfig, exiftool: &'et mut ExifTool) -> Result<Self> {
        let image_path = config.image_path.canonicalize().map_err(|_| {
            LiveMuxError::InvalidFile(format!(
                "Image file doesn't exist: {}",
                config.image_path.display()
            ))
        })?;
        let video_path = config.video_path.canonicalize().map_err(|_| {
            LiveMuxError::InvalidFile(format!(
                "Video file doesn't exist: {}",
                config.video_path.display()
            ))
        })?;

        if let Some(ref out_dir) = config.output_directory {
            if !out_dir.exists() {
                return Err(LiveMuxError::InvalidFile(
                    "Output directory doesn't exist".into(),
                ));
            }
        }

        if config.overwrite && config.output_path.is_some() {
            return Err(LiveMuxError::ArgConflict(
                "--overwrite cannot be used with --output".into(),
            ));
        }

        if config.output_path.is_some() && config.output_directory.is_some() {
            return Err(LiveMuxError::ArgConflict(
                "--output cannot be used with --output-dir".into(),
            ));
        }

        if config.overwrite || config.delete_video {
            warn!("Make sure to have a backup of your image and/or video file");
        }

        // Resolve output path
        let output_fpath = if let Some(ref out_dir) = config.output_directory {
            out_dir.join(image_path.file_name().unwrap())
        } else if config.overwrite {
            image_path.clone()
        } else if let Some(ref out) = config.output_path {
            out.clone()
        } else {
            enrich_fname(&image_path, "LIVE")
        };

        let org_outfpath = output_fpath.clone();
        let xmp = XmpDocument::from_template()?;

        Ok(Self {
            config: MuxerConfig {
                image_path,
                video_path,
                ..config
            },
            exiftool,
            output_fpath,
            org_outfpath,
            xmp,
        })
    }

    pub fn mux(&mut self) -> Result<()> {
        info!("Processing {}", self.config.image_path.display());

        // Step 1: Get metadata
        info!("[mux] Step 1: Reading metadata...");
        let metadata = self.exiftool.get_metadata(&[
            self.config.image_path.as_path(),
            self.config.video_path.as_path(),
        ])?;
        info!("[mux] Step 1: Done");
        let image_metadata = metadata.first().cloned().unwrap_or_default();
        let video_metadata = metadata.get(1).cloned().unwrap_or_default();

        // Step 2: Validate and determine types
        let image_type = self.validate_image(&image_metadata)?;
        self.fix_output_extension(&image_metadata);
        self.validate_video(&video_metadata)?;

        // Step 2.5: Remux MOV to MP4 if needed (strips auxiliary tracks, fixes ftyp brand)
        let remuxed_video: Option<PathBuf>;
        let effective_video_path: PathBuf;
        if ffmpeg::needs_remux(&self.config.video_path) {
            if ffmpeg::check_ffmpeg() {
                info!("[mux] Step 2.5: Remuxing MOV to MP4...");
                let mp4_path = enrich_fname(&self.config.video_path, "REMUX")
                    .with_extension("mp4");
                ffmpeg::remux_to_mp4(&self.config.video_path, &mp4_path)?;
                info!("[mux] Step 2.5: Remuxed to {}", mp4_path.display());
                self.xmp.set_motionphoto_mime("video/mp4")?;
                effective_video_path = mp4_path.clone();
                remuxed_video = Some(mp4_path);
            } else {
                warn!(
                    "ffmpeg not found. MOV video will be used as-is. \
                     Playback may not work on Google Photos."
                );
                effective_video_path = self.config.video_path.clone();
                remuxed_video = None;
            }
        } else {
            effective_video_path = self.config.video_path.clone();
            remuxed_video = None;
        }

        if self.config.no_xmp {
            // Skip XMP — just read image and build trailer
            let video_data = fs::read(&effective_video_path)?;
            let mut samsung_tail = SamsungTags::new(video_data, image_type);
            let mut merged_bytes = fs::read(&self.config.image_path)?;
            samsung_tail.set_image_size(merged_bytes.len());
            let footer = samsung_tail.video_footer();
            merged_bytes.extend_from_slice(&footer);
            self.write_output(&merged_bytes)?;
            self.cleanup_remuxed(&remuxed_video);
            return Ok(());
        }

        // Step 3: Extract Live Photo keyframe timestamp (from original video, not remuxed)
        info!("[mux] Step 3: Reading QuickTime tracks...");
        let track_xml = self.exiftool.quicktime_tracks(&self.config.video_path)?;
        info!("[mux] Step 3: Done");
        if let Some(track_num) = extract_track_number(&track_xml) {
            info!("Live Photo keyframe track number: {}", track_num);
            if let Some(duration_us) = extract_track_duration(&track_num, &track_xml) {
                info!("Live Photo keyframe: {}us", duration_us);
                let us: i64 = duration_us.parse().unwrap_or(-1);
                self.xmp.set_timestamp(us)?;
            }
        } else {
            info!("Could not read Live Photo keyframe. No keyframe will be set.");
        }

        // Step 4: Build SamsungTags
        info!("[mux] Step 4: Building Samsung tags...");
        let video_data = fs::read(&effective_video_path)?;
        let samsung_tail = SamsungTags::new(video_data, image_type);

        // Step 5: Merge source XMP
        info!("[mux] Step 5: Reading source XMP...");
        let source_xmp = self.exiftool.read_xmp(&self.config.image_path)?;
        info!("[mux] Step 5: Done");
        if source_xmp.trim().is_empty() {
            warn!("XMP of original file is empty");
        } else if let Err(e) = self.xmp.merge_source_xmp(&source_xmp) {
            warn!("Could not copy XMP metadata from source: {}", e);
        }

        // Step 6: Set video size and image padding in XMP
        let video_size = samsung_tail.get_video_size();
        let image_padding = samsung_tail.get_image_padding();
        self.xmp.set_motionphoto_length(video_size)?;
        self.xmp.set_primary_padding(image_padding)?;

        // Step 7: Write XMP sidecar (e.g. "IMG.LIVE.HEIC.XMP")
        info!("[mux] Step 7: Writing XMP sidecar...");
        let xmp_sidecar = PathBuf::from(format!("{}.XMP", self.output_fpath.display()));
        fs::write(&xmp_sidecar, self.xmp.to_bytes())?;

        // Step 8: Copy image and embed XMP
        info!("[mux] Step 8: Embedding XMP...");
        let xmp_image = enrich_fname(&self.output_fpath, "XMP");
        fs::copy(&self.config.image_path, &xmp_image)?;
        self.exiftool.embed_xmp(&xmp_sidecar, &xmp_image)?;
        info!("[mux] Step 8: Done");

        // Step 8.5: Remove Apple Live Photo metadata from the output
        info!("[mux] Step 8.5: Removing Apple MakerNotes...");
        self.exiftool.remove_apple_livephoto_tags(&xmp_image)?;
        info!("[mux] Step 8.5: Done");

        // Step 9: Read XMP-enriched image and finalize
        info!("[mux] Step 9: Building final output...");
        let mut merged_bytes = fs::read(&xmp_image)?;
        let mut samsung_tail = {
            let video_data = fs::read(&effective_video_path)?;
            SamsungTags::new(video_data, image_type)
        };
        samsung_tail.set_image_size(merged_bytes.len());
        let footer = samsung_tail.video_footer();
        merged_bytes.extend_from_slice(&footer);

        // Step 10: Write output
        self.write_output(&merged_bytes)?;

        // Cleanup temp files
        if self.config.delete_temp {
            let _ = fs::remove_file(&xmp_sidecar);
            debug!("Deleted: {}", xmp_sidecar.display());
            let _ = fs::remove_file(&xmp_image);
            debug!("Deleted: {}", xmp_image.display());
        }
        self.cleanup_remuxed(&remuxed_video);

        if self.config.delete_video {
            fs::remove_file(&self.config.video_path)?;
            debug!("Deleted: {}", self.config.video_path.display());
        }

        if self.config.overwrite && self.output_fpath != self.org_outfpath {
            fs::remove_file(&self.org_outfpath)?;
            debug!("Deleted: {}", self.org_outfpath.display());
        }

        Ok(())
    }

    fn validate_image(&mut self, metadata: &serde_json::Value) -> Result<ImageType> {
        let ext = self.get_effective_extension(&self.config.image_path, metadata);
        let ext_lower = ext.to_lowercase();

        if ["heic", "heif", "avif"].contains(&ext_lower.as_str()) {
            Ok(ImageType::Heic)
        } else {
            if !["jpg", "jpeg"].contains(&ext_lower.as_str()) {
                warn!("Image extension .{} not supported. Treating as JPG", ext);
            }
            self.xmp.set_primary_mime("image/jpeg")?;
            Ok(ImageType::Jpg)
        }
    }

    fn validate_video(&mut self, metadata: &serde_json::Value) -> Result<()> {
        let ext = self.get_effective_extension(&self.config.video_path, metadata);
        let ext_lower = ext.to_lowercase();

        if ext_lower == "mp4" {
            self.xmp.set_motionphoto_mime("video/mp4")?;
        } else if ext_lower != "mov" {
            warn!("Video extension .{} not supported. Treating as QuickTime MOV", ext);
        }
        Ok(())
    }

    fn get_effective_extension(&self, path: &Path, metadata: &serde_json::Value) -> String {
        // Prefer ExifTool's detected type over file extension
        if let Some(meta_ext) = metadata
            .get("File:FileTypeExtension")
            .and_then(|v| v.as_str())
        {
            let file_ext = path
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if file_ext != meta_ext.to_lowercase() {
                warn!(
                    "File extension .{} doesn't match metadata. Treating as .{}",
                    file_ext, meta_ext
                );
            }
            return meta_ext.to_lowercase();
        }
        path.extension()
            .unwrap_or_default()
            .to_string_lossy()
            .to_lowercase()
    }

    fn fix_output_extension(&mut self, metadata: &serde_json::Value) {
        if let Some(meta_ext) = metadata
            .get("File:FileTypeExtension")
            .and_then(|v| v.as_str())
        {
            let current_ext = self
                .output_fpath
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            if current_ext != meta_ext.to_lowercase() {
                warn!(
                    "Output extension .{} doesn't match input metadata .{}",
                    current_ext, meta_ext
                );
                self.output_fpath = self.output_fpath.with_extension(meta_ext.to_lowercase());
            }
        }
    }

    fn cleanup_remuxed(&self, remuxed_video: &Option<PathBuf>) {
        if let Some(ref path) = remuxed_video {
            if self.config.delete_temp {
                let _ = fs::remove_file(path);
                debug!("Deleted remuxed temp: {}", path.display());
            }
        }
    }

    fn write_output(&self, data: &[u8]) -> Result<()> {
        info!("Writing output file: {}", self.output_fpath.display());
        fs::write(&self.output_fpath, data)?;
        // Preserve timestamps from source image
        let src_meta = fs::metadata(&self.config.image_path)?;
        let mtime = filetime::FileTime::from_last_modification_time(&src_meta);
        let atime = filetime::FileTime::from_last_access_time(&src_meta);
        filetime::set_file_times(&self.output_fpath, atime, mtime)?;
        Ok(())
    }
}
