use std::io;
use std::num::NonZeroUsize;

use assert_matches::assert_matches;
use bytes::{Buf, Bytes};
use derive_builder::Builder;
use mp4san_test::{ffmpeg_assert_eq, gpac_assert_eq};

use crate::parse::box_type::{FREE, FTYP, MDAT, MECO, META, MOOV, SKIP};
use crate::parse::BoxType;
use crate::{sanitize, sanitize_with_config, Config, InputSpan, SanitizedMetadata, Skip};

use super::{
    init_logger, sanitized_data, test_free, test_meco, test_meta, write_mdat_header, write_test_uuid, TestFtypBuilder,
    TestMoovBuilder, TEST_UUID,
};

#[derive(Builder)]
#[builder(name = "TestMp4Builder", build_fn(name = "build_spec"))]
pub struct TestMp4Spec {
    #[builder(default)]
    ftyp: TestFtypBuilder,

    #[builder(default)]
    moov: TestMoovBuilder,

    #[builder(default = "DEFAULT_MDAT_DATA.to_vec()")]
    #[builder(setter(into, each(name = "add_mdat_data", into)))]
    mdat_data: Vec<u8>,

    #[builder(
        default = "Some(self.mdat_data.as_deref().map(|mdat| mdat.len()).unwrap_or(DEFAULT_MDAT_DATA.len()) as u64)"
    )]
    #[builder(setter(strip_option))]
    mdat_data_len: Option<u64>,

    #[builder(default = "vec![FTYP, MDAT, MOOV]")]
    #[builder(setter(into, each(name = "add_box")))]
    boxes: Vec<BoxType>,
}

#[derive(Clone)]
pub struct TestMp4 {
    pub data: Bytes,
    pub data_len: u64,
    pub expected_metadata: Bytes,
    pub mdat_data: Vec<u8>,
    pub mdat: InputSpan,
    pub mdat_skipped: u64,
}

const DEFAULT_MDAT_DATA: &[u8] = &[0xBA, 0xDC, 0x0F, 0xFE, 0xBE, 0xEF];

impl TestMp4Builder {
    pub fn mdat_data_until_eof(&mut self) -> &mut Self {
        self.mdat_data_len = Some(None);
        self
    }

    pub fn build(&self) -> TestMp4 {
        self.build_spec().unwrap().build()
    }
}

