pub mod ftyp;
pub mod moov;
pub mod mp4;

use std::iter;

use bytes::{BufMut, BytesMut};

use crate::parse::box_type::{DINF, DREF, FREE, HDLR, MDAT, MDHD, METT, MVHD, STSC, STSD, STSZ, STTS, TKHD, URL};
use crate::parse::{AnyMp4Box, BoxHeader, BoxType, BoxUuid, FourCC, FullBoxHeader, Mp4Box};
use crate::{InputSpan, SanitizedMetadata};

pub const TEST_UUID: BoxType = BoxType::Uuid(BoxUuid(*b"thisisatestuuid!"));
pub const MP42: FourCC = FourCC { value: *b"mp42" };
pub const MP41: FourCC = FourCC { value: *b"mp41" };
pub const ISOM: FourCC = FourCC { value: *b"isom" };

pub use ftyp::TestFtypBuilder;
pub use moov::TestMoovBuilder;
pub use mp4::TestMp4Builder;

pub fn init_logger() {
    // Ignore errors initializing the logger if tests race to configure it
    let _ignore = env_logger::builder()
        .filter_level(log::LevelFilter::Info)
        .parse_default_env()
        .is_test(true)
        .try_init();
}

pub fn sanitized_data(sanitized: SanitizedMetadata, data: &[u8]) -> Vec<u8> {
    match sanitized.metadata {
        Some(metadata) => {
            let mdat = &data[sanitized.data.offset as usize..][..sanitized.data.len as usize];
            [&metadata[..], mdat].concat()
        }
        None => data.to_vec(),
    }
}

pub fn test_dinf() -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_dinf_data(&mut data);
    Mp4Box::with_bytes(DINF, data)
}

pub fn test_ftyp() -> TestFtypBuilder {
    Default::default()
}

pub fn test_hdlr(handler_type: FourCC) -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_hdlr_data(&mut data, handler_type);
    Mp4Box::with_bytes(HDLR, data)
}

pub fn test_mdhd() -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_mdhd_data(&mut data);
    Mp4Box::with_bytes(MDHD, data)
}

pub fn test_moov() -> TestMoovBuilder {
    Default::default()
}

pub fn test_mp4() -> TestMp4Builder {
    Default::default()
}

pub fn test_mvhd() -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_mvhd_data(&mut data);
    Mp4Box::with_bytes(MVHD, data)
}

pub fn test_stsc() -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_stsc_data(&mut data);
    Mp4Box::with_bytes(STSC, data)
}

pub fn test_stsd() -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_stsd_data(&mut data);
    Mp4Box::with_bytes(STSD, data)
}

pub fn test_stsz(chunk_count: u32) -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_stsz_data(&mut data, chunk_count);
    Mp4Box::with_bytes(STSZ, data)
}

pub fn test_stts(chunk_count: u32) -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_stts_data(&mut data, chunk_count);
    Mp4Box::with_bytes(STTS, data)
}

pub fn test_tkhd(track_id: u32) -> AnyMp4Box {
    let mut data = BytesMut::new();
    write_test_tkhd_data(&mut data, track_id);
    Mp4Box::with_bytes(TKHD, data)
}

pub fn write_hdlr_data<B: BufMut>(mut out: B, handler_type: FourCC) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(0); // pre-defined
    handler_type.put_buf(&mut out);
    for _ in 0..3 {
        out.put_u32(0); // reserved
    }
    out.put_u8(0); // name
}

pub fn write_mdat_header(out: &mut Vec<u8>, data_len: Option<u64>) -> InputSpan {
    let offset = out.len() as u64;
    let header = match data_len {
        Some(data_len) => BoxHeader::with_data_size(MDAT, data_len).unwrap(),
        None => BoxHeader::until_eof(MDAT),
    };
    header.put_buf(&mut *out);
    InputSpan { offset, len: out.len() as u64 - offset }
}

