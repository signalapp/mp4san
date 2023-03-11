use std::io;

use bytes::{Buf, Bytes};
use derive_builder::Builder;

use crate::parse::box_type::{FREE, FTYP, MDAT, MOOV};
use crate::parse::BoxType;
use crate::{sanitize, InputSpan, SanitizedMetadata, Skip};

use super::{
    init_logger, sanitized_data, write_mdat_header, write_test_free, write_test_uuid, TestFtypBuilder, TestMoovBuilder,
    TEST_UUID,
};

#[derive(Builder)]
#[builder(name = "TestMp4Builder", build_fn(name = "build_spec"))]
pub struct TestMp4Spec {
    #[builder(default)]
    ftyp: TestFtypBuilder,

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
pub struct TestMp4 {
    pub data: Bytes,
    pub data_len: u64,
    pub expected_metadata: Bytes,
    pub mdat: InputSpan,
    pub mdat_skipped: u64,
}

impl TestMp4Builder {
    pub fn mdat_data_until_eof(&mut self) -> &mut Self {
        self.mdat_data_len = Some(None);
        self
    }

    pub fn build(&self) -> TestMp4 {
        init_logger();

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
                    spec.ftyp.build().put_buf(&mut data);
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
        spec.ftyp.build().put_buf(&mut expected_metadata);
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

impl TestMp4 {
    pub fn sanitize_ok(&self) -> SanitizedMetadata {
        let sanitized = sanitize(self.clone()).unwrap();
        assert_eq!(sanitized.data, self.mdat);
        assert_eq!(sanitized.metadata, self.expected_metadata);
        sanitize(io::Cursor::new(sanitized_data(sanitized.clone(), &self.data))).unwrap();
        sanitized
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
