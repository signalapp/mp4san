#![allow(clippy::redundant_pattern_matching)]
mod buffer;
mod util;

use std::io::{self, BufRead, BufReader, Read, Seek};
use std::mem::replace;
use std::num::NonZeroU64;
use std::ops::ControlFlow;
use std::ops::ControlFlow::{Break, Continue};

use mp4::{BoxHeader, BoxType, FourCC, FtypBox, MoovBox, ReadBox, WriteBox, HEADER_SIZE};
use util::checked_add_signed;

use crate::buffer::Buffer;

#[derive(Clone, Debug)]
pub struct Sanitizer {
    buffer: Buffer,

    /// The [`ftyp`](FtypBox) box which has been read, if any.
    ftyp: Option<FtypBox>,

    /// The [`moov`](MoovBox) box which has been read, if any.
    moov: Option<MoovBox>,

    /// The [span](InputSpan) of the `mdat` box or contiguous `mdat` boxes (possibly also interspersed with `free`
    /// boxes) which have been found, if any.
    data: Option<InputSpan>,

    /// Whether the captured boxes have been sanitized.
    sanitized: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SanitizerState {
    NeedsData,
    NeedsSkip(NonZeroU64),
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid box layout: {0}")]
    InvalidBoxLayout(&'static str),
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Parse error: {0}")]
    Parse(#[from] mp4::Error),
    #[error("Unsupported box: {0}")]
    UnsupportedBox(BoxType),
    #[error("Unsupported box layout: {0}")]
    UnsupportedBoxLayout(&'static str),
    #[error("Unsupported format: {0}")]
    UnsupportedFormat(FourCC),
    #[error("Invalid input: {0}")]
    InvalidInput(&'static str),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SanitizedMetadata {
    pub metadata: Vec<u8>,
    pub data: InputSpan,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct InputSpan {
    pub offset: u64,
    pub len: u64,
}

pub const COMPATIBLE_BRAND: FourCC = FourCC { value: *b"isom" };

pub fn sanitize<R: Read + Seek>(mut input: R) -> Result<SanitizedMetadata, Error> {
    let start_pos = input.stream_position()?;
    let input_len = input.seek(io::SeekFrom::End(0))?;
    input.seek(io::SeekFrom::Start(start_pos))?;

    let mut reader = BufReader::new(input);
    let mut sanitizer = Sanitizer::new(input_len);
    let mut state = SanitizerState::default();
    loop {
        match state {
            SanitizerState::NeedsData => {
                if reader.fill_buf()?.is_empty() {
                    break;
                }
                state = sanitizer.process_data(reader.buffer())?;
                reader.consume(reader.buffer().len());
            }
            SanitizerState::NeedsSkip(skip_amount) => {
                let old_pos = reader.stream_position()?;
                reader.seek(io::SeekFrom::Start(old_pos + skip_amount.get()))?;
                state = sanitizer.skip_data(skip_amount.get())?;
            }
        }
    }

    sanitizer.finish()
}

impl Sanitizer {
    /// Construct a new `Sanitizer`.
    pub fn new(input_len: u64) -> Self {
        Self { buffer: Buffer::new(input_len), ftyp: None, moov: None, data: None, sanitized: false }
    }

    /// Process the next chunk of input data.
    ///
    /// Returns a [`SanitizerState`], which indicates whether the caller should skip a certain amount of input and call
    /// [`skip_data`](Self::skip_data) before feeding more in to [`process_data`](Self::process_data) or calling
    /// [`finish`](Self::finish).
    pub fn process_data(&mut self, new_input: &[u8]) -> Result<SanitizerState, Error> {
        self.buffer.append_input(new_input)?;
        self.process_buffer()
    }

    /// Signal that `amount` bytes of input were skipped as requested by [`SanitizerState`].
    ///
    /// Note that skipping an amount less than that requested by a previously returned [`SanitizerState`] is not
    /// guaranteed to function correctly, and may result in an error being returned.
    ///
    /// Returns a [`SanitizerState`], which indicates whether the caller should skip a certain amount of input and call
    /// [`skip_data`](Self::skip_data) before feeding more in to [`process_data`](Self::process_data) or calling
    /// [`finish`](Self::finish).
    pub fn skip_data(&mut self, amount: u64) -> Result<SanitizerState, Error> {
        log::debug!(
            "skipping {amount} bytes of input starting at 0x{end_input_pos:08x}",
            end_input_pos = self.buffer.end_input_pos()
        );
        self.buffer.skip_input(amount)?;
        self.process_buffer()
    }

    /// Signal that the end of input has been reached and return sanitized metadata.
    pub fn finish(&mut self) -> Result<SanitizedMetadata, Error> {
        log::debug!(
            "finishing input at 0x{end_input_pos:08x}",
            end_input_pos = self.buffer.end_input_pos()
        );
        match self.process_buffer()? {
            SanitizerState::NeedsData => self.sanitize(),
            SanitizerState::NeedsSkip { .. } => Err(Error::from(io::Error::from(io::ErrorKind::UnexpectedEof))),
        }
    }

    fn process_buffer(&mut self) -> Result<SanitizerState, Error> {
        while !self.buffer.is_empty() {
            match self.read_box() {
                Ok(Continue(())) => (),

                Ok(Break(SanitizerState::NeedsSkip(need_skip_amount))) => {
                    log::debug!("requesting skip of {need_skip_amount} input bytes");
                    return Ok(SanitizerState::NeedsSkip(need_skip_amount));
                }
                Ok(Break(SanitizerState::NeedsData)) => return Ok(SanitizerState::NeedsData),

                Err(Error::Parse(mp4::Error::IoError(err))) | Err(Error::Io(err))
                    if matches!(err.kind(), io::ErrorKind::UnexpectedEof) =>
                {
                    return Ok(SanitizerState::NeedsData);
                }
                Err(err) => return Err(err),
            }
        }
        Ok(SanitizerState::NeedsData)
    }

    fn read_box(&mut self) -> Result<ControlFlow<SanitizerState>, Error> {
        let mut reader = self.buffer.reader();

        let start_pos = reader.stream_position()?;

        // NB: Only pass `size` to other `mp4` functions and don't rely on it to be meaningful; BoxHeader actually
        // subtracts HEADER_SIZE from size in the 64-bit box size case as a hack.
        let BoxHeader { name, size: header_box_size } = BoxHeader::read(&mut reader)?;
        let box_size = match header_box_size {
            0 => {
                let measured_box_size = reader.get_ref().input_len() - start_pos;
                log::info!("last box size: {measured_box_size}");
                measured_box_size
            }
            box_size => box_size,
        };

        match name {
            BoxType::FreeBox => {
                if let Break(state) = skip_box(&mut reader, header_box_size)? {
                    return Ok(Break(state));
                }

                log::info!("free @ 0x{start_pos:08x}: {box_size} bytes");

                // Try to extend any already accumulated data in case there's more mdat boxes to come.
                if let Some(data) = &mut self.data {
                    if data.offset + data.len == start_pos {
                        data.len += reader.stream_position()? - start_pos;
                    }
                }
            }

            BoxType::FtypBox if self.ftyp.is_some() => return Err(Error::InvalidBoxLayout("multiple ftyp boxes")),
            BoxType::FtypBox => {
                let read_ftyp = FtypBox::read_box(&mut reader, box_size)?;
                log::info!("ftyp @ 0x{start_pos:08x}: {read_ftyp:#?}");
                self.ftyp = Some(read_ftyp);
            }

            // NB: ISO 14496-12-2012 specifies a default ftyp, but we don't currently use it. The spec says that it
            // contains a single compatible brand, "mp41", and notably not "isom" which is the ISO spec we follow for
            // parsing now. This implies that there's additional stuff in "mp41" which is not in "isom". "mp41" is also
            // very old at this point, so it'll require additional research/work to be able to parse/remux it.
            _ if self.ftyp.is_none() => return Err(Error::InvalidBoxLayout("ftyp is not the first significant box")),

            BoxType::MdatBox => {
                if let Break(state) = skip_box(&mut reader, header_box_size)? {
                    return Ok(Break(state));
                }

                log::info!("mdat @ 0x{start_pos:08x}: {box_size} bytes");

                if let Some(data) = &mut self.data {
                    // Try to extend already accumulated data.
                    if data.offset + data.len != start_pos {
                        return Err(Error::UnsupportedBoxLayout("discontiguous mdat boxes"));
                    }
                    data.len += reader.stream_position()? - start_pos;
                } else {
                    self.data = Some(InputSpan { offset: start_pos, len: reader.stream_position()? - start_pos });
                }
            }
            BoxType::MoovBox => {
                let read_moov = MoovBox::read_box(&mut reader, box_size)?;
                log::info!("moov @ 0x{start_pos:08x}: {read_moov:#?}");
                self.moov = Some(read_moov);
            }
            _ => {
                log::info!("{name} @ 0x{start_pos:08x}: {box_size} bytes");
                return Err(Error::UnsupportedBox(name));
            }
        }

        reader.commit();
        Ok(Continue(()))
    }

    fn sanitize(&mut self) -> Result<SanitizedMetadata, Error> {
        let Some(ftyp) = &self.ftyp else {
            return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::FtypBox)));
        };
        if !ftyp.compatible_brands.contains(&COMPATIBLE_BRAND) {
            return Err(Error::UnsupportedFormat(ftyp.major_brand));
        };
        let Some(moov) = &mut self.moov else {
            return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MoovBox)));
        };
        let Some(data) = self.data else {
            return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MdatBox)));
        };

        let was_sanitized = replace(&mut self.sanitized, true);

        // Add a free box to pad, if one will fit, if the mdat box would move backward. If one won't fit, or if the mdat box
        // would move forward, adjust mdat offsets in stco/co64 the amount it was displaced.
        let metadata_len = ftyp.get_size() + moov.get_size();
        let mut pad_size = 0;
        match data.offset.checked_sub(metadata_len) {
            Some(0) => (),
            Some(size @ HEADER_SIZE..=u64::MAX) => pad_size = size,
            mdat_backward_displacement => {
                if !was_sanitized {
                    let mdat_displacement = match mdat_backward_displacement {
                        Some(mdat_backward_displacement) => {
                            mdat_backward_displacement.try_into().ok().and_then(i32::checked_neg)
                        }
                        None => metadata_len.checked_sub(data.offset).unwrap().try_into().ok(),
                    };
                    let mdat_displacement: i32 =
                        mdat_displacement.ok_or(Error::UnsupportedBoxLayout("mdat displaced too far"))?;

                    for trak in &mut moov.traks {
                        if let Some(stco) = &mut trak.mdia.minf.stbl.stco {
                            for entry in &mut stco.entries {
                                *entry = checked_add_signed(*entry, mdat_displacement)
                                    .ok_or(Error::InvalidInput("chunk offset not within mdat"))?;
                            }
                        } else if let Some(co64) = &mut trak.mdia.minf.stbl.co64 {
                            for entry in &mut co64.entries {
                                *entry = checked_add_signed(*entry, mdat_displacement.into())
                                    .ok_or(Error::InvalidInput("chunk offset not within mdat"))?;
                            }
                        }
                    }
                }
            }
        }

        let mut metadata = Vec::with_capacity((metadata_len + pad_size) as usize);
        ftyp.write_box(&mut metadata)?;
        moov.write_box(&mut metadata)?;
        if pad_size != 0 {
            BoxHeader { name: BoxType::FreeBox, size: pad_size }.write(&mut metadata)?;
            metadata.resize((metadata_len + pad_size) as usize, 0);
        }

        Ok(SanitizedMetadata { metadata, data })
    }
}

