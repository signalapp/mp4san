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
    use assert_matches::assert_matches;

    use crate::parse::box_type::{CO64, FREE, FTYP, MDAT, MDIA, MINF, MOOV, STBL, STCO, TRAK};
    use crate::util::test::{
        init_logger, sanitized_data, test_ftyp, test_moov, test_mp4, write_test_mdat, ISOM, MP41, MP42, TEST_UUID,
    };

    use super::*;

    #[test]
    fn until_eof_sized_moov() {
        init_logger();

        let mut data = vec![];
        let mut metadata = vec![];
        test_ftyp().build().put_buf(&mut data);
        test_ftyp().build().put_buf(&mut metadata);
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
        let test = test_mp4().mdat_data(&b"abcdefg"[..]).mdat_data_until_eof().build();
        test.sanitize_ok();
    }

    #[test]
    fn skip() {
        test_mp4().mdat_data(&b"abcdefg"[..]).build().sanitize_ok();
    }

    #[test]
    fn max_input_length() {
        let mut test = test_mp4().mdat_data(vec![]).clone();
        let test_data_len = test.mdat_data_len(u64::MAX - 16).build().data.len() as u64;
        let test = test.mdat_data_len(u64::MAX - test_data_len).build();
        let sanitized = sanitize(test.clone()).unwrap();
        assert_eq!(sanitized.data, test.mdat);
        assert_eq!(sanitized.data.offset + sanitized.data.len, u64::MAX);
        assert_eq!(sanitized.metadata, test.expected_metadata);
    }

    #[test]
    fn input_length_overflow() {
        let mut test = test_mp4().mdat_data(vec![]).clone();
        let test_data_len = test.mdat_data_len(u64::MAX - 16).build().data.len() as u64;
        let test = test.mdat_data_len(u64::MAX - test_data_len + 1).build();
        sanitize(test).unwrap_err();
    }

    #[test]
    fn box_size_overflow() {
        let test = test_mp4().mdat_data_len(u64::MAX - 16).build();
        sanitize(test).unwrap_err();
    }

    #[test]
    fn mdat_before_moov() {
        test_mp4().boxes(&[FTYP, MDAT, MOOV][..]).build().sanitize_ok();
    }

    #[test]
    fn no_ftyp() {
        let test = test_mp4().boxes(&[MOOV, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn multiple_ftyp() {
        let test = test_mp4().boxes(&[FTYP, FTYP, MOOV, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn ftyp_not_first_box() {
        let test = test_mp4().boxes(&[FREE, FREE, FTYP, MOOV, MDAT][..]).build();
        test.sanitize_ok();
    }

    #[test]
    fn ftyp_not_first_significant_box() {
        let test = test_mp4().boxes(&[MOOV, FTYP, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }

    #[test]
    fn no_moov() {
        let test = test_mp4().boxes(&[FTYP, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MOOV));
        });
    }

    #[test]
    fn no_mdat() {
        let test = test_mp4().boxes(&[FTYP, MOOV][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(MDAT));
        });
    }

    #[test]
    fn free_boxes_in_metadata() {
        let test = test_mp4().boxes(&[FTYP, FREE, FREE, MOOV, FREE, MDAT][..]).build();
        test.sanitize_ok();
    }

    #[test]
    fn free_boxes_after_mdat() {
        let test = test_mp4().boxes(&[FTYP, MOOV, MDAT, FREE][..]).build();
        test.sanitize_ok();
    }

    #[test]
    fn multiple_mdat() {
        test_mp4()
            .boxes(&[FTYP, MOOV, MDAT, FREE, MDAT, MDAT, FREE][..])
            .build()
            .sanitize_ok();
    }

    #[test]
    fn uuid() {
        let test = test_mp4().boxes(&[FTYP, MOOV, TEST_UUID, MDAT][..]).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedBox(TEST_UUID));
        });
    }

    #[test]
    fn mp41() {
        let test = test_mp4()
            .ftyp(test_ftyp().major_brand(MP41).add_compatible_brand(MP41).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedFormat(MP41));
        });
    }

    #[test]
    fn mp42() {
        let ftyp = test_ftyp()
            .major_brand(MP42)
            .compatible_brands(vec![MP42, ISOM])
            .clone();
        let test = test_mp4().ftyp(ftyp).build();
        test.sanitize_ok();
    }

    #[test]
    fn no_compatible_brands() {
        let test = test_mp4()
            .ftyp(test_ftyp().major_brand(ISOM).compatible_brands(vec![]).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::UnsupportedFormat(ISOM));
        });
    }

    #[test]
    fn no_trak() {
        let test = test_mp4().moov(test_moov().trak(false).clone()).build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::MissingRequiredBox(TRAK));
        });
    }

    #[test]
    fn no_mdia() {
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
        test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().stco(false).co64(true).clone())
            .build()
            .sanitize_ok();
    }

    #[test]
    fn stco_and_co64() {
        let test = test_mp4()
            .boxes(&[FTYP, MDAT, MOOV][..])
            .moov(test_moov().co64(true).clone())
            .build();
        assert_matches!(sanitize(test).unwrap_err(), Error::Parse(err) => {
            assert_matches!(err.current_context(), ParseError::InvalidBoxLayout);
        });
    }
}
