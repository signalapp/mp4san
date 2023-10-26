use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser as _, ValueEnum};

#[derive(clap::Parser)]
struct Args {
    /// The format of the media file.
    ///
    /// If not specified, a guess will be made based on the file extension.
    #[clap(long, short = 't')]
    format: Option<Format>,

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

    let file = File::open(args.file).context("Error opening file")?;

    match format {
        Format::Mp4 => mp4san::sanitize(file).map(drop).context("Error parsing mp4 file")?,
        Format::Webp => webpsan::sanitize(file).context("Error parsing webp file")?,
    }

    Ok(())
}
