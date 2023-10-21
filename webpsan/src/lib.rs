#![warn(missing_docs)]

//! `webpsan` is a WebP format "sanitizer".
//!
//! The sanitizer currently simply checks the validity of a WebP file input, so that passing a malformed filed to an
//! unsafe parser can be avoided.
//!
//! # Usage
//!
//! The main entry points to the sanitizer is [`sanitize`], which take a [`Read`] + [`Skip`] input. The [`Skip`] trait
//! represents a subset of the [`Seek`] trait; an input stream which can be skipped forward, but not necessarily seeked
//! to arbitrary positions.
//!
//! ```
//! let example_input = b"RIFF\x14\0\0\0WEBPVP8L\x08\0\0\0\x2f\0\0\0\0\x88\x88\x08";
//! webpsan::sanitize(std::io::Cursor::new(example_input))?;
//! # Ok::<(), webpsan::Error>(())
//! ```
//!
//! The [`parse`] module also contains a less stable and undocumented API which can be used to parse individual WebP
//! chunk types.
//!
//! [`Seek`]: std::io::Seek

pub mod parse;
mod reader;
mod util;

use std::io::Read;
use std::num::{NonZeroU16, NonZeroU32};

use derive_builder::Builder;
use derive_more::Display;
use mediasan_common::error::{ExtraUnparsedInput, WhileParsingType};
use mediasan_common::{bail_attach, ensure_attach, ensure_matches_attach, InputSpan, ResultExt};
use parse::error::WhileParsingChunk;

use crate::parse::chunk_type::{ALPH, ANIM, ANMF, EXIF, ICCP, RIFF, VP8, VP8L, VP8X, XMP};
use crate::parse::error::MultipleChunks;
use crate::parse::{AlphChunk, AnimChunk, AnmfChunk, ParseError, Vp8lChunk, Vp8xChunk, Vp8xFlags, WebpChunk};
use crate::reader::ChunkReader;

//
// public types
//

/// Error type returned by `webpsan`.
pub type Error = mediasan_common::error::Error<ParseError>;

#[derive(Builder, Clone)]
#[builder(build_fn(name = "try_build"))]
/// Configuration for the WebP sanitizer.
pub struct Config {
    /// Whether to allow unknown chunk types at allowed positions during parsing.
    ///
    /// The default is `false`.
    #[builder(default)]
    pub allow_unknown_chunks: bool,
}

pub use mediasan_common::{Report, SeekSkipAdapter, Skip};

/// Maximum file length as permitted by WebP.
pub const MAX_FILE_LEN: u32 = u32::MAX - 2;

//
// private types
//

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "frame dimensions `{_0}`x`{_1}` do not match canvas dimensions `{_2}`x`{_3}`")]
struct FrameDimensionsMismatch(NonZeroU16, NonZeroU16, NonZeroU32, NonZeroU32);

//
// public functions
//

/// Sanitize a WebP input.
///
/// The `input` must implement [`Read`] + [`Skip`], where [`Skip`] represents a subset of the [`Seek`] trait; an input
/// stream which can be skipped forward, but not necessarily seeked to arbitrary positions.
///
/// See the [module-level documentation](self) for usage examples.
///
/// # Errors
///
/// If the input cannot be parsed, or an IO error occurs, an [`Error`] is returned.
///
/// [`Seek`]: std::io::Seek
pub fn sanitize<R: Read + Skip>(input: R) -> Result<(), Error> {
    sanitize_with_config(input, Config::default())
}

