#[macro_use]
extern crate error_stack;

#[macro_use]
mod macros;

pub mod parse;
mod sync;
mod util;

use std::future::poll_fn;
use std::io;
use std::io::{Read, Seek};
use std::pin::Pin;
use std::task::{ready, Context, Poll};

use derive_more::Display;
use error_stack::Report;
use futures::io::BufReader;
use futures::{pin_mut, AsyncBufRead, AsyncBufReadExt, AsyncRead, AsyncSeek};

use crate::parse::error::{MultipleBoxes, WhileParsingBox};
use crate::parse::{BoxHeader, BoxType, FourCC, FtypBox, MoovBox, Mp4Box, ParseError, StblCoMut};
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

/// A subset of the [`AsyncSeek`] trait, providing a cursor which can skip forward within a stream of bytes.
///
/// [`AsyncSkip`] is automatically implemented for all types implementing [`AsyncSeek`].
pub trait AsyncSkip {
    /// Skip an amount of bytes in a stream.
    ///
    /// A skip beyond the end of a stream is allowed, but behavior is defined by the implementation.
    fn poll_skip(self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>>;

    /// Returns the current position of the cursor from the start of the stream.
    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>>;

    /// Returns the length of this stream, in bytes.
    fn poll_stream_len(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>>;
}

pub const COMPATIBLE_BRAND: FourCC = FourCC { value: *b"isom" };

//
// private types
//

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "box data too large: {} > {}", _0, MAX_READ_BOX_SIZE)]
struct BoxDataTooLarge(u64);

const MAX_READ_BOX_SIZE: u64 = 200 * 1024 * 1024;

//
// public functions
//

pub fn sanitize<R: Read + Skip + Unpin>(input: R) -> Result<SanitizedMetadata, Error> {
    sync::sanitize(input)
}