impl Default for SanitizerState {
    fn default() -> Self {
        SanitizerState::NeedsData
    }
}

impl From<Error> for io::Error {
    fn from(from: Error) -> Self {
        use Error::*;
        match from {
            err @ (InvalidBoxLayout { .. }
            | UnsupportedBox { .. }
            | UnsupportedBoxLayout { .. }
            | UnsupportedFormat { .. }) => io::Error::new(io::ErrorKind::InvalidData, err),
            err @ InvalidInput { .. } => io::Error::new(io::ErrorKind::InvalidInput, err),
            Io(err) => err,
            Parse(mp4::Error::IoError(err)) => err,
            Parse(err) => io::Error::new(io::ErrorKind::InvalidData, err),
        }
    }
}

fn skip_box(reader: &mut buffer::Reader<'_>, header_box_size: u64) -> Result<ControlFlow<SanitizerState>, Error> {
    let box_end_pos = match header_box_size {
        0 => reader.get_ref().input_len(),
        box_size => {
            let input_pos = reader.stream_position()?;
            let box_data_size = box_size
                .checked_sub(HEADER_SIZE)
                .ok_or(Error::InvalidInput("box size too small"))?;
            input_pos
                .checked_add(box_data_size)
                .ok_or(Error::InvalidInput("input length overflow"))?
        }
    };
    let need_skip_amount = box_end_pos.checked_sub(reader.get_ref().end_input_pos());
    match need_skip_amount.and_then(NonZeroU64::new) {
        Some(need_skip_amount) => Ok(Break(SanitizerState::NeedsSkip(need_skip_amount))),
        None => {
            reader.seek(io::SeekFrom::Start(box_end_pos))?;
            Ok(Continue(()))
        }
    }
}

