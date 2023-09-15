use std::io;

use bytes::{Buf, BufMut, Bytes};
use derive_builder::Builder;
use mediasan_common::parse::FourCC;
use mediasan_common::Skip;
use mediasan_common_test::init_logger;

use crate::parse::chunk_type::{ALPH, ANIM, EXIF, ICCP, RIFF, VP8, VP8L, VP8X, XMP};
use crate::parse::{Vp8xFlags, WebpChunk};
use crate::{sanitize_with_config, Config};

use super::{write_test_chunk, write_test_exif, write_test_iccp, write_test_vp8x, write_test_xmp};

#[derive(Builder)]
#[builder(name = "TestWebpBuilder", build_fn(name = "build_spec"))]
pub struct TestWebpSpec {
    #[builder(default = "Some(Default::default())")]
    header: Option<TestFileHeaderSpecBuilder>,

    #[builder(default)]
    vp8x: TestVp8xSpecBuilder,

    #[builder(default = "DEFAULT_IMAGE_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_image_data", into)))]
    image_data: Vec<u8>,

    #[builder(default = "vec![VP8]")]
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
pub struct TestFileHeaderSpec {
    #[builder(default = "RIFF")]
    pub chunk_type: FourCC,

    #[builder(default)]
    pub len: Option<u32>,

    #[builder(default = "WebpChunk::WEBP")]
    pub name: FourCC,
}

const DEFAULT_IMAGE_DATA: &[u8] = &[
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

        for chunk_type in &self.chunks {
            match *chunk_type {
                VP8 | VP8L => {
                    write_test_chunk(&mut data, &chunk_type.value, &self.image_data);
                }
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
                        if self.chunks.contains(&ALPH) {
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
    pub fn sanitize_ok(&self) {
        self.sanitize_ok_with_config(Config::default())
    }

    pub fn sanitize_ok_with_config(&self, config: Config) {
        sanitize_with_config(self.clone(), config).unwrap();
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
