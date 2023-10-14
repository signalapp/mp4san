use std::io;

use bytes::{Buf, BufMut, Bytes};
use derive_builder::Builder;
use mediasan_common::parse::FourCC;
use mediasan_common::Skip;
use mediasan_common_test::init_logger;
use webpsan_test::{libwebp_assert_invalid, libwebp_assert_valid};

use crate::parse::chunk_type::{ALPH, ANIM, ANMF, EXIF, ICCP, RIFF, VP8, VP8L, VP8X, XMP};
use crate::parse::{AlphFlags, Vp8xFlags, WebpChunk};
use crate::{sanitize_with_config, Config, Error};

use super::{
    write_test_alph, write_test_anim, write_test_anmf, write_test_chunk, write_test_exif, write_test_iccp,
    write_test_vp8x, write_test_xmp,
};

#[derive(Builder)]
#[builder(name = "TestWebpBuilder", build_fn(name = "build_spec"))]
pub struct TestWebpSpec {
    #[builder(default = "Some(Default::default())")]
    header: Option<TestFileHeaderSpecBuilder>,

    #[builder(default)]
    vp8x: TestVp8xSpecBuilder,

    #[builder(default)]
    alph: TestAlphSpecBuilder,

    #[builder(default, setter(into, each(name = "add_anmf")))]
    anmfs: Vec<TestAnmfSpecBuilder>,

    #[builder(default = "DEFAULT_VP8L_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_vp8l_data", into)))]
    vp8l_data: Vec<u8>,

    #[builder(default = "DEFAULT_VP8_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_vp8_data", into)))]
    vp8_data: Vec<u8>,

    #[builder(default = "vec![VP8L]")]
    #[builder(setter(into, each(name = "add_chunk")))]
    chunks: Vec<FourCC>,
}

#[derive(Clone)]
pub struct TestWebp {
    pub data: Bytes,
    pub data_len: u64,
}

#[derive(Builder)]
pub struct TestVp8xSpec {
    #[builder(default)]
    pub flags: Option<Vp8xFlags>,

    #[builder(default)]
    pub width: u32,

    #[builder(default)]
    pub height: u32,
}

#[derive(Builder)]
pub struct TestAlphSpec {
    #[builder(default = "AlphFlags::COMPRESS_LOSSLESS")]
    pub flags: AlphFlags,

    #[builder(default = "DEFAULT_ALPH_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_image_data", into)))]
    image_data: Vec<u8>,
}

#[derive(Builder)]
pub struct TestAnmfSpec {
    #[builder(default)]
    x: u32,

    #[builder(default)]
    y: u32,

    #[builder(default)]
    width: u32,

    #[builder(default)]
    height: u32,

    #[builder(default)]
    alph: TestAlphSpecBuilder,

    #[builder(default = "DEFAULT_VP8L_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_vp8l_data", into)))]
    vp8l_data: Vec<u8>,

    #[builder(default = "DEFAULT_VP8_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_vp8_data", into)))]
    vp8_data: Vec<u8>,

    #[builder(default = "vec![VP8L]")]
    #[builder(setter(into, each(name = "add_chunk")))]
    chunks: Vec<FourCC>,
}

#[derive(Builder)]
pub struct TestFileHeaderSpec {
    #[builder(default = "RIFF")]
    pub chunk_type: FourCC,

    #[builder(default)]
    pub len: Option<u32>,

    #[builder(default = "WebpChunk::WEBP")]
    pub name: FourCC,
}

const DEFAULT_VP8_DATA: &[u8] = &[
    18, 1, 0, 157, 1, 42, 1, 0, 1, 0, 18, 0, 52, 0, 0, 13, 192, 0, 254, 251, 253, 80, 0, 0,
];

const DEFAULT_VP8L_DATA: &[u8] = &[
    // image-header: signature image-size alpha-is-used version
    0x2f,
    0b0000_0000,
    0b0000_0000,
    0b0000_0000,
    0b0000_0000,
    // image-stream: optional-transform color-cache-info meta-prefix 5prefix-code
    0b1000_1000,
    0b1000_1000,
    0b0000_1000,
];

const DEFAULT_ALPH_DATA: &[u8] = &[
    // image-stream: optional-transform color-cache-info meta-prefix 5prefix-code
    0b1000_1000,
    0b1000_1000,
    0b0000_1000,
];

impl TestWebpBuilder {
    pub fn build(&self) -> TestWebp {
        self.build_spec().unwrap().build()
    }
}

