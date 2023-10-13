#![allow(missing_docs)]

use std::fmt::Debug;
use std::num::{NonZeroU32, NonZeroU8};

use bitstream_io::{Numeric, LE};
use derive_more::Display;
use futures_util::AsyncRead;
use mediasan_common::{ensure_attach, ensure_matches_attach};
use num_integer::div_ceil;
use num_traits::AsPrimitive;

use crate::{Error, ResultExt};

use super::bitstream::{BitBufReader, CanonicalHuffmanTree, LZ77_MAX_LEN};
use super::ParseError;

#[derive(Clone)]
pub struct LosslessImage {
    _image: SpatiallyCodedImage,
}

//
// private types
//

#[derive(Clone, Display, PartialEq, Eq)]
enum Transform {
    #[display(fmt = "predictor transform: block size {block_size}")]
    Predictor { block_size: u16, _image: EntropyCodedImage },
    #[display(fmt = "color transform: block size {block_size}")]
    Color { block_size: u16, _image: EntropyCodedImage },
    #[display(fmt = "subtract green transform")]
    SubtractGreen,
    #[display(fmt = "color indexing transform: {} colors", "image.width")]
    ColorIndexing { image: EntropyCodedImage },
}

#[repr(u8)]
#[derive(Clone, Copy, Debug, Display, PartialEq, Eq, PartialOrd, Ord)]
enum TransformType {
    #[display(fmt = "predictor")]
    Predictor = 0b00,
    #[display(fmt = "color")]
    Color = 0b01,
    #[display(fmt = "subtract green")]
    SubtractGreen = 0b10,
    #[display(fmt = "color indexing")]
    ColorIndexing = 0b11,
}

#[derive(Clone, PartialEq, Eq)]
struct EntropyCodedImage {
    width: NonZeroU32,
    height: NonZeroU32,
}

#[derive(Clone, PartialEq, Eq)]
struct SpatiallyCodedImage;

#[derive(Clone, Copy, Debug, Display, PartialEq, Eq)]
#[display(fmt = "distance {dist} length {len}")]
struct BackReference {
    dist: NonZeroU32,
    len: NonZeroU32,
}

#[derive(Clone, Copy, Debug, Default, Display, PartialEq, Eq)]
#[display(fmt = "({alpha}, {red}, {green}, {blue})")]
struct Color {
    alpha: u8,
    red: u8,
    green: u8,
    blue: u8,
}

#[derive(Clone)]
struct ColorCache {
    order: Option<NonZeroU8>,
}

#[derive(Clone, Display, PartialEq, Eq)]
enum MetaPrefixCodes {
    #[display(fmt = "single meta prefix code")]
    Single,
    #[display(fmt = "multiple meta prefix codes: max code group {max_code_group}, block size {block_size}")]
    Multiple {
        block_size: u16,
        max_code_group: u16,
        _image: EntropyCodedImage,
    },
}

trait PrefixCode {
    type Symbol: Numeric;

    fn new(tree: CanonicalHuffmanTree<LE, Self::Symbol>) -> Self;

    fn alphabet_size(color_cache_len: u16) -> u16;
}

struct PrefixCodeGroup {
    green: GreenPrefixCode,
    red: ARBPrefixCode,
    blue: ARBPrefixCode,
    alpha: ARBPrefixCode,
    distance: DistancePrefixCode,
}

struct CodeLengthPrefixCode {
    tree: CanonicalHuffmanTree<LE, u8>,
}

struct GreenPrefixCode {
    tree: CanonicalHuffmanTree<LE, u16>,
}

struct ARBPrefixCode {
    tree: CanonicalHuffmanTree<LE, u8>,
}

struct DistancePrefixCode {
    tree: CanonicalHuffmanTree<LE, u8>,
}

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "out-of-bounds color cache index `{_0}` >= `{_1}`")]
struct ColorCacheIndexOutOfBounds(u16, u16);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid back-reference distance `{_0}` at pixel `{_1}`")]
struct InvalidBackRefDistance(NonZeroU32, u32);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid back-reference length `{_0}` at pixel `{_1}` with image length `{_2}`")]
struct InvalidBackRefLength(NonZeroU32, u32, u32);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid code length repetition `{_0}` at `{_1}` with max symbols `{_2}`")]
struct InvalidCodeLengthRepetition(u8, usize, u16);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid color cache size `{_0}`")]
struct InvalidColorCacheSize(u8);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid duplicate {_0} transform")]
struct InvalidDuplicateTransform(TransformType);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid predictor `{_0}`")]
struct InvalidPredictor(u8);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "invalid symbol count `{_0}` >= `{_1}`")]
struct InvalidSymbolCount(u16, u16);

