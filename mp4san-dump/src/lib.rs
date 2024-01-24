//! WARNING: This is not a 100% correct implementation of a frame dumper for H.264 in MPEG-4.
//!
//! Many many things were skipped and/or hardcoded. Do not use this as a reference, only a starting
//! point.

use std::io;
use std::ops::Range;

use bytes::Buf as _;
use futures_util::{pin_mut, FutureExt as _};
use mediasan_common::{bail_attach, ensure_attach, report_attach, SeekSkipAdapter};
use mp4san::parse::{
    ArrayEntry, BoxHeader, BoxType, FtypBox, MoovBox, Mp4Box, ParseBox, ParseError, ParsedBox, StscEntry, TrakBox,
};
use mp4san::{Error, COMPATIBLE_BRAND};

// Note: modified version of mp4san::sanitize_async_with_config; should eventually be folded back
// together with that.
pub fn parse(input: &[u8]) -> Result<Mp4Box<MoovBox>, Error> {
    let mut reader = io::Cursor::new(input);

    let mut mdat_seen = false;
    let mut ftyp_seen = false;
    let mut moov: Option<Mp4Box<MoovBox>> = None;

    while reader.position() != reader.get_ref().len() as u64 {
        let start_pos = reader.position();

        let header = BoxHeader::parse(&mut reader).expect("valid header");

        match header.box_type() {
            name @ (BoxType::FREE | BoxType::SKIP) => {
                skip_box(&mut reader, &header).expect("can skip in Cursor");
                log::info!("{name} @ 0x{start_pos:08x}");
            }

            BoxType::FTYP => {
                assert!(!ftyp_seen, "multiple ftyp boxes");
                let mut read_ftyp = read_data_sync(&mut reader, header, 1024).expect("valid box");
                let ftyp_data: &mut FtypBox = read_ftyp.data.parse().expect("valid ftyp");
                let compatible_brand_count = ftyp_data.compatible_brands().len();
                let FtypBox { major_brand, minor_version, .. } = ftyp_data;
                log::info!("ftyp @ 0x{start_pos:08x}: {major_brand} version {minor_version}, {compatible_brand_count} compatible brands");

                ensure_attach!(
                    ftyp_data.compatible_brands().any(|b| b == COMPATIBLE_BRAND),
                    ParseError::UnsupportedFormat(ftyp_data.major_brand)
                );

                ftyp_seen = true;
            }

            // NB: ISO 14496-12-2012 specifies a default ftyp, but we don't currently use it. The spec says that it
            // contains a single compatible brand, "mp41", and notably not "isom" which is the ISO spec we follow for
            // parsing now. This implies that there's additional stuff in "mp41" which is not in "isom". "mp41" is also
            // very old at this point, so it'll require additional research/work to be able to parse/remux it.
            _ if !ftyp_seen => {
                bail_attach!(ParseError::InvalidBoxLayout, "ftyp is not the first significant box");
            }

            BoxType::MDAT => {
                mdat_seen = true;
                skip_box(&mut reader, &header).expect("can skip in Cursor");
                log::info!("mdat @ 0x{start_pos:08x}");
            }

            BoxType::MOOV => {
                let mut read_moov: Mp4Box<MoovBox> =
                    read_data_sync(&mut reader, header, 1024 * 1024).expect("can read box");
                let children = read_moov.data.parse().expect("valid moov").parsed_mut();

                let chunk_count = children.tracks.iter().map(|trak| trak.co().entry_count()).sum::<u32>();
                let trak_count = children.tracks.len();

                log::info!("moov @ 0x{start_pos:08x}: {trak_count} traks {chunk_count} chunks");
                moov = Some(read_moov);
            }

            name => {
                skip_box(&mut reader, &header).expect("can skip in Cursor");
                log::info!("{name} @ 0x{start_pos:08x}");
            }
        }
    }

    if !ftyp_seen {
        bail_attach!(ParseError::MissingRequiredBox(BoxType::FTYP));
    }
    if !mdat_seen {
        bail_attach!(ParseError::MissingRequiredBox(BoxType::MDAT));
    }
    let Some(moov) = moov else {
        bail_attach!(ParseError::MissingRequiredBox(BoxType::MOOV));
    };

    Ok(moov)
}

/// Skip a box's data assuming its header has already been read.
fn skip_box(reader: &mut impl io::Seek, header: &BoxHeader) -> Result<(), io::Error> {
    match header.box_data_size().expect("valid header") {
        Some(box_size) => reader.seek(io::SeekFrom::Current(box_size as i64))?,
        None => reader.seek(io::SeekFrom::End(0))?,
    };
    Ok(())
}