impl TestMp4Spec {
    pub fn build(&self) -> TestMp4 {
        init_logger();

        let mut moov = self.moov();

        let mut data = vec![];
        let mut mdat: Option<InputSpan> = None;
        let mut mdat_header_len = None;
        let mut moov_offsets = Vec::new();
        for box_type in &self.boxes {
            match *box_type {
                FTYP => {
                    self.ftyp.build().put_buf(&mut data);
                }
                MOOV => {
                    moov_offsets.push(data.len());
                    moov.build().put_buf(&mut data);
                }
                MDAT => {
                    let written_mdat = write_mdat_header(&mut data, self.mdat_data_len);
                    mdat_header_len = Some(data.len() as u64 - written_mdat.offset);
                    data.extend_from_slice(&self.mdat_data);

                    let mdat_data_len = self.mdat_data_len.unwrap_or(self.mdat_data.len() as u64);
                    let mdat_len = written_mdat.len.saturating_add(mdat_data_len);
                    match &mut mdat {
                        Some(mdat) => mdat.len += mdat_len,
                        None => mdat = Some(InputSpan { len: mdat_len, ..written_mdat }),
                    }
                }
                name @ (FREE | META | MECO | SKIP) => {
                    let mp4_box = match name {
                        FREE | SKIP => test_free(name, 13),
                        META => test_meta(),
                        MECO => test_meco(),
                        _ => unreachable!(),
                    };
                    if let Some(mdat) = &mut mdat {
                        if data.len() as u64 == mdat.offset + mdat.len {
                            mdat.len += mp4_box.encoded_len();
                        }
                    };
                    mp4_box.put_buf(&mut data);
                }
                TEST_UUID => {
                    write_test_uuid(&mut data);
                }
                _ => panic!("invalid box type for test {box_type}"),
            }
        }

        let mdat = mdat.unwrap_or(InputSpan { offset: data.len() as u64, len: 0 });
        let mdat_header_len = mdat_header_len.unwrap_or(0);

        // Calculate and write correct chunk offsets
        let mut co_entries = moov.build_spec().unwrap().co_entries;
        for co_entry in &mut co_entries {
            *co_entry += mdat.offset + mdat_header_len;
        }
        for moov_offset in &moov_offsets {
            let moov = moov.co_entries(co_entries.clone()).build();
            moov.put_buf(&mut data[*moov_offset..]);
        }

        // Calculate expected output metadata. NB: The expectation that the output metadata matches the input
        // metadata verbatim is overly-strict and could be weakened.
        let mut expected_metadata = vec![];
        self.ftyp.build().put_buf(&mut expected_metadata);
        let mut expected_metadata_moov_offsets = Vec::new();
        for _ in moov_offsets {
            expected_metadata_moov_offsets.push(expected_metadata.len());
            let moov = moov.co_entries(co_entries.clone()).build();
            moov.put_buf(&mut expected_metadata);
        }

        // Calculate and write correct expected output chunk offsets
        for co_entry in &mut co_entries {
            *co_entry -= mdat.offset + mdat_header_len;
            *co_entry += expected_metadata.len() as u64 + mdat_header_len;
        }
        for expected_metadata_moov_offset in expected_metadata_moov_offsets {
            let moov = moov.co_entries(co_entries.clone()).build();
            moov.put_buf(&mut expected_metadata[expected_metadata_moov_offset..]);
        }

        TestMp4 {
            data_len: data.len() as u64,
            data: data.into(),
            expected_metadata: expected_metadata.into(),
            mdat_data: self.mdat_data.clone(),
            mdat,
            mdat_skipped: 0,
        }
    }

    pub fn moov(&self) -> TestMoovBuilder {
        let mut moov = self.moov.clone();
        for mdat_data_idx in 0..self.mdat_data.len() {
            moov.add_co_entry(mdat_data_idx as u64);
        }
        moov
    }
}

impl TestMp4 {
    pub fn sanitize_ok(&self) -> SanitizedMetadata {
        self.sanitize_ok_with_config(Config::default())
    }

    pub fn sanitize_ok_with_config(&self, config: Config) -> SanitizedMetadata {
        let sanitized = sanitize_with_config(self.clone(), config).unwrap();
        assert_eq!(sanitized.data, self.mdat);
        assert_matches!(sanitized.metadata.as_deref(), Some(metadata) => {
            assert_eq!(metadata, self.expected_metadata(metadata.len()));
        });
        let sanitized_data = sanitized_data(sanitized.clone(), &self.data);
        sanitize(io::Cursor::new(&sanitized_data)).unwrap();
        ffmpeg_assert_eq(&sanitized_data, &self.mdat_data);
        gpac_assert_eq(&sanitized_data, &self.mdat_data);
        sanitized
    }

    pub fn sanitize_ok_noop(&self) -> SanitizedMetadata {
        let sanitized = sanitize(self.clone()).unwrap();
        assert_eq!(sanitized.data, self.mdat);
        assert_eq!(sanitized.metadata, None);
        ffmpeg_assert_eq(&self.data, &self.mdat_data);
        gpac_assert_eq(&self.data, &self.mdat_data);
        sanitized
    }

    fn expected_metadata(&self, actual_len: usize) -> Bytes {
        let Some(pad_len) = self.expected_metadata.len().checked_sub(actual_len).and_then(NonZeroUsize::new) else {
            return self.expected_metadata.clone();
        };
        let mut expected_metadata = self.expected_metadata.to_vec();
        test_free(FREE, pad_len.get() as u32).put_buf(&mut expected_metadata);
        expected_metadata.into()
    }
}

impl io::Read for TestMp4 {
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
