//! WARNING: This is not a 100% correct implementation of a frame dumper for H.264 in MPEG-4.
//!
//! Many many things were skipped and/or hardcoded. Do not use this as a reference, only a starting
//! point.

use std::io::Read as _;
use std::io::{self, Write};

use bytes::Buf;
use mp4san::parse::MoovBox;

const NAL_HEADER: &[u8] = &[0, 0, 0, 1];

pub fn main() {
    env_logger::init();

    let mut input = Vec::with_capacity(100 * 1024);
    io::stdin().read_to_end(&mut input).expect("can read stdin");

    let moov = mp4san_dump::parse(&input).expect("valid input");
    let moov_children = moov.data.parsed::<MoovBox>().expect("parsed moov box already").parsed();

    // FIXME: The first track isn't always the video track.
    if let Some(track) = moov_children.tracks.get(0) {
        mp4san_dump::for_each_sample(track, &input, |mut sample| {
            // FIXME: not all NAL lengths use four bytes
            let nal_length = sample.get_u32() as usize;
            assert_eq!(
                nal_length,
                sample.len(),
                "NAL split across samples, or multiple NALs in a sample"
            );

            std::io::stdout().write_all(NAL_HEADER).expect("can write to stdout");
            std::io::stdout().write_all(sample).expect("can write to stdout");

            Ok(())
        })
        .expect("valid parsed input");
    }
}