pub async fn sanitize_async<R: AsyncRead + AsyncSkip>(input: R) -> Result<SanitizedMetadata, Error> {
    let reader = BufReader::with_capacity(BoxHeader::MAX_SIZE as usize, input);
    pin_mut!(reader);

    let mut ftyp: Option<Mp4Box<FtypBox>> = None;
    let mut moov: Option<Mp4Box<MoovBox>> = None;
    let mut data: Option<InputSpan> = None;

    while !reader.as_mut().fill_buf().await?.is_empty() {
        let start_pos = stream_position(reader.as_mut()).await?;

        let header = BoxHeader::read(&mut reader)
            .await
            .map_eof(|_| Error::Parse(report_attach!(ParseError::TruncatedBox, "while parsing box header")))?;

        match header.box_type() {
            BoxType::FREE => {
                let box_size = skip_box(reader.as_mut(), &header).await? + header.encoded_len();
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
                let mut read_ftyp = Mp4Box::read_data(reader.as_mut(), header).await?;
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
                let box_size = skip_box(reader.as_mut(), &header).await? + header.encoded_len();
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
                let mut read_moov = Mp4Box::read_data(reader.as_mut(), header).await?;
                let moov_data = read_moov.data.parse()?;
                log::info!("moov @ 0x{start_pos:08x}: {moov_data:#?}");
                moov = Some(read_moov);
            }
            name => {
                let box_size = skip_box(reader.as_mut(), &header).await? + header.encoded_len();
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
    let Some(moov) = moov else {
        return Err(report!(ParseError::MissingRequiredBox(BoxType::MOOV)).into());
    };
    let Some(data) = data else {
        return Err(report!(ParseError::MissingRequiredBox(BoxType::MDAT)).into());
    };

    // Make sure none of the metadata boxes use BoxSize::UntilEof, as we want the caller to be able to concatenate movie
    // data to the end of the metadata.
    let ftyp = Mp4Box::with_data(ftyp.data)?;
    let mut moov = Mp4Box::with_data(moov.data)?;

    // Add a free box to pad, if one will fit, if the mdat box would move backward. If one won't fit, or if the mdat box
    // would move forward, adjust mdat offsets in stco/co64 the amount it was displaced.
    let metadata_len = ftyp.encoded_len() + moov.encoded_len();
    let mut pad_size = 0;
    const PAD_HEADER_SIZE: u64 = BoxHeader::with_u32_data_size(BoxType::FREE, 0).encoded_len();
    const MAX_PAD_SIZE: u64 = u32::MAX as u64 - PAD_HEADER_SIZE;
    match data.offset.checked_sub(metadata_len) {
        Some(0) => {
            log::info!("metadata: 0x{metadata_len:08x} bytes");
        }
        Some(size @ PAD_HEADER_SIZE..=MAX_PAD_SIZE) => {
            pad_size = size;
            log::info!("metadata: 0x{metadata_len:08x} bytes; adding padding of 0x{pad_size:08x} bytes");
        }
        mdat_backward_displacement => {
            let mdat_displacement = match mdat_backward_displacement {
                Some(mdat_backward_displacement) => {
                    mdat_backward_displacement.try_into().ok().and_then(i32::checked_neg)
                }
                None => metadata_len.checked_sub(data.offset).unwrap().try_into().ok(),
            };
            let mdat_displacement: i32 = mdat_displacement
                .ok_or_else(|| report_attach!(ParseError::UnsupportedBoxLayout, "mdat displaced too far"))?;

            log::info!("metadata: 0x{metadata_len:08x} bytes; displacing chunk offsets by 0x{mdat_displacement:08x}");

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
// AsyncSkip impls
//

impl<T: AsyncSeek> AsyncSkip for T {
    fn poll_skip(mut self: Pin<&mut Self>, cx: &mut Context<'_>, amount: u64) -> Poll<io::Result<()>> {
        match amount.try_into() {
            Ok(0) => (),
            Ok(amount) => {
                ready!(self.poll_seek(cx, io::SeekFrom::Current(amount)))?;
            }
            Err(_) => {
                let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
                let seek_pos = stream_pos
                    .checked_add(amount)
                    .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "seek past u64::MAX"))?;
                ready!(self.poll_seek(cx, io::SeekFrom::Start(seek_pos)))?;
            }
        }
        Ok(()).into()
    }

    fn poll_stream_position(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        self.poll_seek(cx, io::SeekFrom::Current(0))
    }

    fn poll_stream_len(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<u64>> {
        // This is the unstable Seek::stream_len
        let stream_pos = ready!(self.as_mut().poll_stream_position(cx))?;
        let len = ready!(self.as_mut().poll_seek(cx, io::SeekFrom::End(0)))?;

        if stream_pos != len {
            ready!(self.poll_seek(cx, io::SeekFrom::Start(stream_pos)))?;
        }

        Ok(len).into()
    }
}

//
// private functions
//

/// Skip a box's data assuming its header has already been read.
///
/// Returns the amount of data that was skipped.
async fn skip_box<R: AsyncRead + AsyncSkip>(
    mut reader: Pin<&mut BufReader<R>>,
    header: &BoxHeader,
) -> Result<u64, Error> {
    let box_data_size = match header.box_data_size()? {
        Some(box_size) => box_size,
        None => stream_len(reader.as_mut()).await? - stream_position(reader.as_mut()).await?,
    };
    skip(reader, box_data_size).await.map_eof(|_| {
        Error::Parse(report_attach!(
            ParseError::TruncatedBox,
            WhileParsingBox(header.box_type())
        ))
    })?;
    Ok(box_data_size)
}

async fn skip<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>, amount: u64) -> io::Result<()> {
    let buf_len = reader.buffer().len();
    if let Some(skip_amount) = amount.checked_sub(buf_len as u64) {
        if skip_amount != 0 {
            poll_fn(|cx| reader.as_mut().get_pin_mut().poll_skip(cx, skip_amount)).await?;
        }
    }
    reader.consume(buf_len.min(amount as usize));
    Ok(())
}

async fn stream_position<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>) -> io::Result<u64> {
    let stream_pos = poll_fn(|cx| reader.as_mut().get_pin_mut().poll_stream_position(cx)).await?;
    Ok(stream_pos.saturating_sub(reader.buffer().len() as u64))
}

async fn stream_len<R: AsyncRead + AsyncSkip>(mut reader: Pin<&mut BufReader<R>>) -> io::Result<u64> {
    poll_fn(|cx| reader.as_mut().get_pin_mut().poll_stream_len(cx)).await
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
    use std::iter;

    use assert_matches::assert_matches;
    use bytes::{Buf, Bytes};
    use derive_builder::Builder;

    use crate::parse::box_type::*;
    use crate::parse::{BoxUuid, Co64Box, MdiaBox, MinfBox, StblBox, StcoBox, TrakBox};
    use crate::util::test::init_logger;

    use super::*;

    const TEST_UUID: BoxType = BoxType::Uuid(BoxUuid(*b"thisisatestuuid!"));
    const MP42: FourCC = FourCC { value: *b"mp42" };
    const MP41: FourCC = FourCC { value: *b"mp41" };
    const ISOM: FourCC = FourCC { value: *b"isom" };

    fn test_ftyp(major_brand: FourCC, compatible_brands: Vec<FourCC>) -> Mp4Box<FtypBox> {
        Mp4Box::with_data(FtypBox::new(major_brand, 0, compatible_brands).into()).unwrap()
    }

    fn test_moov() -> TestMoovBuilder {
        Default::default()
    }

    #[derive(Builder)]
    #[builder(name = "TestMoovBuilder", build_fn(name = "build_spec"))]
    struct TestMoovSpec {
        #[builder(default)]
        #[builder(setter(into))]
        co_entries: Vec<u64>,

        #[builder(default = "true")]
        stco: bool,

        #[builder(default)]
        co64: bool,

        #[builder(default = "true")]
        stbl: bool,

        #[builder(default = "true")]
        minf: bool,

        #[builder(default = "true")]
        mdia: bool,

        #[builder(default = "true")]
        trak: bool,
    }

    impl TestMoovBuilder {
        fn build(&self) -> Mp4Box<MoovBox> {
            let spec = self.build_spec().unwrap();

            let mut co = vec![];
            if spec.co64 {
                let entries = spec.co_entries.iter().cloned();
                co.push(Mp4Box::with_data(Co64Box::with_entries(entries).into()).unwrap().into());
            }
            if spec.stco {
                let entries = spec.co_entries.into_iter().map(|entry| entry as u32);
                co.push(Mp4Box::with_data(StcoBox::with_entries(entries).into()).unwrap().into());
            }
            let stbl = match spec.stbl {
                true => vec![Mp4Box::with_data(StblBox::with_children(co).into()).unwrap().into()],
                false => vec![],
            };
            let minf = match spec.minf {
                true => vec![Mp4Box::with_data(MinfBox::with_children(stbl).into()).unwrap().into()],
                false => vec![],
            };
            let mdia = match spec.mdia {
                true => vec![Mp4Box::with_data(MdiaBox::with_children(minf).into()).unwrap().into()],
                false => vec![],
            };
            let trak = match spec.trak {
                true => vec![Mp4Box::with_data(TrakBox::with_children(mdia).into()).unwrap().into()],
                false => vec![],
            };
            Mp4Box::with_data(MoovBox::with_children(trak).into()).unwrap()
        }
    }

    fn test_mp4() -> TestMp4Builder {
        Default::default()
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
            Some(data_len) => BoxHeader::with_data_size(MDAT, data_len).unwrap(),
            None => BoxHeader::until_eof(MDAT),
        };
        header.put_buf(&mut *out);
        InputSpan { offset, len: out.len() as u64 - offset }
    }

    fn write_test_free(mut out: &mut Vec<u8>, len: u32) {
        const FREE_HEADER_SIZE: u32 = BoxHeader::with_u32_data_size(FREE, 0).encoded_len() as u32;
        BoxHeader::with_u32_data_size(FREE, len - FREE_HEADER_SIZE).put_buf(&mut out);
        out.extend(iter::repeat(0).take((len - FREE_HEADER_SIZE) as usize));
    }

    fn write_test_uuid(out: &mut Vec<u8>) {
        BoxHeader::with_u32_data_size(TEST_UUID, 0).put_buf(out);
    }

    fn sanitized_data(sanitized: SanitizedMetadata, data: &[u8]) -> Vec<u8> {
        let mdat = &data[sanitized.data.offset as usize..][..sanitized.data.len as usize];
        [&sanitized.metadata[..], mdat].concat()
    }

    #[derive(Builder)]
    #[builder(name = "TestMp4Builder", build_fn(name = "build_spec"))]
    struct TestMp4Spec {
        #[builder(default = "ISOM")]
        major_brand: FourCC,

        #[builder(default = "vec![ISOM]")]
        #[builder(setter(into, each(name = "add_compatible_brand")))]
        compatible_brands: Vec<FourCC>,

        #[builder(default)]
        moov: TestMoovBuilder,

        #[builder(default)]
        #[builder(setter(into, each(name = "add_mdat_data", into)))]
        mdat_data: Vec<u8>,

        #[builder(default = "Some(self.mdat_data.as_deref().unwrap_or_default().len() as u64)")]
        #[builder(setter(strip_option))]
        mdat_data_len: Option<u64>,

        #[builder(default = "vec![FTYP, MOOV, MDAT]")]
        #[builder(setter(into, each(name = "add_box")))]
        boxes: Vec<BoxType>,
    }

    #[derive(Clone)]
    struct TestMp4 {
        data: Bytes,
        data_len: u64,
        expected_metadata: Bytes,
        mdat: InputSpan,
        mdat_skipped: u64,
    }

    impl TestMp4Builder {
        fn mdat_data_until_eof(&mut self) -> &mut Self {
            self.mdat_data_len = Some(None);
            self
        }

        fn build(&self) -> TestMp4 {
            let spec = self.build_spec().unwrap();
            let mut moov = spec.moov;
            moov.co_entries(vec![0]);

            let mut data = vec![];
            let mut mdat: Option<InputSpan> = None;
            let mut moov_offsets = Vec::new();
            let mut metadata_free_len = 0;
            for box_type in &spec.boxes {
                match *box_type {
                    FTYP => {
                        test_ftyp(spec.major_brand, spec.compatible_brands.clone()).put_buf(&mut data);
                    }
                    MOOV => {
                        moov_offsets.push(data.len());
                        moov.build().put_buf(&mut data);
                    }
                    MDAT => {
                        let written_mdat = write_mdat_header(&mut data, spec.mdat_data_len);
                        data.extend_from_slice(&spec.mdat_data);

                        let mdat_data_len = spec.mdat_data_len.unwrap_or(spec.mdat_data.len() as u64);
                        let mdat_len = written_mdat.len.saturating_add(mdat_data_len);
                        match &mut mdat {
                            Some(mdat) => mdat.len += mdat_len,
                            None => mdat = Some(InputSpan { len: mdat_len, ..written_mdat }),
                        }
                    }
                    FREE => {
                        let free_len = 13;
                        write_test_free(&mut data, free_len);
                        match &mut mdat {
                            Some(mdat) => mdat.len += free_len as u64,
                            None => metadata_free_len += free_len,
                        }
                    }
                    TEST_UUID => {
                        write_test_uuid(&mut data);
                    }
                    _ => panic!("invalid box type for test {box_type}"),
                }
            }

            let mdat = mdat.unwrap_or(InputSpan { offset: data.len() as u64, len: 0 });

            // Calculate and write correct chunk offsets
            let mut co_entries = moov.build_spec().unwrap().co_entries;
            for co_entry in &mut co_entries {
                *co_entry += mdat.offset;
            }
            for moov_offset in &moov_offsets {
                let moov = moov.co_entries(co_entries.clone()).build();
                moov.put_buf(&mut data[*moov_offset..]);
            }

            // Calculate expected output metadata. NB: The expectation that the output metadata matches the input
            // metadata verbatim is overly-strict and could be weakened.
            let mut expected_metadata = vec![];
            test_ftyp(spec.major_brand, spec.compatible_brands).put_buf(&mut expected_metadata);
            let mut expected_metadata_moov_offsets = Vec::new();
            for _ in moov_offsets {
                expected_metadata_moov_offsets.push(expected_metadata.len());
                let moov = moov.co_entries(co_entries.clone()).build();
                moov.put_buf(&mut expected_metadata);
            }
            if metadata_free_len != 0 {
                write_test_free(&mut expected_metadata, metadata_free_len);
            }

            // Calculate and write correct expected output chunk offsets
            for co_entry in &mut co_entries {
                *co_entry -= mdat.offset;
                *co_entry += expected_metadata.len() as u64;
            }
            for expected_metadata_moov_offset in expected_metadata_moov_offsets {
                let moov = moov.co_entries(co_entries.clone()).build();
                moov.put_buf(&mut expected_metadata[expected_metadata_moov_offset..]);
            }

            TestMp4 {
                data_len: data.len() as u64,
                data: data.into(),
                expected_metadata: expected_metadata.into(),
                mdat,
                mdat_skipped: 0,
            }
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
    fn until_eof_sized_moov() {
        init_logger();

        let mut data = vec![];
        let mut metadata = vec![];
        test_ftyp(ISOM, vec![ISOM]).put_buf(&mut data);
        test_ftyp(ISOM, vec![ISOM]).put_buf(&mut metadata);
        let mdat = write_test_mdat(&mut data, b"abcdefg");

        let moov_pos = data.len();
        test_moov().build().put_buf(&mut data);
        test_moov().build().put_buf(&mut metadata);
        BoxHeader::until_eof(MOOV).put_buf(&mut &mut data[moov_pos..]);

        let sanitized = sanitize(io::Cursor::new(&data)).unwrap();
        assert_eq!(sanitized.data, mdat);
        assert_eq!(sanitized.metadata, metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &data))).unwrap();
    }

    #[test]
    fn until_eof_sized_mdat() {
        init_logger();

        let test = test_mp4().mdat_data(&b"abcdefg"[..]).mdat_data_until_eof().build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn skip() {
        init_logger();

        let test = test_mp4().mdat_data(&b"abcdefg"[..]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn max_input_length() {
        init_logger();

        let test = test_mp4().mdat_data_len(u64::MAX - 16).build();
        let test = test_mp4().mdat_data_len(u64::MAX - test.data.len() as u64).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.data.offset + sanitized.data.len, u64::MAX);
        assert_eq!(sanitized.metadata, test.expected_metadata);
    }

    #[test]
    fn input_length_overflow() {
        init_logger();

        let test = test_mp4().mdat_data_len(u64::MAX - 16).build();
        let test = test_mp4().mdat_data_len(u64::MAX - test.data.len() as u64 + 1).build();
        sanitize(test).unwrap_err();
    }

    #[test]
    fn box_size_overflow() {
        init_logger();

        let test = test_mp4().mdat_data_len(u64::MAX - 16).build();
        sanitize(test).unwrap_err();
    }

    #[test]
    fn mdat_before_moov() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, MDAT, MOOV][..]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn no_ftyp() {
        init_logger();

        let test = test_mp4().boxes(&[MOOV, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn multiple_ftyp() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, FTYP, MOOV, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn ftyp_not_first_box() {
        init_logger();

        let test = test_mp4().boxes(&[FREE, FREE, FTYP, MOOV, MDAT][..]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn ftyp_not_first_significant_box() {
        init_logger();

        let test = test_mp4().boxes(&[MOOV, FTYP, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn no_moov() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MOOV));
        });
    }

    #[test]
    fn no_mdat() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, MOOV][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MDAT));
        });
    }

    #[test]
    fn free_boxes_in_metadata() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, FREE, FREE, MOOV, FREE, MDAT][..]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn free_boxes_after_mdat() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, MOOV, MDAT, FREE][..]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn multiple_mdat() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MOOV, MDAT, FREE, MDAT, MDAT, FREE][..])
            .build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn uuid() {
        init_logger();

        let test = test_mp4().boxes(&[FTYP, MOOV, TEST_UUID, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedBox(TEST_UUID));
        });
    }

    #[test]
    fn mp41() {
        init_logger();

        let test = test_mp4().major_brand(MP41).add_compatible_brand(MP41).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedFormat(MP41));
        });
    }

    #[test]
    fn mp42() {
        init_logger();

        let test = test_mp4().major_brand(MP42).compatible_brands(vec![MP42, ISOM]).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn no_compatible_brands() {
        init_logger();

        let test = test_mp4().major_brand(ISOM).compatible_brands(vec![]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedFormat(ISOM));
        });
    }

    #[test]
    fn no_trak() {
        init_logger();

        let test = test_mp4().moov(test_moov().trak(false).clone()).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(TRAK));
        });
    }

    #[test]
    fn no_mdia() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().mdia(false).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MDIA));
        });
    }

    #[test]
    fn no_minf() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().minf(false).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MINF));
        });
    }

    #[test]
    fn no_stbl() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().stbl(false).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(STBL));
        });
    }

    #[test]
    fn no_stco() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().stco(false).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(STCO | CO64));
        });
    }

    #[test]
    fn co64() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().stco(false).co64(true).clone())
            .build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.metadata, test.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized, &test.data))).unwrap();
    }

    #[test]
    fn stco_and_co64() {
        init_logger();

        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().co64(true).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }
}