pub fn write_test_dinf_data<B: BufMut>(mut out: B) {
    BoxHeader::with_u32_data_size(DREF, 20).put_buf(&mut out); // dref header
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(1); // entry count
    BoxHeader::with_u32_data_size(URL, 4).put_buf(&mut out); // url header
    FullBoxHeader { version: 0, flags: 1 }.put_buf(&mut out);
}

pub fn write_test_free(mut out: &mut Vec<u8>, len: u32) {
    const FREE_HEADER_SIZE: u32 = BoxHeader::with_u32_data_size(FREE, 0).encoded_len() as u32;
    BoxHeader::with_u32_data_size(FREE, len - FREE_HEADER_SIZE).put_buf(&mut out);
    out.extend(iter::repeat(0).take((len - FREE_HEADER_SIZE) as usize));
}

pub fn write_test_mdat(out: &mut Vec<u8>, data: &[u8]) -> InputSpan {
    let mut span = write_mdat_header(out, Some(data.len() as u64));
    out.extend_from_slice(data);
    span.len += data.len() as u64;
    span
}

pub fn write_test_mdhd_data<B: BufMut>(mut out: B) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(0); // creation time
    out.put_u32(0); // modification time
    out.put_u32(1); // timescale
    out.put_u32(0); // duration
    out.put_u16(u16::from_be_bytes(*b"US")); // language
    out.put_u16(0); // pre-defined
}

pub fn write_test_mvhd_data<B: BufMut>(mut out: B) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(0); // creation time
    out.put_u32(0); // modification time
    out.put_u32(1); // timescale
    out.put_u32(0); // duration
    out.put_u32(0x00010000); // rate
    out.put_u16(0x0100); // volume
    out.put_u16(0); // reserved
    for _ in 0..2 {
        out.put_u32(0); // reserved
    }
    for value in [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
        out.put_u32(value); // matrix
    }
    for _ in 0..6 {
        out.put_u32(0); // pre-defined
    }
    out.put_u32(u32::MAX); // next track id
}

pub fn write_test_stsc_data<B: BufMut>(mut out: B) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(1); // entry count
    out.put_u32(1); // first chunk
    out.put_u32(1); // samples per chunk
    out.put_u32(1); // sample description index
}

pub fn write_test_stsd_data<B: BufMut>(mut out: B) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(1); // entry count
    BoxHeader::with_u32_data_size(METT, 9).put_buf(&mut out); // mett header
    for _ in 0..6 {
        out.put_u8(0); // reserved
    }
    out.put_u16(1); // data reference index
    out.put_u8(0); // mime format
}

pub fn write_test_stsz_data<B: BufMut>(mut out: B, chunk_count: u32) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(1); // sample size
    out.put_u32(chunk_count); // sample count
}

pub fn write_test_stts_data<B: BufMut>(mut out: B, chunk_count: u32) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(1); // entry count
    out.put_u32(chunk_count); // sample count
    out.put_u32(1); // sample delta
}

pub fn write_test_tkhd_data<B: BufMut>(mut out: B, track_id: u32) {
    FullBoxHeader::default().put_buf(&mut out);
    out.put_u32(0); // creation time
    out.put_u32(0); // modification time
    out.put_u32(track_id); // track id
    out.put_u32(0); // reserved
    out.put_u32(0); // duration
    for _ in 0..2 {
        out.put_u32(0); // reserved
    }
    out.put_u16(0); // layer
    out.put_u16(0); // alternate group
    out.put_u16(0); // volume
    out.put_u16(0); // reserved
    for value in [0x00010000, 0, 0, 0, 0x00010000, 0, 0, 0, 0x40000000] {
        out.put_u32(value); // matrix
    }
    out.put_u32(0); // width
    out.put_u32(0); // height
}

pub fn write_test_uuid(out: &mut Vec<u8>) {
    BoxHeader::with_u32_data_size(TEST_UUID, 0).put_buf(out);
}