#[derive(Clone, Copy, Debug, Display)]
#[display(fmt = "while parsing {_0} transform")]
struct WhileParsingTransform(TransformType);

//
// LosslessImage impls
//

impl LosslessImage {
    pub async fn read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<Self, Error> {
        let mut transformed_width = width;
        let mut transforms = [false; TransformType::COUNT];
        while reader.read_bit().await? {
            let transform = Transform::read(reader, transformed_width, height)
                .await
                .while_parsing_type()?;

            transformed_width = transform.transformed_width(transformed_width);

            ensure_attach!(
                !transforms[transform.transform_type() as usize],
                ParseError::InvalidInput,
                InvalidDuplicateTransform(transform.transform_type()),
            );

            transforms[transform.transform_type() as usize] = true;
            log::info!("{transform}");
        }

        let _image = SpatiallyCodedImage::read(reader, transformed_width, height)
            .await
            .while_parsing_type()?;

        Ok(Self { _image })
    }
}

//
// Transform impls
//

impl Transform {
    async fn read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<Self, Error> {
        match TransformType::read(reader).await? {
            transform @ TransformType::Predictor => {
                let block_order = 2 + reader
                    .read::<u32>(3)
                    .await
                    .attach_printable(WhileParsingTransform(transform))?;
                let block_size = 2u16.pow(block_order);
                let width_in_blocks = len_in_blocks(width, block_size);
                let height_in_blocks = len_in_blocks(height, block_size);
                let _image = EntropyCodedImage::read(reader, width_in_blocks, height_in_blocks, |pixel| {
                    ensure_matches_attach!(
                        pixel.green,
                        0..=13,
                        ParseError::InvalidInput,
                        InvalidPredictor(pixel.green),
                    );
                    Ok(())
                })
                .await
                .while_parsing_type()
                .attach_printable(WhileParsingTransform(transform))?;
                Ok(Self::Predictor { block_size, _image })
            }
            transform @ TransformType::Color => {
                let block_order = 2 + reader
                    .read::<u32>(3)
                    .await
                    .attach_printable(WhileParsingTransform(transform))?;
                let block_size = 2u16.pow(block_order);
                let width_in_blocks = len_in_blocks(width, block_size);
                let height_in_blocks = len_in_blocks(height, block_size);
                let _image = EntropyCodedImage::read(reader, width_in_blocks, height_in_blocks, |_| Ok(()))
                    .await
                    .while_parsing_type()
                    .attach_printable(WhileParsingTransform(transform))?;
                Ok(Self::Color { block_size, _image })
            }
            TransformType::SubtractGreen => Ok(Self::SubtractGreen),
            transform @ TransformType::ColorIndexing => {
                let len_minus_one = reader
                    .read(8)
                    .await
                    .attach_printable(WhileParsingTransform(transform))?;
                let len = NonZeroU32::MIN.saturating_add(len_minus_one);
                let image = EntropyCodedImage::read(reader, len, NonZeroU32::MIN, |_| Ok(()))
                    .await
                    .while_parsing_type()
                    .attach_printable(WhileParsingTransform(transform))?;
                Ok(Self::ColorIndexing { image })
            }
        }
    }

    fn transform_type(&self) -> TransformType {
        match self {
            Transform::Predictor { .. } => TransformType::Predictor,
            Transform::Color { .. } => TransformType::Color,
            Transform::SubtractGreen => TransformType::SubtractGreen,
            Transform::ColorIndexing { .. } => TransformType::ColorIndexing,
        }
    }

    fn transformed_width(&self, width: NonZeroU32) -> NonZeroU32 {
        match self {
            Transform::ColorIndexing { image } => {
                let block_size = match image.width.get() {
                    0..=2 => 8,
                    3..=4 => 4,
                    5..=16 => 2,
                    17.. => 1,
                };
                len_in_blocks(width, block_size)
            }
            _ => width,
        }
    }
}