/// Sanitize a WebP input, with the given [`Config`].
///
/// The `input` must implement [`Read`] + [`Skip`], where [`Skip`] represents a subset of the [`Seek`] trait; an input
/// stream which can be skipped forward, but not necessarily seeked to arbitrary positions.
///
/// # Errors
///
/// If the input cannot be parsed, or an IO error occurs, an [`Error`] is returned.
///
/// [`Seek`]: std::io::Seek
pub fn sanitize_with_config<R: Read + Skip>(input: R, config: Config) -> Result<(), Error> {
    let mut file_reader = ChunkReader::new(input, RIFF);
    let InputSpan { offset, len } = file_reader.read_header(RIFF)?;
    let WebpChunk = file_reader.parse_data()?;

    ensure_attach!(
        len <= MAX_FILE_LEN.into(),
        ParseError::InvalidInput,
        WhileParsingChunk(RIFF)
    );

    let mut reader = file_reader.child_reader();

    log::info!("{name} @ 0x{offset:08x}: {len} bytes", name = RIFF);

    let (name, InputSpan { offset, len }) = reader.read_any_header().attach_printable("while parsing first chunk")?;
    match name {
        VP8 => {
            reader.skip_data()?;
            log::info!("{name} @ 0x{offset:08x}: {len} bytes");
        }
        VP8L => {
            let vp8l @ Vp8lChunk { .. } = reader.parse_data()?;
            let (width, height) = (vp8l.width(), vp8l.height());
            vp8l.sanitize_image_data(reader.data_reader())?;
            reader.skip_data()?;
            log::info!("{name} @ 0x{offset:08x}: {len} bytes, {width}x{height}");
        }
        VP8X => {
            let vp8x @ Vp8xChunk { flags, .. } = reader.parse_data()?;
            let (width, height) = (vp8x.canvas_width(), vp8x.canvas_height());
            log::info!("{name} @ 0x{offset:08x}: {width}x{height}, flags {flags:08b}");

            sanitize_extended(&mut reader, &vp8x, &config)?
        }
        _ => {
            log::info!("{name} @ 0x{offset:08x}: {len} bytes");
            bail_attach!(
                ParseError::InvalidChunkLayout,
                "expected image data or VP8X",
                WhileParsingChunk(name),
            );
        }
    }

    // It's not clear whether the WebP spec accepts unknown chunks at the end of simple format files, but many of the
    // WebP test vectors contain non-standard trailing informational chunks.
    while reader.has_remaining()? {
        let (name, InputSpan { offset, len }) = reader
            .read_any_header()
            .attach_printable("while parsing unknown chunks")?;
        match name {
            ALPH | ANIM | EXIF | ICCP | VP8 | VP8L | VP8X | XMP => {
                bail_attach!(ParseError::InvalidChunkLayout, MultipleChunks(name))
            }
            ANMF => bail_attach!(ParseError::InvalidChunkLayout, "non-contiguous ANMF chunk"),
            _ => ensure_attach!(config.allow_unknown_chunks, ParseError::UnsupportedChunk(name)),
        }
        reader.skip_data()?;
        log::info!("{name} @ 0x{offset:08x}: {len} bytes");
    }

    ensure_attach!(
        !file_reader.has_remaining()?,
        ParseError::InvalidInput,
        ExtraUnparsedInput,
    );

    Ok(())
}

fn sanitize_extended<R: Read + Skip>(
    reader: &mut ChunkReader<R>,
    vp8x: &Vp8xChunk,
    config: &Config,
) -> Result<(), Error> {
    if vp8x.flags.contains(Vp8xFlags::HAS_ICCP_CHUNK) {
        let InputSpan { offset, len } = reader.read_header(ICCP)?;
        reader.skip_data()?;
        log::info!("{name} @ 0x{offset:08x}: {len} bytes", name = ICCP);
    }

    if vp8x.flags.contains(Vp8xFlags::IS_ANIMATED) {
        sanitize_animated(reader, vp8x, config)?;
    } else {
        sanitize_still(reader, vp8x).attach_printable("while parsing still image data")?;
    }

    if vp8x.flags.contains(Vp8xFlags::HAS_EXIF_CHUNK) {
        let InputSpan { offset, len } = reader.read_header(EXIF)?;
        reader.skip_data()?;
        log::info!("{name} @ 0x{offset:08x}: {len} bytes", name = EXIF);
    }

    if vp8x.flags.contains(Vp8xFlags::HAS_XMP_CHUNK) {
        let InputSpan { offset, len } = reader.read_header(XMP)?;
        reader.skip_data()?;
        log::info!("{name} @ 0x{offset:08x}: {len} bytes", name = XMP);
    }

    Ok(())
}

