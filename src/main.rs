mod cli;

use anyhow::{Context, Result};
use clap::Parser;
use tracing::info;

use livemux_core::batch::{self, BatchConfig};
use livemux_core::exiftool::ExifTool;
use livemux_core::muxer::{Muxer, MuxerConfig};

use cli::{Cli, Commands};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Set up tracing
    let level = if cli.verbose { "debug" } else { "info" };
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new(level)),
        )
        .with_target(false)
        .init();

    let mut et = ExifTool::spawn().context("Failed to start ExifTool")?;

    match cli.command {
        Commands::Mux {
            image,
            video,
            output,
            no_xmp,
        } => {
            let config = MuxerConfig {
                image_path: image,
                video_path: video,
                output_path: output,
                output_directory: None,
                delete_video: cli.delete_video,
                delete_temp: !cli.keep_temp,
                overwrite: cli.overwrite,
                no_xmp,
            };
            let mut muxer = Muxer::new(config, &mut et)?;
            muxer.mux()?;
        }
        Commands::Dir {
            directory,
            output_dir,
            recursive,
            exif_match,
            incremental,
            copy_unmuxed,
        } => {
            let config = BatchConfig {
                directory,
                output_dir,
                recursive,
                exif_match,
                incremental,
                copy_unmuxed,
                delete_video: cli.delete_video,
                delete_temp: !cli.keep_temp,
                overwrite: cli.overwrite,
            };
            batch::mux_directory(
                &config,
                &mut et,
                |total| {
                    info!("Found {} image/video pairs", total);
                },
                |progress| {
                    if progress.status == "processing" {
                        info!("[{}/{}] Processing: {}", progress.current, progress.total, progress.file);
                    } else if progress.success {
                        info!("[{}/{}] Done: {}", progress.current, progress.total, progress.file);
                    }
                },
            )?;
        }
    }

    Ok(())
}
