mod parse;
mod util;

use std::io::{self, BufRead, BufReader, Read, Seek};

use bytes::{Buf, BufMut, BytesMut};
use mp4::{BoxType, FourCC, FtypBox, MoovBox, ReadBox, WriteBox};

use crate::parse::BoxHeader;
use crate::util::checked_add_signed;

//
// public types
//

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

const MAX_READ_BOX_SIZE: u64 = 200 * 1024 * 1024;

//
// public functions
//

pub fn sanitize<R: Read + Skip>(input: R) -> Result<SanitizedMetadata, Error> {
    let mut reader = BufReader::with_capacity(BoxHeader::MAX_SIZE as usize, input);

    let mut ftyp: Option<FtypBox> = None;
    let mut moov: Option<MoovBox> = None;
    let mut data: Option<InputSpan> = None;

    while !reader.fill_buf()?.is_empty() {
        let start_pos = reader.stream_position()?;

        let header = BoxHeader::read(&mut reader)?;

        match header.box_type() {
            BoxType::FreeBox => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("free @ 0x{start_pos:08x}: {box_size} bytes");

                // Try to extend any already accumulated data in case there's more mdat boxes to come.
                if let Some(data) = &mut data {
                    if data.offset + data.len == start_pos {
                        data.len += box_size;
                    }
                }
            }

            BoxType::FtypBox if ftyp.is_some() => return Err(Error::InvalidBoxLayout("multiple ftyp boxes")),
            BoxType::FtypBox => {
                let read_ftyp = read_box(&mut reader, &header)?;
                log::info!("ftyp @ 0x{start_pos:08x}: {read_ftyp:#?}");
                ftyp = Some(read_ftyp);
            }

            // NB: ISO 14496-12-2012 specifies a default ftyp, but we don't currently use it. The spec says that it
            // contains a single compatible brand, "mp41", and notably not "isom" which is the ISO spec we follow for
            // parsing now. This implies that there's additional stuff in "mp41" which is not in "isom". "mp41" is also
            // very old at this point, so it'll require additional research/work to be able to parse/remux it.
            _ if ftyp.is_none() => return Err(Error::InvalidBoxLayout("ftyp is not the first significant box")),

            BoxType::MdatBox => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("mdat @ 0x{start_pos:08x}: {box_size} bytes");

                if let Some(data) = &mut data {
                    // Try to extend already accumulated data.
                    if data.offset + data.len != start_pos {
                        return Err(Error::UnsupportedBoxLayout("discontiguous mdat boxes"));
                    }
                    data.len += box_size;
                } else {
                    data = Some(InputSpan { offset: start_pos, len: box_size });
                }
            }
            BoxType::MoovBox => {
                let read_moov = read_box(&mut reader, &header)?;
                log::info!("moov @ 0x{start_pos:08x}: {read_moov:#?}");
                moov = Some(read_moov);
            }
            name => {
                let box_size = skip_box(&mut reader, &header)? + header.encoded_len();
                log::info!("{name} @ 0x{start_pos:08x}: {box_size} bytes");
                return Err(Error::UnsupportedBox(name));
            }
        }
    }

    let Some(ftyp) = ftyp else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::FtypBox)));
    };
    if !ftyp.compatible_brands.contains(&COMPATIBLE_BRAND) {
        return Err(Error::UnsupportedFormat(ftyp.major_brand));
    };
    let Some(mut moov) = moov else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MoovBox)));
    };
    let Some(data) = data else {
        return Err(Error::Parse(mp4::Error::BoxNotFound(BoxType::MdatBox)));
    };

    // Add a free box to pad, if one will fit, if the mdat box would move backward. If one won't fit, or if the mdat box
    // would move forward, adjust mdat offsets in stco/co64 the amount it was displaced.
    let metadata_len = ftyp.get_size() + moov.get_size();
    let mut pad_size = 0;
    const PAD_HEADER_SIZE: u64 = BoxHeader::with_u32_data_size(BoxType::FreeBox, 0).encoded_len();
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
            let mdat_displacement: i32 =
                mdat_displacement.ok_or(Error::UnsupportedBoxLayout("mdat displaced too far"))?;

            for trak in &mut moov.traks {
                if let Some(stco) = &mut trak.mdia.minf.stbl.stco {
                    for entry in &mut stco.entries {
                        *entry = checked_add_signed(*entry, mdat_displacement)
                            .ok_or(mp4::Error::InvalidData("chunk offset not within mdat"))?;
                    }
                } else if let Some(co64) = &mut trak.mdia.minf.stbl.co64 {
                    for entry in &mut co64.entries {
                        *entry = checked_add_signed(*entry, mdat_displacement.into())
                            .ok_or(mp4::Error::InvalidData("chunk offset not within mdat"))?;
                    }
                }
            }
        }
    }

    let mut metadata = Vec::with_capacity((metadata_len + pad_size) as usize);
    ftyp.write_box(&mut metadata)?;
    moov.write_box(&mut metadata)?;
    if pad_size != 0 {
        let pad_header = BoxHeader::with_u32_data_size(BoxType::FreeBox, (pad_size - PAD_HEADER_SIZE) as u32);
        pad_header.write(&mut metadata);
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
fn read_box<R, T>(reader: &mut BufReader<R>, header: &BoxHeader) -> Result<T, Error>
where
    R: Read + Skip,
    T: for<'a> ReadBox<&'a mut io::Cursor<BytesMut>>,
{
    let box_data_size = match header.box_data_size()? {
        Some(box_data_size) => box_data_size,
        None => reader.stream_len()? - reader.stream_position()?,
    };

    if box_data_size > MAX_READ_BOX_SIZE {
        return Err(mp4::Error::InvalidData("box too large").into());
    }

    let mut buf = BytesMut::with_capacity((header.encoded_len() + box_data_size) as usize);
    header.write(&mut buf);
    buf.put_bytes(0, box_data_size as usize);
    reader.read_exact(&mut buf[header.encoded_len() as usize..])?;

    let mut cursor = io::Cursor::new(buf);
    cursor.advance(header.encoded_len() as usize);

    // `read_box` actually expects `box_data_size + HEADER_SIZE` as a hack to handle extended box headers.
    let mp4box = T::read_box(&mut cursor, box_data_size + mp4::HEADER_SIZE)?;
    Ok(mp4box)
}

/// Skip a box's data assuming its header has already been read.
///
/// Returns the amount of data that was skipped.
fn skip_box<R: Read + Skip>(reader: &mut BufReader<R>, header: &BoxHeader) -> Result<u64, Error> {
    let box_data_size = match header.box_data_size()? {
        Some(box_size) => box_size,
        None => reader.stream_len()? - reader.stream_position()?,
    };
    reader.skip(box_data_size)?;
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

#[cfg(test)]
mod test {
    use bytes::Bytes;
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
        let header = match data_len {
            Some(data_len) => BoxHeader::with_data_size(BoxType::MdatBox, data_len).unwrap(),
            None => BoxHeader::until_eof(BoxType::MdatBox),
        };
        header.write(&mut *out);
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
            test_ftyp().write_box(&mut data).unwrap();
            MoovBox::default().write_box(&mut data).unwrap();
            let mdat = write_test_mdat(&mut data, mdat_data);
            Self { data_len: data.len() as u64, data: data.into(), mdat, mdat_skipped: 0 }
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
        test_ftyp().write_box(&mut data).unwrap();
        let mdat = write_test_mdat(&mut data, b"abcdefg");

        let moov_pos = data.len();
        MoovBox::default().write_box(&mut data).unwrap();
        BoxHeader::until_eof(BoxType::MoovBox).write(&mut &mut data[moov_pos..]);

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
