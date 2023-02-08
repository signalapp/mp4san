use std::fs::File;
use std::io::{self, BufRead, BufReader, BufWriter, Read};
use std::path::PathBuf;

use anyhow::Context;
use clap::Parser as _;
use libflate::gzip;
use mp4::{BoxHeader, BoxType, HEADER_SIZE};

/// A tool to minify MP4 files (by removing their video data) for use as test input to mp4san.
#[derive(clap::Parser)]
#[command(version, about)]
struct Args {
    /// Path to MP4 input file.
    input: PathBuf,
    /// Path to gzipped MP4 test output file.
    output: PathBuf,
}

fn main() -> Result<(), anyhow::Error> {
    env_logger::init();

    let args = Args::try_parse().context("Error parsing command line arguments")?;

    let input = File::open(&args.input).context("Error opening input file")?;

    let output = File::create(&args.output).context("Error opening output file")?;

    let mut reader = BufReader::new(input);
    let mut encoder = gzip::Encoder::new(BufWriter::new(output)).context("Error writing to output")?;

    while !reader.fill_buf()?.is_empty() {
        let header = BoxHeader::read(&mut reader)?;
        header.write(&mut encoder).context("Error writing to output")?;

        let mut data_reader: Box<dyn Read> = match header.size {
            0 => Box::new(&mut reader),
            _ => Box::new(reader.by_ref().take(header.size - HEADER_SIZE)),
        };
        match header.name {
            BoxType::MdatBox => {
                let data_len = io::copy(&mut data_reader, &mut io::sink()).context("Error reading input")?;
                io::copy(&mut io::repeat(0).take(data_len), &mut encoder).context("Error writing to output")?;
            }
            _ => {
                io::copy(&mut data_reader, &mut encoder).context("Error copying input to output")?;
            }
        }
    }

    encoder.finish().into_result().context("Error writing to output")?;

    Ok(())
}