#[cfg(test)]
mod test {
    use mp4::WriteBox;

    use crate::util::test::init_logger;

    use super::*;

    fn test_ftyp() -> FtypBox {
        FtypBox { major_brand: COMPATIBLE_BRAND, minor_version: 0, compatible_brands: vec![COMPATIBLE_BRAND] }
    }

    fn write_test_mdat(out: &mut Vec<u8>, data: &[u8]) -> InputSpan {
        let mut span = write_mdat_header(out, Some(data.len() as u64));
        out.extend_from_slice(data);
        span.len += data.len() as u64;
        span
    }

    fn write_mdat_header(out: &mut Vec<u8>, data_len: Option<u64>) -> InputSpan {
        let offset = out.len() as u64;
        let size = match data_len {
            Some(data_len) if data_len <= u32::MAX as u64 - HEADER_SIZE => data_len + HEADER_SIZE,
            Some(data_len) => data_len + HEADER_SIZE + 8,
            None => 0,
        };
        BoxHeader { name: BoxType::MdatBox, size }.write(out).unwrap();
        InputSpan { offset, len: out.len() as u64 - offset }
    }

    fn header_size(box_size: u64) -> u64 {
        if box_size <= u32::MAX as u64 {
            HEADER_SIZE
        } else {
            HEADER_SIZE + 8
        }
    }

