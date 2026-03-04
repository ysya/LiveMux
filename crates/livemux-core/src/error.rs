use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum LiveMuxError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("ExifTool error: {message}")]
    ExifTool { message: String },

    #[error("FFmpeg error: {message}")]
    FFmpeg { message: String },

    #[error("XMP parse error: {0}")]
    XmpParse(String),

    #[error("XMP element not found: {0}")]
    XmpElementMissing(String),

    #[error("Invalid file: {0}")]
    InvalidFile(String),

    #[error("Argument conflict: {0}")]
    ArgConflict(String),

    #[error("Regex error: {0}")]
    Regex(#[from] regex::Error),

    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),
}

// Serialize as the Display string — required for Tauri command error returns.
impl Serialize for LiveMuxError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, LiveMuxError>;