impl TransformType {
    const PREDICTOR: u8 = TransformType::Predictor as u8;
    const COLOR: u8 = TransformType::Color as u8;
    const SUBTRACT_GREEN: u8 = TransformType::SubtractGreen as u8;
    const COLOR_INDEXING: u8 = TransformType::ColorIndexing as u8;

    const COUNT: usize = 4;

    async fn read<R: AsyncRead + Unpin>(reader: &mut BitBufReader<R, LE>) -> Result<Self, Error> {
        match reader.read(2).await? {
            Self::PREDICTOR => Ok(Self::Predictor),
            Self::COLOR => Ok(Self::Color),
            Self::SUBTRACT_GREEN => Ok(Self::SubtractGreen),
            Self::COLOR_INDEXING => Ok(Self::ColorIndexing),
            0b100.. => unreachable!(),
        }
    }
}

//
// EntropyCodedImage impls
//

impl EntropyCodedImage {
    async fn read<R: AsyncRead + Unpin, F: FnMut(Color) -> Result<(), Error>>(
        reader: &mut BitBufReader<R, LE>,
        width: NonZeroU32,
        height: NonZeroU32,
        mut fun: F,
    ) -> Result<Self, Error> {
        let color_cache = ColorCache::read(reader).await.while_parsing_type()?;
        let codes = PrefixCodeGroup::read(reader, &color_cache).await.while_parsing_type()?;
        let readahead_bits = codes.readahead_bits();

        let len = width.saturating_mul(height);
        let mut pixel_idx = 0;
        while pixel_idx < len.get() {
            if reader.buf_bits() < u64::from(readahead_bits) {
                reader.fill_buf().await?;
            }
            match reader.buf_read_huffman(&codes.green.tree)? {
                symbol @ 0..=255 => {
                    let color = Color::buf_read(reader, symbol as u8, &codes).while_parsing_type()?;
                    log::debug!("color: {color}");
                    fun(color)?;
                    pixel_idx += 1;
                }
                symbol @ 256..=279 => {
                    let back_ref = BackReference::buf_read(reader, symbol - 256, &codes, width).while_parsing_type()?;
                    log::debug!("backref: {back_ref}");
                    ensure_matches_attach!(
                        pixel_idx.checked_sub(back_ref.dist.get()),
                        Some(_),
                        ParseError::InvalidInput,
                        InvalidBackRefDistance(back_ref.dist, pixel_idx),
                    );
                    ensure_attach!(
                        back_ref.len.get() <= len.get() - pixel_idx,
                        ParseError::InvalidInput,
                        InvalidBackRefLength(back_ref.len, pixel_idx, len.get()),
                    );
                    pixel_idx += back_ref.len.get();
                }
                symbol @ 280.. => {
                    let color_cache_index = symbol - 280;
                    log::debug!("cached: {color_cache_index}");
                    ensure_attach!(
                        color_cache_index < color_cache.len(),
                        ParseError::InvalidInput,
                        ColorCacheIndexOutOfBounds(color_cache_index, color_cache.len()),
                    );
                    pixel_idx += 1;
                }
            }
        }
        Ok(Self { width, height })
    }
}

//
// SpatiallyCodedImage impls
//

impl SpatiallyCodedImage {
    async fn read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<Self, Error> {
        let color_cache = ColorCache::read(reader).await.while_parsing_type()?;
        let meta = MetaPrefixCodes::read(reader, width, height)
            .await
            .while_parsing_type()?;
        log::info!("{meta}");

        for _ in 0..=meta.max_code_group() {
            let _codes = PrefixCodeGroup::read(reader, &color_cache).await.while_parsing_type()?;
        }
        Ok(Self)
    }
}

//
// BackReference impls
//