    fn needs_skip(amount: u64) -> SanitizerState {
        SanitizerState::NeedsSkip(amount.try_into().unwrap())
    }

    struct TestMp4 {
        data: Vec<u8>,
        mdat_data: Vec<u8>,
        mdat: InputSpan,
        mdat_data_pos: usize,
    }

    impl TestMp4 {
        fn new(mdat_data: &[u8]) -> Self {
            let mut data = vec![];
            test_ftyp().write_box(&mut data).unwrap();
            MoovBox::default().write_box(&mut data).unwrap();
            let mdat = write_test_mdat(&mut data, mdat_data);
            let mdat_data_pos = (mdat.offset + header_size(mdat.len)) as usize;
            Self { data, mdat_data: mdat_data.to_vec(), mdat, mdat_data_pos }
        }

        fn with_mdat_data_len(mdat_data: &[u8], mdat_data_len: Option<u64>) -> Self {
            let mut data = vec![];
            test_ftyp().write_box(&mut data).unwrap();
            MoovBox::default().write_box(&mut data).unwrap();
            let mut mdat = write_mdat_header(&mut data, mdat_data_len);
            data.extend_from_slice(&mdat_data);
            if let Some(mdat_data_len) = mdat_data_len {
                mdat.len = mdat.len.saturating_add(mdat_data_len);
            } else {
                mdat.len += mdat_data.len() as u64;
            }
            let mdat_data_pos = (mdat.offset + header_size(mdat.len)) as usize;
            Self { data, mdat_data: mdat_data.to_vec(), mdat, mdat_data_pos }
        }
    }

