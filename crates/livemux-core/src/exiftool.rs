use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

use tracing::info;

use crate::error::{LiveMuxError, Result};

/// Manages a persistent ExifTool subprocess using the `-stay_open` protocol.
///
/// Communication flow:
/// 1. Write args to stdin (one per line)
/// 2. Write `-execute{N}\n` as sentinel
/// 3. Read stdout until `{ready{N}}` sentinel line
pub struct ExifTool {
    process: Child,
    stdin: BufWriter<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    seq: u32,
}

impl ExifTool {
    /// Spawn a persistent ExifTool process using the system `exiftool`.
    pub fn spawn() -> Result<Self> {
        Self::spawn_with_path(None)
    }

    /// Spawn a persistent ExifTool process.
    ///
    /// If `bundled_dir` is provided, looks for the bundled exiftool script there first.
    /// Falls back to the system `exiftool` if the bundled version is not found.
    pub fn spawn_with_path(bundled_dir: Option<&Path>) -> Result<Self> {
        let (program, args) = Self::resolve_exiftool(bundled_dir);
        info!("Starting ExifTool: {} {:?}", program.display(), args);

        let mut cmd_args: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        cmd_args.extend_from_slice(&["-stay_open", "True", "-@", "-"]);

        let mut process = Command::new(&program)
            .args(&cmd_args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|e| LiveMuxError::ExifTool {
                message: format!("Failed to spawn exiftool: {e}. Is exiftool installed?"),
            })?;

        let stdin = BufWriter::new(
            process
                .stdin
                .take()
                .ok_or_else(|| LiveMuxError::ExifTool {
                    message: "Failed to open exiftool stdin".into(),
                })?,
        );
        let stdout = BufReader::new(
            process
                .stdout
                .take()
                .ok_or_else(|| LiveMuxError::ExifTool {
                    message: "Failed to open exiftool stdout".into(),
                })?,
        );

        Ok(Self {
            process,
            stdin,
            stdout,
            seq: 0,
        })
    }

    /// Resolve the exiftool executable path.
    ///
    /// Priority:
    /// 1. Bundled exiftool script in `bundled_dir` (run via `perl`)
    /// 2. System `exiftool` on PATH
    fn resolve_exiftool(bundled_dir: Option<&Path>) -> (PathBuf, Vec<String>) {
        if let Some(dir) = bundled_dir {
            let script = dir.join("exiftool");
            if script.exists() {
                // Use perl to run the bundled script
                return (PathBuf::from("perl"), vec![script.to_string_lossy().into_owned()]);
            }
        }
        // Fallback to system exiftool
        (PathBuf::from("exiftool"), vec![])
    }

    /// Send arbitrary args to ExifTool and return stdout as String.
    pub fn execute(&mut self, args: &[&str]) -> Result<String> {
        let bytes = self.execute_bytes(args)?;
        String::from_utf8(bytes).map_err(|e| LiveMuxError::ExifTool {
            message: format!("ExifTool output is not valid UTF-8: {e}"),
        })
    }

    /// Send args to ExifTool and return stdout as raw bytes.
    pub fn execute_bytes(&mut self, args: &[&str]) -> Result<Vec<u8>> {
        self.seq += 1;
        let sentinel = format!("{{ready{}}}", self.seq);
        let sentinel_bytes = sentinel.as_bytes();

        for arg in args {
            writeln!(self.stdin, "{}", arg)?;
        }
        writeln!(self.stdin, "-execute{}", self.seq)?;
        self.stdin.flush()?;

        let mut output: Vec<u8> = Vec::new();
        loop {
            let buf = self.stdout.fill_buf()?;
            if buf.is_empty() {
                return Err(LiveMuxError::ExifTool {
                    message: "ExifTool process terminated unexpectedly".into(),
                });
            }

            let buf_len = buf.len();
            output.extend_from_slice(buf);
            self.stdout.consume(buf_len);

            if let Some(pos) = find_subsequence(&output, sentinel_bytes) {
                output.truncate(pos);
                // Trim trailing newline/carriage-return before sentinel
                while output.last() == Some(&b'\n') || output.last() == Some(&b'\r') {
                    output.pop();
                }
                break;
            }
        }
        Ok(output)
    }

    /// Get JSON metadata for one or more files.
    pub fn get_metadata(&mut self, paths: &[&Path]) -> Result<Vec<serde_json::Value>> {
        let mut args: Vec<String> = vec!["-j".into(), "-n".into()];
        for p in paths {
            args.push(p.to_string_lossy().into_owned());
        }
        let arg_refs: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
        let raw = self.execute(&arg_refs)?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(raw.trim())?;
        Ok(parsed)
    }

    /// Read raw XMP block from image file.
    pub fn read_xmp(&mut self, path: &Path) -> Result<String> {
        let path_str = path.to_string_lossy().into_owned();
        let result = self.execute(&["-XMP", "-b", &path_str])?;
        Ok(result)
    }

    /// Write XMP from sidecar file into image (overwrite original).
    pub fn embed_xmp(&mut self, sidecar: &Path, image: &Path) -> Result<()> {
        let sidecar_str = sidecar.to_string_lossy().into_owned();
        let image_str = image.to_string_lossy().into_owned();
        self.execute(&["-overwrite_original", "-tagsfromfile", &sidecar_str, "-xmp", &image_str])?;
        Ok(())
    }

    /// Get QuickTime track analysis XML output.
    pub fn quicktime_tracks(&mut self, path: &Path) -> Result<String> {
        let path_str = path.to_string_lossy().into_owned();
        self.execute(&[
            "-X",
            "-ee",
            "-n",
            "-QuickTime:StillImageTime",
            "-QuickTime:TrackDuration",
            &path_str,
        ])
    }

    /// Remove Apple Live Photo metadata that may interfere with
    /// Google/Samsung motion photo detection.
    ///
    /// Apple's `LivePhotoVideoIndex` lives inside binary MakerNotes and cannot
    /// be removed individually. We strip the entire MakerNotes block instead.
    pub fn remove_apple_livephoto_tags(&mut self, image: &Path) -> Result<()> {
        let image_str = image.to_string_lossy().into_owned();
        let _ = self.execute(&[
            "-overwrite_original",
            "-MakerNotes:all=",
            &image_str,
        ]);
        Ok(())
    }

    /// Extract embedded video from a motion photo using ExifTool.
    pub fn extract_embedded_video(&mut self, path: &Path) -> Result<Option<Vec<u8>>> {
        let path_str = path.to_string_lossy().into_owned();

        // Try Google Camera headers first
        let result = self.execute_bytes(&["-b", "-MotionPhotoVideo", &path_str])?;
        if !result.is_empty() {
            return Ok(Some(result));
        }

        // Try Samsung headers
        let result = self.execute_bytes(&["-b", "-EmbeddedVideoFile", &path_str])?;
        if !result.is_empty() {
            return Ok(Some(result));
        }

        Ok(None)
    }
}

impl Drop for ExifTool {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "-stay_open");
        let _ = writeln!(self.stdin, "False");
        let _ = self.stdin.flush();
        let _ = self.process.wait();
    }
}

fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|w| w == needle)
}
