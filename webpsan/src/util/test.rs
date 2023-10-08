pub mod webp;

use bytes::BufMut;
use webp::{TestFileHeaderSpecBuilder, TestWebpBuilder};

use self::webp::{TestAlphSpecBuilder, TestAnmfSpecBuilder, TestVp8xSpecBuilder};

pub fn test_alph() -> TestAlphSpecBuilder {
    Default::default()
}

pub fn test_anmf() -> TestAnmfSpecBuilder {
    Default::default()
}

pub fn test_header() -> TestFileHeaderSpecBuilder {
    Default::default()
}

pub fn test_vp8x() -> TestVp8xSpecBuilder {
    Default::default()
}

pub fn test_webp() -> TestWebpBuilder {
    Default::default()
}

pub fn write_test_chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(name);
    out.put_u32_le(data.len() as u32);
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.put_u8(0);
    }
}

pub fn write_test_alph(out: &mut Vec<u8>, flags: u8, data: &[u8]) {
    out.extend_from_slice(b"ALPH");
    out.put_u32_le(1 + data.len() as u32);
    out.put_u8(flags);
    out.extend_from_slice(data);
    if (1 + data.len()) % 2 == 1 {
        out.put_u8(0);
    }
}

pub fn write_test_anim(out: &mut Vec<u8>) {
    out.extend_from_slice(b"ANIM");
    out.put_u32_le(6);
    out.put_u32_le(0xFACEC4FE);
    out.put_u16_le(0xF00F);
}

pub fn write_test_anmf(out: &mut Vec<u8>, x: u32, y: u32, width: u32, height: u32, data: &[u8]) {
    out.extend_from_slice(b"ANMF");
    out.put_u32_le(16 + data.len() as u32);
    out.put_uint_le(x.into(), 3);
    out.put_uint_le(y.into(), 3);
    out.put_uint_le(width.into(), 3);
    out.put_uint_le(height.into(), 3);
    out.put_uint_le(0xC0FFEE, 3);
    out.put_u8(0);
    out.extend_from_slice(data);
}

pub fn write_test_exif(out: &mut Vec<u8>) {
    write_test_chunk(out, b"EXIF", b"dummy EXIF data");
}

pub fn write_test_iccp(out: &mut Vec<u8>) {
    write_test_chunk(out, b"ICCP", b"dummy ICCP profile");
}

pub fn write_test_vp8x(out: &mut Vec<u8>, flags: u8, width: u32, height: u32) {
    out.extend_from_slice(b"VP8X");
    out.put_u32_le(10);
    out.push(flags);
    out.extend_from_slice(&[0; 3]);
    assert!(width < 2u32.pow(24));
    out.put_int_le(width as i64, 3);
    assert!(height < 2u32.pow(24));
    out.put_int_le(height as i64, 3);
}

pub fn write_test_xmp(out: &mut Vec<u8>) {
    write_test_chunk(out, b"XMP ", b"dummy XMP data");
}
