pub mod ftyp;
pub mod moov;
pub mod mp4;

use std::iter;

use crate::parse::box_type::{FREE, MDAT};
use crate::parse::{BoxHeader, BoxType, BoxUuid, FourCC};
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
    let mdat = &data[sanitized.data.offset as usize..][..sanitized.data.len as usize];
    [&sanitized.metadata[..], mdat].concat()
}

pub fn test_ftyp() -> TestFtypBuilder {
    Default::default()
}

pub fn test_moov() -> TestMoovBuilder {
    Default::default()
}

pub fn test_mp4() -> TestMp4Builder {
    Default::default()
}

pub fn write_test_mdat(out: &mut Vec<u8>, data: &[u8]) -> InputSpan {
    let mut span = write_mdat_header(out, Some(data.len() as u64));
    out.extend_from_slice(data);
    span.len += data.len() as u64;
    span
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

pub fn write_test_free(mut out: &mut Vec<u8>, len: u32) {
    const FREE_HEADER_SIZE: u32 = BoxHeader::with_u32_data_size(FREE, 0).encoded_len() as u32;
    BoxHeader::with_u32_data_size(FREE, len - FREE_HEADER_SIZE).put_buf(&mut out);
    out.extend(iter::repeat(0).take((len - FREE_HEADER_SIZE) as usize));
}

pub fn write_test_uuid(out: &mut Vec<u8>) {
    BoxHeader::with_u32_data_size(TEST_UUID, 0).put_buf(out);
}
