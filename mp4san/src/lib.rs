#[macro_use]
extern crate error_stack;

#[macro_use]
mod macros;

pub mod parse;
mod util;

use std::io::{self, BufRead, BufReader, Read, Seek};

use bytes::BytesMut;
use derive_more::Display;
use error_stack::Report;

use crate::parse::error::{MultipleBoxes, WhileParsingBox};
use crate::parse::{BoxData, BoxHeader, BoxType, FourCC, FtypBox, MoovBox, Mp4Box, ParseBox, ParseError, StblCoMut};
use crate::util::{checked_add_signed, IoResultExt};

//
// public types
//

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    #[error("Parse error: {0}")]
    Parse(Report<ParseError>),
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

/// A subset of the [`Seek`] trait, providing a cursor which can skip forward within a stream of bytes.
///
/// [`Skip`] is automatically implemented for all types implementing [`Seek`].
pub trait Skip {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn skip(&mut self, amount: u64) -> io::Result<()>;

    /// Returns the current position of the cursor from the start of the stream.
    fn stream_position(&mut self) -> io::Result<u64>;

    /// Returns the length of this stream, in bytes.
    fn stream_len(&mut self) -> io::Result<u64>;
}

pub const COMPATIBLE_BRAND: FourCC = FourCC { value: *b"isom" };

//
// private types
//

/// [`Skip`] extension trait for [`BufReader`].
///
/// The blanket implementation of [`Skip`] for types implementing [`Seek`] means that [`Skip`] can't be implemented for
/// [`BufReader<T>`] when `T` is only [`Skip`] and not [`Seek`]. This trait fixes that.
trait BufReaderSkipExt {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn skip(&mut self, amount: u64) -> io::Result<()>;

    /// Returns the current position of the cursor from the start of the stream.
    fn stream_position(&mut self) -> io::Result<u64>;

    /// Returns the length of this stream, in bytes.
    fn stream_len(&mut self) -> io::Result<u64>;
}

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "box data too large: {} > {}", _0, MAX_READ_BOX_SIZE)]
struct BoxDataTooLarge(u64);

const MAX_READ_BOX_SIZE: u64 = 200 * 1024 * 1024;

//
// public functions
//

