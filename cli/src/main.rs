use std::fs;
use std::fs::File;
use std::io;
use std::io::{Read, Seek, Write};
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser as _, ValueEnum};
use mp4san::{Config, SanitizedMetadata};

#[derive(clap::Parser)]
struct Args {
    /// The format of the media file.
    ///
    /// If not specified, a guess will be made based on the file extension.
    #[clap(long, short = 't')]
    format: Option<Format>,

    /// Path to the file to write sanitized output.
    #[clap(long, short = 'o')]
    output: Option<PathBuf>,

    #[clap(long, short = 'c')]
    cumulative_mdat_box_size: Option<u64>,

    /// Path to the file to test sanitization on.
    file: PathBuf,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum Format {
    Mp4,
    Webp,
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .try_init()
        .context("Error initializing logging")?;

    let args = Args::try_parse().context("Error parsing command line arguments")?;

    let format = match args.format {
        Some(t) => t,
        None => {
            let extension = args.file.extension().unwrap_or_default();
            ValueEnum::from_str(&extension.to_string_lossy(), true)
                .map_err(|_| anyhow::anyhow!("can't guess media format (unrecognized extension {extension:?})"))?
        }
    };

    let mut infile = File::open(&args.file).context("Error opening file")?;

    let _cumulative_mdat_box_size = match args.cumulative_mdat_box_size {
        Some(t) => t,
        None => 0,
    };

    match format {
        Format::Mp4 => {
            let analysis_result = match args.cumulative_mdat_box_size {
                Some(t) => {
                    let mut config = Config::default();
                    config.cumulative_mdat_box_size = t;
                    mp4san::sanitize_with_config(&mut infile, config).context("Error parsing mp4 file")?
                },
                None => mp4san::sanitize(&mut infile).context("Error parsing mp4 file")?,
            };
            match analysis_result {
                SanitizedMetadata { metadata: Some(metadata), data } => {
                    if let Some(output_path) = args.output {
                        let mut outfile = File::create(output_path).context("Error opening output file")?;
                        outfile.write(&metadata).context("Error writing output")?;
                        infile
                            .seek(io::SeekFrom::Start(data.offset))
                            .context("Error seeking input")?;
                        io::copy(&mut infile.take(data.len), &mut outfile).context("Error copying input to output")?;
                    }
                }
                SanitizedMetadata { metadata: None, .. } => {
                    if let Some(output_path) = args.output {
                        fs::copy(&args.file, output_path).context("Error writing output")?;
                    }
                }
            }
        },
        Format::Webp => {
            webpsan::sanitize(infile).context("Error parsing webp file")?;
            if let Some(output_path) = args.output {
                fs::copy(args.file, output_path).context("Error writing output")?;
            }
        }
    };

    Ok(())
}