    #[test]
    fn zero_size_moov() {
        init_logger();

        let mut data = vec![];
        test_ftyp().write_box(&mut data).unwrap();
        let mdat = write_test_mdat(&mut data, b"abcdefg");

        let moov_pos = data.len();
        MoovBox::default().write_box(&mut data).unwrap();
        let mut header = BoxHeader::read(&mut &data[moov_pos..]).unwrap();
        header.size = 0;
        header.write(&mut &mut data[moov_pos..]).unwrap();

        let mut sanitizer = Sanitizer::new(data.len() as u64);
        assert_eq!(
            sanitizer.process_data(&data[..data.len() - 1]).unwrap(),
            SanitizerState::NeedsData
        );
        assert_eq!(
            sanitizer.process_data(&data[data.len() - 1..]).unwrap(),
            SanitizerState::NeedsData
        );

        let sanitized = sanitizer.finish().unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitize(io::Cursor::new(&data)).unwrap(), sanitized);
    }

    #[test]
    fn zero_size_mdat() {
        init_logger();

        let TestMp4 { data, mdat, mdat_data_pos, .. } = TestMp4::with_mdat_data_len(b"abcdefg", None);
        let mut sanitizer = Sanitizer::new(data.len() as u64);
        assert_eq!(
            sanitizer.process_data(&data[..mdat.offset as usize]).unwrap(),
            SanitizerState::NeedsData
        );
        assert_eq!(
            sanitizer
                .process_data(&data[mdat.offset as usize..mdat_data_pos])
                .unwrap(),
            needs_skip(mdat.len - HEADER_SIZE)
        );
        assert_eq!(sanitizer.skip_data(mdat.len - HEADER_SIZE - 1).unwrap(), needs_skip(1));
        assert_eq!(sanitizer.skip_data(1).unwrap(), SanitizerState::NeedsData);

        let sanitized = sanitizer.finish().unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitize(io::Cursor::new(&data)).unwrap(), sanitized);
    }

    #[test]
    fn skip() {
        init_logger();

        let TestMp4 { data, mdat, mdat_data_pos, .. } = TestMp4::new(b"abcdefg");
        let mut sanitizer = Sanitizer::new(data.len() as u64);
        assert_eq!(
            sanitizer.process_data(&data[..mdat_data_pos]).unwrap(),
            needs_skip(mdat.len - HEADER_SIZE)
        );
        assert_eq!(
            sanitizer.process_data(&data[mdat_data_pos..mdat_data_pos + 1]).unwrap(),
            needs_skip(mdat.len - HEADER_SIZE - 1)
        );
        assert_eq!(
            sanitizer.skip_data(mdat.len - HEADER_SIZE - 1 - 3).unwrap(),
            needs_skip(3)
        );
        assert_eq!(sanitizer.skip_data(3).unwrap(), SanitizerState::NeedsData);

        let sanitized = sanitizer.finish().unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitize(io::Cursor::new(&data)).unwrap(), sanitized);
    }

    #[test]
    fn skip_too_little() {
        init_logger();

        let mut data = vec![];
        test_ftyp().write_box(&mut data).unwrap();
        let mdat = write_test_mdat(&mut data, b"abcdefg");
        let mdat_data_pos = (mdat.offset + HEADER_SIZE) as usize;
        MoovBox::default().write_box(&mut data).unwrap();

        let mut sanitizer = Sanitizer::new(data.len() as u64);
        assert_eq!(
            sanitizer.process_data(&data[..mdat_data_pos]).unwrap(),
            needs_skip(mdat.len - HEADER_SIZE)
        );
        assert_eq!(sanitizer.skip_data(mdat.len - HEADER_SIZE - 1).unwrap(), needs_skip(1));
        sanitizer
            .process_data(&data[(mdat.offset + mdat.len - 1) as usize..])
            .unwrap_err();
    }

    #[test]
    fn skip_too_much() {
        init_logger();

        let TestMp4 { data, mdat_data, mdat_data_pos, .. } = TestMp4::new(b"abcdefg");
        let mut sanitizer = Sanitizer::new(data.len() as u64);
        assert_eq!(
            sanitizer.process_data(&data[..mdat_data_pos]).unwrap(),
            needs_skip(mdat_data.len() as u64)
        );
        sanitizer.skip_data(mdat_data.len() as u64 + 1).unwrap_err();
    }

    #[test]
    fn erroneous_skip() {
        init_logger();

        let mut sanitizer = Sanitizer::new(100);
        sanitizer.skip_data(1).unwrap_err();
    }

    #[test]
    fn max_input_length() {
        init_logger();

        let mut data = vec![];
        test_ftyp().write_box(&mut data).unwrap();
        MoovBox::default().write_box(&mut data).unwrap();
        let mdat_data_len = u64::MAX - data.len() as u64 - (HEADER_SIZE + 8);
        let mut mdat = write_mdat_header(&mut data, Some(mdat_data_len));
        mdat.len += mdat_data_len;

        let mut sanitizer = Sanitizer::new(data.len() as u64 + mdat_data_len);
        assert_eq!(sanitizer.process_data(&data).unwrap(), needs_skip(mdat_data_len));
        assert_eq!(sanitizer.skip_data(mdat_data_len).unwrap(), SanitizerState::NeedsData);
        let sanitized = sanitizer.finish().unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitized.data.offset + sanitized.data.len, u64::MAX);
    }

    #[test]
    fn zero_size_mdat_input_length_overflow() {
        init_logger();

        let TestMp4 { data, .. } = TestMp4::with_mdat_data_len(b"abcdefg", None);
        let mut sanitizer = Sanitizer::new(u64::MAX);
        assert_eq!(
            sanitizer.process_data(&data).unwrap(),
            needs_skip(u64::MAX - data.len() as u64)
        );
        assert_eq!(
            sanitizer.skip_data(u64::MAX - data.len() as u64).unwrap(),
            SanitizerState::NeedsData
        );
        sanitizer.skip_data(1).unwrap_err();
    }

    #[test]
    fn input_length_overflow() {
        init_logger();

        let mut data = vec![];
        test_ftyp().write_box(&mut data).unwrap();
        MoovBox::default().write_box(&mut data).unwrap();
        let mdat = BoxHeader { name: BoxType::MdatBox, size: u64::MAX - data.len() as u64 + 1 };
        mdat.write(&mut data).unwrap();

        let mut sanitizer = Sanitizer::new(data.len() as u64);
        sanitizer.process_data(&data).unwrap_err();
    }

    #[test]
    fn box_size_overflow() {
        init_logger();

        let mut data = vec![];
        test_ftyp().write_box(&mut data).unwrap();
        MoovBox::default().write_box(&mut data).unwrap();
        BoxHeader { name: BoxType::MdatBox, size: u64::MAX }
            .write(&mut data)
            .unwrap();

        let mut sanitizer = Sanitizer::new(data.len() as u64);
        sanitizer.process_data(&data).unwrap_err();
    }

    #[test]
    fn skip_overflow() {
        init_logger();

        let TestMp4 { data, .. } = TestMp4::with_mdat_data_len(b"abcdefg", None);
        let mut sanitizer = Sanitizer::new(u64::MAX);
        assert_eq!(
            sanitizer.process_data(&data).unwrap(),
            needs_skip(u64::MAX - data.len() as u64)
        );
        assert_eq!(
            sanitizer.skip_data(u64::MAX - data.len() as u64).unwrap(),
            SanitizerState::NeedsData
        );
        sanitizer.skip_data(1).unwrap_err();
    }
}