pub fn sanitize<R: Read + Skip>(input: R) -> Result<SanitizedMetadata, Error> {
    let mut reader = BufReader::with_capacity(BoxHeader::MAX_SIZE as usize, input);

    let mut ftyp: Option<Mp4Box<FtypBox>> = None;
    let mut moov: Option<Mp4Box<MoovBox>> = None;
    let mut data: Option<InputSpan> = None;

    while !reader.fill_buf()?.is_empty() {
        let start_pos = reader.stream_position()?;

        let header = BoxHeader::read(&mut reader)
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedBox, "while parsing box header")))?;

        match header.box_type() {
            BoxType::FREE => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("free @ 0x{start_pos:08x}: {box_size} bytes");

                // Try to extend any already accumulated data in case there's more mdat boxes to come.
                if let Some(data) = &mut data {
                    if data.offset + data.len == start_pos {
                        data.len += box_size;
                    }
                }
            }

            BoxType::FTYP => {
                ensure_attach!(
                    ftyp.is_none(),
                    ParseError::InvalidBoxLayout,
                    MultipleBoxes(BoxType::FTYP)
                );
                let mut read_ftyp = read_box(&mut reader, header)?;
                let ftyp_data = read_ftyp.data.parse()?;
                log::info!("ftyp @ 0x{start_pos:08x}: {ftyp_data:#?}");
                ftyp = Some(read_ftyp);
            }

            // NB: ISO 14496-12-2012 specifies a default ftyp, but we don't currently use it. The spec says that it
            // contains a single compatible brand, "mp41", and notably not "isom" which is the ISO spec we follow for
            // parsing now. This implies that there's additional stuff in "mp41" which is not in "isom". "mp41" is also
            // very old at this point, so it'll require additional research/work to be able to parse/remux it.
            _ if ftyp.is_none() => {
                bail_attach!(ParseError::InvalidBoxLayout, "ftyp is not the first significant box");
            }

            BoxType::MDAT => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("mdat @ 0x{start_pos:08x}: {box_size} bytes");

                if let Some(data) = &mut data {
                    // Try to extend already accumulated data.
                    ensure_attach!(
                        data.offset + data.len == start_pos,
                        ParseError::UnsupportedBoxLayout,
                        "discontiguous mdat boxes",
                    );
                    data.len += box_size;
                } else {
                    data = Some(InputSpan { offset: start_pos, len: box_size });
                }
            }
            BoxType::MOOV => {
                let mut read_moov = read_box(&mut reader, header)?;
                let moov_data = read_moov.data.parse()?;
                log::info!("moov @ 0x{start_pos:08x}: {moov_data:#?}");
                moov = Some(read_moov);
            }
            name => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("{name} @ 0x{start_pos:08x}: {box_size} bytes");
                return Err(report!(ParseError::UnsupportedBox(name)).into());
            }
        }
    }

    let Some(mut ftyp) = ftyp else {
        return Err(report!(ParseError::MissingRequiredBox(BoxType::FTYP)).into());
    };
    let ftyp_data = ftyp.data.parse()?;
    if !ftyp_data.compatible_brands().any(|b| b == COMPATIBLE_BRAND) {
        return Err(report!(ParseError::UnsupportedFormat(ftyp_data.major_brand)).into());
    };
    let Some(mut moov) = moov else {
        return Err(report!(ParseError::MissingRequiredBox(BoxType::MOOV)).into());
    };
    let Some(data) = data else {
        return Err(report!(ParseError::MissingRequiredBox(BoxType::MDAT)).into());
    };

    // Add a free box to pad, if one will fit, if the mdat box would move backward. If one won't fit, or if the mdat box
    // would move forward, adjust mdat offsets in stco/co64 the amount it was displaced.
    let metadata_len = ftyp.encoded_len() + moov.encoded_len();
    let mut pad_size = 0;
    const PAD_HEADER_SIZE: u64 = BoxHeader::with_u32_data_size(BoxType::FREE, 0).encoded_len();
    const MAX_PAD_SIZE: u64 = u32::MAX as u64 - PAD_HEADER_SIZE;
    match data.offset.checked_sub(metadata_len) {
        Some(0) => (),
        Some(size @ PAD_HEADER_SIZE..=MAX_PAD_SIZE) => pad_size = size,
        mdat_backward_displacement => {
            let mdat_displacement = match mdat_backward_displacement {
                Some(mdat_backward_displacement) => {
                    mdat_backward_displacement.try_into().ok().and_then(i32::checked_neg)
                }
                None => metadata_len.checked_sub(data.offset).unwrap().try_into().ok(),
            };
            let mdat_displacement: i32 = mdat_displacement
                .ok_or_else(|| report_attach!(ParseError::UnsupportedBoxLayout, "mdat displaced too far"))?;

            for trak in &mut moov.data.parse()?.traks() {
                let co = trak?.mdia_mut()?.minf_mut()?.stbl_mut()?.co_mut()?;
                if let StblCoMut::Stco(stco) = co {
                    for mut entry in &mut stco.entries_mut() {
                        entry.set(
                            checked_add_signed(entry.get(), mdat_displacement).ok_or_else(|| {
                                report_attach!(ParseError::InvalidInput, "chunk offset not within mdat")
                            })?,
                        );
                    }
                } else if let StblCoMut::Co64(co64) = co {
                    for mut entry in &mut co64.entries_mut() {
                        entry.set(
                            checked_add_signed(entry.get(), mdat_displacement.into()).ok_or_else(|| {
                                report_attach!(ParseError::InvalidInput, "chunk offset not within mdat")
                            })?,
                        );
                    }
                }
            }
        }
    }

    let mut metadata = Vec::with_capacity((metadata_len + pad_size) as usize);
    ftyp.put_buf(&mut metadata);
    moov.put_buf(&mut metadata);
    if pad_size != 0 {
        let pad_header = BoxHeader::with_u32_data_size(BoxType::FREE, (pad_size - PAD_HEADER_SIZE) as u32);
        pad_header.put_buf(&mut metadata);
        metadata.resize((metadata_len + pad_size) as usize, 0);
    }

    Ok(SanitizedMetadata { metadata, data })
}

//
// Skip impls
//

