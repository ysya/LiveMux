use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser, Debug)]
#[command(
    name = "livemux",
    about = "Mux HEIC/JPG Live Photos into Samsung/Google Motion Photos",
    version
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Delete source video after muxing
    #[arg(long, global = true)]
    pub delete_video: bool,

    /// Overwrite original image file
    #[arg(long, global = true)]
    pub overwrite: bool,

    /// Keep temporary files
    #[arg(long, global = true)]
    pub keep_temp: bool,

    /// Enable verbose output
    #[arg(long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Mux a single image+video pair into a Motion Photo
    Mux {
        /// Input image file (HEIC/JPG)
        #[arg(short, long)]
        image: PathBuf,

        /// Input video file (MOV/MP4)
        #[arg(short, long)]
        video: PathBuf,

        /// Output file path (default: <image>.LIVE.<ext>)
        #[arg(short, long)]
        output: Option<PathBuf>,

        /// Skip XMP metadata embedding
        #[arg(long)]
        no_xmp: bool,
    },

    /// Batch mux a directory of image+video pairs
    Dir {
        /// Input directory containing images and videos
        #[arg(short, long)]
        directory: PathBuf,

        /// Output directory (default: same as input)
        #[arg(short, long)]
        output_dir: Option<PathBuf>,

        /// Process subdirectories recursively
        #[arg(short, long)]
        recursive: bool,

        /// Match image/video pairs by EXIF ContentIdentifier instead of filename
        #[arg(long)]
        exif_match: bool,

        /// Skip files that are already muxed
        #[arg(long)]
        incremental: bool,

        /// Copy unmatched image files to output directory
        #[arg(long)]
        copy_unmuxed: bool,
    },
}