impl TestWebpSpec {
    pub fn build(&self) -> TestWebp {
        init_logger();

        let mut data = vec![];

        let mut file_header = self.header.clone().map(|header| header.build().unwrap());
        if let Some(file_header) = &file_header {
            let len = file_header.len.unwrap_or(0xDEADBEEF);

            data.extend_from_slice(&file_header.chunk_type.value);
            data.put_u32_le(len as u32);
            data.extend_from_slice(&file_header.name.value);
        }

        let mut anmfs = self.anmfs.clone();

        for chunk_type in &self.chunks {
            match *chunk_type {
                VP8L => write_test_chunk(&mut data, &chunk_type.value, &self.vp8l_data),
                VP8 => write_test_chunk(&mut data, &chunk_type.value, &self.vp8_data),
                VP8X => {
                    let spec = self.vp8x.build().unwrap();
                    let flags = spec.flags.unwrap_or_else(|| {
                        let mut flags = Vp8xFlags::empty();
                        if self.chunks.contains(&ICCP) {
                            flags.insert(Vp8xFlags::HAS_ICCP_CHUNK);
                        }
                        if self.chunks.contains(&ANIM) {
                            flags.insert(Vp8xFlags::IS_ANIMATED);
                        }
                        let mut anmfs = self.anmfs.iter().map(|anmf| anmf.build().unwrap());
                        let any_anmf_alph =
                            self.chunks.contains(&ANMF) && anmfs.any(|anmf| anmf.chunks.contains(&ALPH));
                        if self.chunks.contains(&ALPH) || any_anmf_alph {
                            flags.insert(Vp8xFlags::HAS_ALPH_CHUNK);
                        }
                        if self.chunks.contains(&EXIF) {
                            flags.insert(Vp8xFlags::HAS_EXIF_CHUNK);
                        }
                        if self.chunks.contains(&XMP) {
                            flags.insert(Vp8xFlags::HAS_XMP_CHUNK);
                        }
                        flags
                    });
                    write_test_vp8x(&mut data, flags.bits(), spec.width, spec.height);
                }
                ALPH => {
                    let alph = self.alph.build().unwrap();
                    write_test_alph(&mut data, alph.flags.bits(), &alph.image_data)
                }
                ANIM => write_test_anim(&mut data),
                ANMF => {
                    let anmf = (!anmfs.is_empty()).then(|| anmfs.remove(0)).unwrap_or_default();
                    let TestAnmfSpec { x, y, width, height, alph, vp8l_data, vp8_data, chunks } = anmf.build().unwrap();
                    let alph = alph.build().unwrap();

                    let mut anmf_data = vec![];
                    for chunk_type in chunks {
                        match chunk_type {
                            ALPH => write_test_alph(&mut anmf_data, alph.flags.bits(), &alph.image_data),
                            VP8L => write_test_chunk(&mut anmf_data, &chunk_type.value, &vp8l_data),
                            VP8 => write_test_chunk(&mut anmf_data, &chunk_type.value, &vp8_data),
                            _ => panic!("invalid chunk type in ANMF for test {chunk_type}"),
                        }
                    }
                    write_test_anmf(&mut data, x, y, width, height, &anmf_data);
                }
                ICCP => write_test_iccp(&mut data),
                EXIF => write_test_exif(&mut data),
                XMP => write_test_xmp(&mut data),
                _ => panic!("invalid chunk type for test {chunk_type}"),
            }
        }

        if let Some(file_header) = &mut file_header {
            if file_header.len.is_none() {
                let len = data.len() as u32 - 8;
                (&mut data[4..]).put_u32_le(len);
            }
        }

        TestWebp { data_len: data.len() as u64, data: data.into() }
    }
}

impl TestWebp {
    /// Sanitize a spec-compliant file, asserting the sanitizer accepts it.
    pub fn sanitize_ok(&self) {
        self.sanitize_ok_with_config(Config::default())
    }

    /// Sanitize a spec-compliant file, with a [`Config`], asserting the sanitizer accepts it.
    pub fn sanitize_ok_with_config(&self, config: Config) {
        sanitize_with_config(self.clone(), config).unwrap();
        libwebp_assert_valid(&self.data)
    }

    /// Sanitize an invalid file that no parser should accept, asserting the sanitizer rejects it.
    pub fn sanitize_invalid(&self) -> Error {
        self.sanitize_invalid_with_config(Config::default())
    }

    /// Sanitize an invalid file that no parser should accept, with a [`Config`], asserting the sanitizer rejects it.
    pub fn sanitize_invalid_with_config(&self, config: Config) -> Error {
        let err = sanitize_with_config(self.clone(), config).unwrap_err();
        log::info!("sanitizer rejected invalid file: {err}\n{err:?}");
        libwebp_assert_invalid(&self.data);
        err
    }

    /// Sanitize a spec-non-compliant file that some parsers may still accept, asserting the sanitizer rejects it.
    pub fn sanitize_non_compliant(&self) -> Error {
        self.sanitize_non_compliant_with_config(Config::default())
    }

    /// Sanitize a spec-non-compliant file that some parsers may still accept, with a [`Config`], asserting the
    /// sanitizer rejects it.
    pub fn sanitize_non_compliant_with_config(&self, config: Config) -> Error {
        let err = sanitize_with_config(self.clone(), config).unwrap_err();
        log::info!("sanitizer rejected non-compliant file: {err}\n{err:?}");
        err
    }
}

impl io::Read for TestWebp {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        (&mut self.data).reader().read(buf)
    }
}

impl Skip for TestWebp {
    fn skip(&mut self, amount: u64) -> io::Result<()> {
        let advance_amount = self.data.len().min(amount as usize);
        self.data.advance(advance_amount);
        Ok(())
    }

    fn stream_position(&mut self) -> io::Result<u64> {
        Ok(self.data_len - self.data.len() as u64)
    }

    fn stream_len(&mut self) -> io::Result<u64> {
        Ok(self.data_len)
    }
}