fn sanitize_still<R: Read + Skip>(reader: &mut ChunkReader<R>, vp8x: &Vp8xChunk) -> Result<(), Error> {
    let mut alph = None;
    if vp8x.flags.contains(Vp8xFlags::HAS_ALPH_CHUNK) {
        let InputSpan { offset, len } = reader.read_header(ALPH)?;
        let read_alph @ AlphChunk { flags } = reader.parse_data()?;
        read_alph.sanitize_image_data(reader.data_reader(), vp8x)?;
        reader.skip_data()?;
        log::info!("{name} @ 0x{offset:08x}: {len} bytes, flags {flags:08b}", name = ALPH);
        alph = Some(read_alph);
    }

    ensure_attach!(reader.has_remaining()?, ParseError::MissingRequiredChunk(VP8));
    let (name, InputSpan { offset, len }) = reader.read_any_header()?;
    match name {
        VP8 => {
            reader.skip_data()?;
            log::info!("{name} @ 0x{offset:08x}: {len} bytes");
        }
        VP8L => {
            ensure_matches_attach!(alph, None, ParseError::InvalidChunkLayout, WhileParsingChunk(VP8L));

            let vp8l @ Vp8lChunk { .. } = reader.parse_data()?;
            let (width, height) = (vp8l.width(), vp8l.height());
            ensure_attach!(
                (width.into(), height.into()) == (vp8x.canvas_width(), vp8x.canvas_height()),
                ParseError::InvalidInput,
                FrameDimensionsMismatch(width, height, vp8x.canvas_width(), vp8x.canvas_height()),
                WhileParsingType::new::<Vp8lChunk>(),
            );
            vp8l.sanitize_image_data(reader.data_reader())?;
            reader.skip_data()?;
            log::info!("{name} @ 0x{offset:08x}: {len} bytes, {width}x{height}");
        }
        _ => bail_attach!(
            ParseError::InvalidChunkLayout,
            "expected image data",
            WhileParsingChunk(name),
        ),
    }
    Ok(())
}

fn sanitize_animated<R: Read + Skip>(
    reader: &mut ChunkReader<R>,
    vp8x: &Vp8xChunk,
    config: &Config,
) -> Result<(), Error> {
    let InputSpan { offset, len } = reader.read_header(ANIM)?;
    let AnimChunk { .. } = reader.parse_data()?;
    log::info!("{name} @ 0x{offset:08x}: {len} bytes", name = ANIM);

    ensure_matches_attach!(
        reader.peek_header()?,
        Some(ANMF),
        ParseError::MissingRequiredChunk(ANMF),
    );

    while let Some(ANMF) = reader.peek_header()? {
        let InputSpan { offset, len } = reader.read_header(ANMF)?;
        let anmf @ AnmfChunk { flags, .. } = reader.parse_data()?;
        let (x, y, width, height) = (anmf.x(), anmf.y(), anmf.width(), anmf.height());
        log::info!(
            "{name} @ 0x{offset:08x}: {len} bytes, {width}x{height} @ ({x}, {y}), flags {flags:08b}",
            name = ANMF
        );

        let mut anmf_reader = reader.child_reader();

        let mut alph = None;
        if vp8x.flags.contains(Vp8xFlags::HAS_ALPH_CHUNK) {
            if let Some(ALPH) = anmf_reader.peek_header()? {
                let InputSpan { offset, len } = anmf_reader.read_header(ALPH)?;
                let read_alph @ AlphChunk { flags } = anmf_reader.parse_data()?;
                read_alph.sanitize_image_data(anmf_reader.data_reader(), vp8x)?;
                anmf_reader.skip_data()?;
                log::info!("{name} @ 0x{offset:08x}: {len} bytes, flags {flags:08b}", name = ALPH);
                alph = Some(read_alph);
            }
        }

        let (name, InputSpan { offset, len }) = anmf_reader
            .read_any_header()
            .attach_printable("while parsing animated image frame")?;
        match name {
            VP8 => {
                anmf_reader.skip_data()?;
                log::info!("{name} @ 0x{offset:08x}: {len} bytes");
            }
            VP8L => {
                ensure_matches_attach!(alph, None, ParseError::InvalidChunkLayout, WhileParsingChunk(VP8L));

                let vp8l @ Vp8lChunk { .. } = anmf_reader.parse_data()?;
                let (width, height) = (vp8l.width(), vp8l.height());
                ensure_attach!(
                    (vp8l.width().into(), vp8l.height().into()) == (vp8x.canvas_width(), vp8x.canvas_height()),
                    ParseError::InvalidInput,
                    FrameDimensionsMismatch(vp8l.width(), vp8l.height(), vp8x.canvas_width(), vp8x.canvas_height()),
                    WhileParsingType::new::<Vp8lChunk>(),
                );
                vp8l.sanitize_image_data(anmf_reader.data_reader())?;
                anmf_reader.skip_data()?;
                log::info!("{name} @ 0x{offset:08x}: {len} bytes, {width}x{height}");
            }
            _ => bail_attach!(
                ParseError::InvalidChunkLayout,
                "expected image data",
                WhileParsingChunk(name),
            ),
        }

        while anmf_reader.has_remaining()? {
            let (name, InputSpan { offset, len }) = anmf_reader
                .read_any_header()
                .attach_printable("while parsing unknown chunks")?;
            match name {
                ALPH | ANMF | ANIM | EXIF | ICCP | VP8 | VP8L | VP8X | XMP => bail_attach!(
                    ParseError::InvalidChunkLayout,
                    MultipleChunks(name),
                    WhileParsingChunk(ANMF),
                ),
                _ => ensure_attach!(
                    config.allow_unknown_chunks,
                    ParseError::UnsupportedChunk(name),
                    WhileParsingChunk(ANMF)
                ),
            }
            anmf_reader.skip_data()?;
            log::info!("{name} @ 0x{offset:08x}: {len} bytes");
        }
    }
    Ok(())
}

