use std::fs::File;
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser as _;

#[derive(clap::Parser)]
struct Args {
    file: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .try_init()
        .context("Error initializing logging")?;

    let args = Args::try_parse().context("Error parsing command line arguments")?;

    let file = File::open(args.file).context("Error opening mp4 file")?;

    mp4san::sanitize(file).context("Error parsing mp4 file")?;

    Ok(())
}