impl BackReference {
    #[rustfmt::skip]
    const DISTANCE_MAP: [(i8, u8); 120] = [
        (0, 1),  (1, 0),  (1, 1),  (-1, 1), (0, 2),  (2, 0),  (1, 2),
        (-1, 2), (2, 1),  (-2, 1), (2, 2),  (-2, 2), (0, 3),  (3, 0),
        (1, 3),  (-1, 3), (3, 1),  (-3, 1), (2, 3),  (-2, 3), (3, 2),
        (-3, 2), (0, 4),  (4, 0),  (1, 4),  (-1, 4), (4, 1),  (-4, 1),
        (3, 3),  (-3, 3), (2, 4),  (-2, 4), (4, 2),  (-4, 2), (0, 5),
        (3, 4),  (-3, 4), (4, 3),  (-4, 3), (5, 0),  (1, 5),  (-1, 5),
        (5, 1),  (-5, 1), (2, 5),  (-2, 5), (5, 2),  (-5, 2), (4, 4),
        (-4, 4), (3, 5),  (-3, 5), (5, 3),  (-5, 3), (0, 6),  (6, 0),
        (1, 6),  (-1, 6), (6, 1),  (-6, 1), (2, 6),  (-2, 6), (6, 2),
        (-6, 2), (4, 5),  (-4, 5), (5, 4),  (-5, 4), (3, 6),  (-3, 6),
        (6, 3),  (-6, 3), (0, 7),  (7, 0),  (1, 7),  (-1, 7), (5, 5),
        (-5, 5), (7, 1),  (-7, 1), (4, 6),  (-4, 6), (6, 4),  (-6, 4),
        (2, 7),  (-2, 7), (7, 2),  (-7, 2), (3, 7),  (-3, 7), (7, 3),
        (-7, 3), (5, 6),  (-5, 6), (6, 5),  (-6, 5), (8, 0),  (4, 7),
        (-4, 7), (7, 4),  (-7, 4), (8, 1),  (8, 2),  (6, 6),  (-6, 6),
        (8, 3),  (5, 7),  (-5, 7), (7, 5),  (-7, 5), (8, 4),  (6, 7),
        (-6, 7), (7, 6),  (-7, 6), (8, 5),  (7, 7),  (-7, 7), (8, 6),
        (8, 7)
    ];
    const DISTANCE_MAP_LEN: u32 = Self::DISTANCE_MAP.len() as u32;

    fn buf_read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        len_symbol: u16,
        codes: &PrefixCodeGroup,
        width: NonZeroU32,
    ) -> Result<Self, Error> {
        let len = reader.buf_read_lz77(len_symbol)?;
        let dist_symbol = reader.buf_read_huffman(&codes.distance.tree)?;
        let dist_code = reader.buf_read_lz77(dist_symbol.into())?;
        let dist = match dist_code.get() {
            0 => unreachable!(),
            dist_code @ 1..=Self::DISTANCE_MAP_LEN => {
                let (dx, dy) = Self::DISTANCE_MAP[dist_code as usize - 1];
                (u32::from(dy) * u32::from(width))
                    .checked_add_signed(dx.into())
                    .and_then(NonZeroU32::new)
                    .unwrap_or(NonZeroU32::MIN)
            }
            _ => NonZeroU32::new(dist_code.get() - Self::DISTANCE_MAP_LEN).unwrap_or_else(|| unreachable!()),
        };
        Ok(Self { dist, len })
    }

    fn readahead_bits(codes: &PrefixCodeGroup) -> u32 {
        2 * u32::from(LZ77_MAX_LEN) + codes.distance.tree.longest_code_len()
    }
}

//
// Color impls
//

impl Color {
    fn buf_read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        green: u8,
        codes: &PrefixCodeGroup,
    ) -> Result<Self, Error> {
        Ok(Self {
            green,
            red: reader.buf_read_huffman(&codes.red.tree)?,
            blue: reader.buf_read_huffman(&codes.blue.tree)?,
            alpha: reader.buf_read_huffman(&codes.alpha.tree)?,
        })
    }
}

impl From<Color> for u32 {
    fn from(color: Color) -> Self {
        (color.alpha as u32) << 24 | (color.red as u32) << 16 | (color.green as u32) << 8 | color.blue as u32
    }
}

//
// ColorCache impls
//

impl ColorCache {
    async fn read<R: AsyncRead + Unpin>(reader: &mut BitBufReader<R, LE>) -> Result<Self, Error> {
        let has_color_cache = reader.read_bit().await?;
        if has_color_cache {
            let order = reader.read::<u8>(4).await?;
            ensure_attach!(order <= 11, ParseError::InvalidInput, InvalidColorCacheSize(order));
            ensure_matches_attach!(
                NonZeroU8::new(order),
                Some(order),
                ParseError::InvalidInput,
                InvalidColorCacheSize(order),
            );
            Ok(Self { order: Some(order) })
        } else {
            Ok(Self { order: None })
        }
    }