//
// Config impls
//

impl Config {
    /// Construct a builder for `Config`.
    ///
    /// See the documentation for [`ConfigBuilder`].
    pub fn builder() -> ConfigBuilder {
        ConfigBuilder::default()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::builder().build()
    }
}

//
// ConfigBuilder impls
//

impl ConfigBuilder {
    /// Build a new [`Config`].
    pub fn build(&self) -> Config {
        self.try_build().unwrap()
    }
}

#[cfg(doctest)]
#[doc = include_str!("../README.md")]
pub mod readme {}

#[cfg(test)]
mod test {
    use super::*;

    use assert_matches::assert_matches;
    use mediasan_common::parse::FourCC;

    use crate::parse::AlphFlags;
    use crate::util::test::{test_alph, test_anmf, test_header, test_vp8x, test_webp};

    const TEST: FourCC = FourCC { value: *b"TeSt" };

    #[test]
    pub fn not_riff() {
        let header = test_header().chunk_type(TEST).clone();
        let test = test_webp().header(Some(header)).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn not_webp() {
        let header = test_header().name(TEST).clone();
        let test = test_webp().header(Some(header)).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidInput, "{err:?}");
        });
    }

    #[test]
    pub fn file_len_zero() {
        let header = test_header().len(Some(0)).clone();
        let test = test_webp().header(Some(header)).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::TruncatedChunk, "{err:?}");
        });
    }

    #[test]
    pub fn file_len_one() {
        let header = test_header().len(Some(1)).clone();
        let test = test_webp().header(Some(header)).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::TruncatedChunk, "{err:?}");
        });
    }

    #[test]
    pub fn file_len_invalid() {
        let header = test_header().len(Some(MAX_FILE_LEN + 1)).clone();
        let test = test_webp().header(Some(header)).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidInput, "{err:?}");
        });
    }

    #[test]
    pub fn file_extra_data() {
        let mut test = test_webp().build();
        test.sanitize_ok();
        test.data = [&test.data[..], b"extra data"].concat().into();
        test.data_len = test.data.len() as u64;
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidInput, "{err:?}");
        });
    }

    #[test]
    pub fn image_data_missing() {
        let test = test_webp().chunks([]).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn lossy() {
        test_webp().chunks([VP8]).build().sanitize_ok();
    }

    #[test]
    pub fn lossless() {
        test_webp().chunks([VP8L]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_lossy() {
        test_webp().chunks([VP8X, VP8]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_lossy_alpha_lossless() {
        let alph = test_alph().flags(AlphFlags::COMPRESS_LOSSLESS).clone();
        test_webp().chunks([VP8X, ALPH, VP8]).alph(alph).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_lossy_alpha_uncompressed() {
        let alph = test_alph().flags(AlphFlags::empty()).clone();
        test_webp().chunks([VP8X, ALPH, VP8]).alph(alph).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_lossless() {
        test_webp().chunks([VP8X, VP8L]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_lossless_alpha() {
        let test = test_webp().chunks([VP8X, ALPH, VP8L]).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_animated() {
        test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_empty() {
        let test = test_webp().chunks([VP8X, ANIM]).anmfs([]).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::MissingRequiredChunk(ANMF), "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_animated_lossless() {
        let anmf = test_anmf().chunks([VP8L]).clone();
        let anmfs = [anmf.clone(), anmf];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_lossy() {
        let anmf = test_anmf().chunks([VP8]).clone();
        let anmfs = [anmf.clone(), anmf];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_lossy_lossless() {
        let anmfs = [test_anmf().chunks([VP8]).clone(), test_anmf().chunks([VP8L]).clone()];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_lossless_alpha() {
        let anmf = test_anmf().chunks([ALPH, VP8L]).clone();
        let anmfs = [anmf.clone(), anmf];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_animated_lossy_alpha() {
        let anmf = test_anmf().chunks([ALPH, VP8]).clone();
        let anmfs = [anmf.clone(), anmf];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_alpha_no_alpha() {
        let anmf_no_alpha = test_anmf().chunks([VP8]).clone();
        let alpha_anmf = test_anmf().chunks([ALPH, VP8]).clone();
        let anmfs = [alpha_anmf, anmf_no_alpha];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_no_alpha_alpha() {
        let anmf_no_alpha = test_anmf().chunks([VP8]).clone();
        let alpha_anmf = test_anmf().chunks([ALPH, VP8]).clone();
        let anmfs = [anmf_no_alpha, alpha_anmf];
        let test = test_webp().chunks([VP8X, ANIM, ANMF, ANMF]).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_alph_missing() {
        let vp8x = test_vp8x()
            .flags(Some(Vp8xFlags::IS_ANIMATED | Vp8xFlags::HAS_ALPH_CHUNK))
            .clone();
        let anmfs = [test_anmf().chunks([VP8L]).clone()];
        let test = test_webp().chunks([VP8X, ANIM, ANMF]).vp8x(vp8x).anmfs(anmfs).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_animated_alph_wrong_order() {
        let anmfs = [test_anmf().chunks([VP8L, ALPH]).clone()];
        let test = test_webp().chunks([VP8X, ANIM, ANMF]).anmfs(anmfs).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_animated_unexpected_alph() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::IS_ANIMATED)).clone();
        let anmfs = [test_anmf().chunks([ALPH, VP8L]).clone()];
        let test = test_webp().chunks([VP8X, ANIM, ANMF]).vp8x(vp8x).anmfs(anmfs).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_image_data_missing() {
        let test = test_webp().chunks([VP8X]).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::MissingRequiredChunk(VP8), "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_unexpected_alph() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, ALPH, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_unexpected_anim() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, ANIM, ANMF]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_unexpected_anmf() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, ANMF]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_alph_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::HAS_ALPH_CHUNK)).clone();
        let test = test_webp().chunks([VP8X, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_anim_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::IS_ANIMATED)).clone();
        let test = test_webp().chunks([VP8X, ANMF]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_anim_anmf_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::IS_ANIMATED)).clone();
        let test = test_webp().chunks([VP8X]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_invalid(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::MissingRequiredChunk(ANIM), "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_iccp() {
        test_webp().chunks([VP8X, ICCP, VP8L]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_exif() {
        test_webp().chunks([VP8X, VP8L, EXIF]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_xmp() {
        test_webp().chunks([VP8X, VP8L, XMP]).build().sanitize_ok();
    }

    #[test]
    pub fn vp8x_all_meta() {
        let test = test_webp().chunks([VP8X, ICCP, VP8L, EXIF, XMP]).build();
        test.sanitize_ok();
    }

    #[test]
    pub fn vp8x_wrong_order() {
        let test = test_webp().chunks([VP8L, VP8X]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_alph_wrong_order() {
        let test = test_webp().chunks([VP8X, VP8L, ALPH]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_iccp_wrong_order() {
        let test = test_webp().chunks([VP8X, VP8L, ICCP]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_exif_wrong_order() {
        let test = test_webp().chunks([VP8X, EXIF, VP8L]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
        let test = test_webp().chunks([VP8X, VP8L, XMP, EXIF]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_xmp_wrong_order() {
        let test = test_webp().chunks([VP8X, XMP, VP8L]).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_iccp_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::HAS_ICCP_CHUNK)).clone();
        let test = test_webp().chunks([VP8X, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_exif_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::HAS_EXIF_CHUNK)).clone();
        let test = test_webp().chunks([VP8X, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::MissingRequiredChunk(EXIF), "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_xmp_missing() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::HAS_XMP_CHUNK)).clone();
        let test = test_webp().chunks([VP8X, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::MissingRequiredChunk(XMP), "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_iccp_unexpected() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, ICCP, VP8L]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_exif_unexpected() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, VP8L, EXIF]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn vp8x_xmp_unexpected() {
        let vp8x = test_vp8x().flags(Some(Vp8xFlags::empty())).clone();
        let test = test_webp().chunks([VP8X, VP8L, XMP]).vp8x(vp8x).build();
        assert_matches!(test.sanitize_non_compliant(), Error::Parse(err) => {
            assert_matches!(err.get_ref(), ParseError::InvalidChunkLayout, "{err:?}");
        });
    }

    #[test]
    pub fn lossless_max_image_data() {
        let data = b"\x2f\xff\xff\xff\x0f\x81\x88\x88\x18\x44\x44\xc4\xff\x45\x44\x04\x21\x22\x22\x22\x22\x02";
        test_webp().vp8l_data(&data[..]).build().sanitize_ok();
    }
}
