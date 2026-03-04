use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};

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
    /// Spawn a persistent ExifTool process.
    pub fn spawn() -> Result<Self> {
        let mut process = Command::new("exiftool")
            .args(["-stay_open", "True", "-@", "-"])
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

    /// Send arbitrary args to ExifTool and return stdout as String.
    pub fn execute(&mut self, args: &[&str]) -> Result<String> {
        self.seq += 1;
        let sentinel = format!("{{ready{}}}", self.seq);

        for arg in args {
            writeln!(self.stdin, "{}", arg)?;
        }
        writeln!(self.stdin, "-execute{}", self.seq)?;
        self.stdin.flush()?;

        let mut output = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let bytes_read = self.stdout.read_line(&mut line)?;
            if bytes_read == 0 {
                return Err(LiveMuxError::ExifTool {
                    message: "ExifTool process terminated unexpectedly".into(),
                });
            }
            if line.trim_end() == sentinel {
                break;
            }
            output.push_str(&line);
        }
        Ok(output)
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

            // Search for sentinel in buffer
            let buf_len = buf.len();
            output.extend_from_slice(buf);
            self.stdout.consume(buf_len);

            // Check if output ends with sentinel + newline
            if let Some(pos) = find_subsequence(&output, sentinel_bytes) {
                output.truncate(pos);
                // Trim trailing newline before sentinel
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