    fn len(&self) -> u16 {
        self.order.map(|order| 2u16.pow(order.get().into())).unwrap_or_default()
    }
}

//
// MetaPrefixCodes impls
//

impl MetaPrefixCodes {
    async fn read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        width: NonZeroU32,
        height: NonZeroU32,
    ) -> Result<Self, Error> {
        let has_meta = reader.read_bit().await?;
        if has_meta {
            let block_order = 2 + reader.read::<u32>(3).await?;
            let block_size = 2u16.pow(block_order);
            let width_in_blocks = len_in_blocks(width, block_size);
            let height_in_blocks = len_in_blocks(height, block_size);
            let mut max_code_group = 0;
            let _image = EntropyCodedImage::read(reader, width_in_blocks, height_in_blocks, |color| {
                max_code_group = max_code_group.max(u16::from(color.red) << 8 | u16::from(color.green));
                Ok(())
            })
            .await
            .while_parsing_type()?;
            Ok(Self::Multiple { block_size, max_code_group, _image })
        } else {
            Ok(Self::Single)
        }
    }

    fn max_code_group(&self) -> u16 {
        match self {
            MetaPrefixCodes::Single => 0,
            &MetaPrefixCodes::Multiple { max_code_group, .. } => max_code_group,
        }
    }
}

//
// PrefixCodeGroup impls
//

impl PrefixCodeGroup {
    async fn read<R: AsyncRead + Unpin>(
        reader: &mut BitBufReader<R, LE>,
        color_cache: &ColorCache,
    ) -> Result<Self, Error> {
        let green = Self::read_prefix_code(reader, color_cache).await.while_parsing_type()?;
        let red = Self::read_prefix_code(reader, color_cache).await.while_parsing_type()?;
        let blue = Self::read_prefix_code(reader, color_cache).await.while_parsing_type()?;
        let alpha = Self::read_prefix_code(reader, color_cache).await.while_parsing_type()?;
        let distance = Self::read_prefix_code(reader, color_cache).await.while_parsing_type()?;
        Ok(Self { green, red, blue, alpha, distance })
    }

    async fn read_prefix_code<R: AsyncRead + Unpin, T: PrefixCode>(
        reader: &mut BitBufReader<R, LE>,
        color_cache: &ColorCache,
    ) -> Result<T, Error>
    where
        usize: AsPrimitive<T::Symbol>,
        T::Symbol: Copy + Ord + 'static,
    {
        let simple_code_length_code = reader.read_bit().await?;
        let tree = if simple_code_length_code {
            let has_second_symbol = reader.read_bit().await?;

            let is_first_symbol_8bits = reader.read_bit().await?;
            let first_symbol = if is_first_symbol_8bits {
                reader.read(8).await?
            } else {
                Numeric::from_u8(reader.read_bit().await? as u8)
            };
            let symbols = if has_second_symbol {
                let second_symbol = reader.read(8).await?;
                vec![(first_symbol, vec![0]), (second_symbol, vec![1])]
            } else {
                vec![(first_symbol, vec![])]
            };
            CanonicalHuffmanTree::from_symbols(symbols)?
        } else {
            let code_length_code = CodeLengthPrefixCode::read(reader).await?;

            let max_symbols = if reader.read_bit().await? {
                let length_bit_len = 2 + 2 * reader.read::<u32>(3).await?;
                log::debug!("length_bit_len: {length_bit_len:?}");
                2u16.saturating_add(reader.read(length_bit_len).await?)
            } else {
                T::alphabet_size(color_cache.len())
            };
            log::debug!("max_symbols: {max_symbols:?}");

            ensure_attach!(
                max_symbols <= T::alphabet_size(color_cache.len()),
                ParseError::InvalidInput,
                InvalidSymbolCount(max_symbols, T::alphabet_size(color_cache.len())),
            );

            let mut code_lengths = Vec::with_capacity(max_symbols as usize);
            let mut last_non_zero_code_length = NonZeroU8::new(8).unwrap_or_else(|| unreachable!());
            while code_lengths.len() < max_symbols as usize {
                let code_length_code = reader.read_huffman(&code_length_code.tree).await?;
                let (code_length, repeat_times) = match code_length_code {
                    0..=15 => (code_length_code, 1),
                    16 => (last_non_zero_code_length.get(), 3 + reader.read::<u8>(2).await?),
                    17 => (0, 3 + reader.read::<u8>(3).await?),
                    18 => (0, 11 + reader.read::<u8>(7).await?),
                    19.. => unreachable!(),
                };
                if let Some(non_zero_code_length) = NonZeroU8::new(code_length) {
                    last_non_zero_code_length = non_zero_code_length;
                }

                let new_code_lengths_len = code_lengths.len() + usize::from(repeat_times);
                ensure_attach!(
                    new_code_lengths_len <= usize::from(max_symbols),
                    ParseError::InvalidVp8lPrefixCode,
                    InvalidCodeLengthRepetition(repeat_times, code_lengths.len(), max_symbols)
                );
                let new_code_lengths =
                    (code_lengths.len()..new_code_lengths_len).map(|symbol| (symbol.as_(), code_length));
                code_lengths.extend(new_code_lengths);
            }

            log::debug!("code_lengths: {code_lengths:?}");
            CanonicalHuffmanTree::new(&mut code_lengths)?
        };
        Ok(T::new(tree))
    }