impl<T: Seek> Skip for T {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                self.seek(io::SeekFrom::Current(amount))?;
            }
            Err(_) => {
                let stream_pos = self.stream_position()?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                self.seek(io::SeekFrom::Start(seek_pos))?;
            }
        }
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        io::Seek::stream_position(self)
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        // This is the unstable Seek::stream_len
        let stream_pos = self.stream_position()?;
        let len = self.seek(io::SeekFrom::End(0))?;

        if stream_pos != len {
            self.seek(io::SeekFrom::Start(stream_pos))?;
        }

        Ok(len)
    }
}

//
// private functions
//

/// Read and parse a box's data assuming its header has already been read.
fn read_box<R, T>(reader: &mut BufReader<R>, header: BoxHeader) -> Result<Mp4Box<T>, Error>
where
    R: Read + Skip,
    T: ParseBox,
{
    let box_data_size = match header.box_data_size()? {
        Some(box_data_size) => box_data_size,
        None => reader.stream_len()? - reader.stream_position()?,
    };

    ensure_attach!(
        box_data_size <= MAX_READ_BOX_SIZE,
        ParseError::InvalidInput,
        BoxDataTooLarge(box_data_size),
        WhileParsingBox(header.box_type()),
    );

    let mut buf = BytesMut::zeroed(box_data_size as usize);
    reader.read_exact(&mut buf).map_eof(|_| {
        Error::Parse(report_attach!(
            ParseError::TruncatedBox,
            WhileParsingBox(header.box_type())
        ))
    })?;
    Ok(Mp4Box { header, data: BoxData::Bytes(buf) })
}

/// Skip a box's data assuming its header has already been read.
///
/// Returns the amount of data that was skipped.
fn skip_box<R: Read + Skip>(reader: &mut BufReader<R>, header: &BoxHeader) -> Result<u64, Error> {
    let box_data_size = match header.box_data_size()? {
        Some(box_size) => box_size,
        None => reader.stream_len()? - reader.stream_position()?,
    };
    reader.skip(box_data_size).map_eof(|_| {
        Error::Parse(report_attach!(
            ParseError::TruncatedBox,
            WhileParsingBox(header.box_type())
        ))
    })?;
    Ok(box_data_size)
}

//
// BufReaderSkipExt impls
//

impl<T: Read + Skip> BufReaderSkipExt for BufReader<T> {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let buf_len = self.buffer().len();
        if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
            if skip_amount != 0 {
                self.get_mut().skip(skip_amount)?;
            }
        }
        self.consume(buf_len.min(amount as usize));
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        let stream_pos = self.get_mut().stream_position()?;
        Ok(stream_pos.saturating_sub(self.buffer().len() as u64))
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        self.get_mut().stream_len()
    }
}

//
// Error impls
//

impl From<Report<ParseError>> for Error {
    fn from(from: Report<ParseError>) -> Self {
        Self::Parse(from)
    }
}

#[cfg(test)]
mod test {
    use bytes::{Buf, Bytes};

    use crate::util::test::init_logger;

    use super::*;

    fn test_ftyp() -> Mp4Box<FtypBox> {
        Mp4Box::with_data(FtypBox::new(COMPATIBLE_BRAND, 0, [COMPATIBLE_BRAND])).unwrap()
    }

    fn test_moov() -> Mp4Box<MoovBox> {
        Mp4Box::with_data(MoovBox::default()).unwrap()
    }

    fn write_test_mdat(out: &mut Vec<u8>, data: &[u8]) -> InputSpan {
        let mut span = write_mdat_header(out, Some(data.len() as u64));
        out.extend_from_slice(data);
        span.len += data.len() as u64;
        span
    }

    fn write_mdat_header(out: &mut Vec<u8>, data_len: Option<u64>) -> InputSpan {
        let offset = out.len() as u64;
        let header = match data_len {
            Some(data_len) => BoxHeader::with_data_size(BoxType::MDAT, data_len).unwrap(),
            None => BoxHeader::until_eof(BoxType::MDAT),
        };
        header.put_buf(&mut *out);
        InputSpan { offset, len: out.len() as u64 - offset }
    }

    struct TestMp4 {
        data: Bytes,
        data_len: u64,
        mdat: InputSpan,
        mdat_skipped: u64,
    }