/// Read a box's data.
fn read_data_sync<T, R>(reader: &mut R, header: BoxHeader, max_size: u64) -> Result<Mp4Box<T>, Error>
where
    R: std::io::Read + std::io::Seek,
    T: ParseBox + ParsedBox,
{
    let async_reader = SeekSkipAdapter(futures_util::io::AllowStdIo::new(reader));
    pin_mut!(async_reader);
    Mp4Box::read_data(async_reader, header, max_size)
        .now_or_never()
        .expect("only awaits reader")
}

/// Iterates through every sample in the track in order, ignoring timecodes and edit lists.
pub fn for_each_sample(
    track: &TrakBox,
    full_data: &[u8],
    mut process: impl FnMut(&[u8]) -> Result<(), Error>,
) -> Result<(), Error> {
    let samples = track.parsed().media.parsed().info.parsed().samples;
    let mut sample_to_chunk_walker = SampleToChunkWalker::new(samples.parsed().sample_to_chunk.entries());

    let offset_for_chunk = |i| {
        Ok::<usize, Error>(match samples.parsed().chunk_offsets {
            mp4san::parse::StblCoRef::Stco(stco) => stco
                .entries()
                .nth(i as usize - 1)
                .ok_or_else(|| report_attach!(ParseError::InvalidInput))?
                .get()? as usize,
            mp4san::parse::StblCoRef::Co64(co64) => co64
                .entries()
                .nth(i as usize - 1)
                .ok_or_else(|| report_attach!(ParseError::InvalidInput))?
                .get()? as usize,
        })
    };

    let mut prev_chunk_index = 0;
    let mut current_chunk_data: &[u8] = &[];
    for (sample_size, sample_index) in samples.parsed().sample_sizes.sample_sizes().zip(0..) {
        let sample_size = sample_size?;
        let ChunkInfo { chunk_index, num_samples_in_chunk: _ } = sample_to_chunk_walker.chunk_info_for(sample_index);
        if chunk_index != prev_chunk_index {
            current_chunk_data = &full_data[offset_for_chunk(chunk_index)?..];
            prev_chunk_index = chunk_index;
        }
        let current_sample = &current_chunk_data[..sample_size as usize];
        current_chunk_data.advance(sample_size as usize);

        process(current_sample)?;
    }

    Ok(())
}

#[derive(PartialEq, Eq)]
struct ChunkInfo {
    pub chunk_index: u32,
    pub num_samples_in_chunk: u32,
}

/// Wraps an iterator over stsc ("sample-to-chunk") entries to allow looking up chunks for given
/// sample indexes.
///
/// Samples must be accessed in increasing order, but skipping is allowed. This type expects sample
/// indexes to be zero-indexed.
struct SampleToChunkWalker<T: Iterator> {
    entry_iter: std::iter::Peekable<T>,
    chunks_in_entry: Range<u32>,
    num_samples_in_chunk: u32,
    samples_seen_so_far: usize,
}

impl<'a, T> SampleToChunkWalker<T>
where
    T: Iterator<Item = ArrayEntry<'a, StscEntry>>,
{
    fn new(iter: T) -> Self {
        Self { entry_iter: iter.peekable(), chunks_in_entry: 0..0, num_samples_in_chunk: 0, samples_seen_so_far: 0 }
    }

    fn advance_one_chunk(&mut self) {
        self.samples_seen_so_far += self.num_samples_in_chunk as usize;
        _ = self.chunks_in_entry.next();
        if self.chunks_in_entry.is_empty() {
            let next_entry = self
                .entry_iter
                .next()
                .expect("has more entries")
                .get()
                .expect("valid sample-to-chunk entry");
            let upper_bound = self.entry_iter.peek().map_or(u32::MAX, |following_entry| {
                following_entry.get().expect("valid sample-to-chunk entry").first_chunk
            });
            self.chunks_in_entry = next_entry.first_chunk..upper_bound;
            self.num_samples_in_chunk = next_entry.samples_per_chunk;
        }
    }

    fn chunk_info_for(&mut self, sample_index: usize) -> ChunkInfo {
        assert!(
            sample_index >= self.samples_seen_so_far,
            "searching backwards is not implemented"
        );
        while sample_index - self.samples_seen_so_far >= self.num_samples_in_chunk as usize {
            self.advance_one_chunk();
        }

        ChunkInfo { chunk_index: self.chunks_in_entry.start, num_samples_in_chunk: self.num_samples_in_chunk }
    }
}