    pub fn argb_readahead_bits(&self) -> u32 {
        self.alpha.tree.longest_code_len()
            + self.red.tree.longest_code_len()
            + self.green.tree.longest_code_len()
            + self.blue.tree.longest_code_len()
    }

    pub fn readahead_bits(&self) -> u32 {
        self.argb_readahead_bits()
            .max(self.green.tree.longest_code_len() + BackReference::readahead_bits(self))
    }
}

//
// CodeLengthPrefixCode impls
//

impl CodeLengthPrefixCode {
    async fn read<R: AsyncRead + Unpin>(reader: &mut BitBufReader<R, LE>) -> Result<Self, Error> {
        const CODE_ORDER: [u8; 19] = [17, 18, 0, 1, 2, 3, 4, 5, 16, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

        let code_length_count = 4 + usize::from(reader.read::<u8>(4).await?);

        let mut code_lengths = [Default::default(); CODE_ORDER.len()];
        let mut code_order_iter = CODE_ORDER.iter();
        for &code_length_idx in code_order_iter.by_ref().take(code_length_count) {
            code_lengths[usize::from(code_length_idx)] = (code_length_idx, reader.read(3).await?);
        }
        for &code_length_idx in code_order_iter {
            code_lengths[usize::from(code_length_idx)] = (code_length_idx, 0);
        }

        let tree = CanonicalHuffmanTree::new(&mut code_lengths).attach_printable("while parsing code length code")?;
        Ok(Self { tree })
    }
}

//
// GreenPrefixCode impls
//

impl PrefixCode for GreenPrefixCode {
    type Symbol = u16;

    fn new(tree: CanonicalHuffmanTree<LE, Self::Symbol>) -> Self {
        Self { tree }
    }

    fn alphabet_size(color_cache_len: u16) -> u16 {
        256 + 24 + color_cache_len
    }
}

//
// ARBPrefixCode impls
//

impl PrefixCode for ARBPrefixCode {
    type Symbol = u8;

    fn new(tree: CanonicalHuffmanTree<LE, Self::Symbol>) -> Self {
        Self { tree }
    }

    fn alphabet_size(_color_cache_len: u16) -> u16 {
        256
    }
}

//
// DistancePrefixCode impls
//

impl PrefixCode for DistancePrefixCode {
    type Symbol = u8;

    fn new(tree: CanonicalHuffmanTree<LE, Self::Symbol>) -> Self {
        Self { tree }
    }

    fn alphabet_size(_color_cache_len: u16) -> u16 {
        40
    }
}

//
// private functions
//

fn len_in_blocks(len: NonZeroU32, block_size: u16) -> NonZeroU32 {
    NonZeroU32::new(div_ceil(len.get(), block_size.into())).unwrap_or_else(|| unreachable!())
}