    impl TestMp4 {
        fn new(mdat_data: &[u8]) -> Self {
            let mut data = vec![];
            test_ftyp().put_buf(&mut data);
            test_moov().put_buf(&mut data);
            let mdat = write_test_mdat(&mut data, mdat_data);
            Self { data_len: data.len() as u64, data: data.into(), mdat, mdat_skipped: 0 }
        }

        fn with_mdat_data_len(mdat_data: &[u8], mdat_data_len: Option<u64>) -> Self {
            let mut data = vec![];
            test_ftyp().put_buf(&mut data);
            test_moov().put_buf(&mut data);
            let mut mdat = write_mdat_header(&mut data, mdat_data_len);
            data.extend_from_slice(&mdat_data);
            if let Some(mdat_data_len) = mdat_data_len {
                mdat.len = mdat.len.saturating_add(mdat_data_len);
            } else {
                mdat.len += mdat_data.len() as u64;
            }
            Self { data_len: data.len() as u64, data: data.into(), mdat, mdat_skipped: 0 }
        }
    }

    impl Read for TestMp4 {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            (&mut self.data).reader().read(buf)
        }
    }

    impl Skip for TestMp4 {
        fn skip(&mut self, amount: u64) -> io::Result<()> {
            let advance_amount = self.data.len().min(amount as usize);
            self.data.advance(advance_amount);

            let skip_amount = amount.saturating_sub(advance_amount as u64);
            let mdat_end = self.mdat.offset.saturating_add(self.mdat.len);
            let mdat_skip_max = mdat_end.saturating_sub(self.data_len);
            match self.mdat_skipped.checked_add(skip_amount) {
                Some(mdat_skipped) if mdat_skipped <= mdat_skip_max => {
                    self.mdat_skipped = mdat_skipped;
                    Ok(())
                }
                _ => Err(io::Error::new(
                    io::ErrorKind::UnexpectedEof,
                    "test skipped past u64 limit",
                )),
            }
        }

        fn stream_position(&mut self) -> io::Result<u64> {
            Ok(self.data_len - self.data.len() as u64 + self.mdat_skipped)
        }

        fn stream_len(&mut self) -> io::Result<u64> {
            Ok(self.data_len.max(self.mdat.offset + self.mdat.len))
        }
    }

    #[test]
    fn zero_size_moov() {
        init_logger();

        let mut data = vec![];
        test_ftyp().put_buf(&mut data);
        let mdat = write_test_mdat(&mut data, b"abcdefg");

        let moov_pos = data.len();
        test_moov().put_buf(&mut data);
        BoxHeader::until_eof(BoxType::MOOV).put_buf(&mut &mut data[moov_pos..]);

        let sanitized = sanitize(io::Cursor::new(&data)).unwrap();
        assert_eq!(sanitized.data, mdat);
    }

    #[test]
    fn zero_size_mdat() {
        init_logger();

        let test @ TestMp4 { mdat, .. } = TestMp4::with_mdat_data_len(b"abcdefg", None);
        let sanitized = sanitize(test).unwrap();
        assert_eq!(sanitized.data, mdat);
    }

    #[test]
    fn skip() {
        init_logger();

        let test @ TestMp4 { mdat, .. } = TestMp4::new(b"abcdefg");
        let sanitized = sanitize(test).unwrap();
        assert_eq!(sanitized.data, mdat);
    }

    #[test]
    fn max_input_length() {
        init_logger();

        let test = TestMp4::with_mdat_data_len(b"", Some(u64::MAX - 16));
        let test @ TestMp4 { mdat, .. } = TestMp4::with_mdat_data_len(b"", Some(u64::MAX - test.data.len() as u64));
        let sanitized = sanitize(test).unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitized.data.offset + sanitized.data.len, u64::MAX);
    }

    #[test]
    fn input_length_overflow() {
        init_logger();

        let test = TestMp4::with_mdat_data_len(b"", Some(u64::MAX - 16));
        let test = TestMp4::with_mdat_data_len(b"", Some(u64::MAX - test.data.len() as u64 + 1));
        sanitize(test).unwrap_err();
    }

    #[test]
    fn box_size_overflow() {
        init_logger();

        let test = TestMp4::with_mdat_data_len(b"", Some(u64::MAX - 16));
        sanitize(test).unwrap_err();
    }
}
