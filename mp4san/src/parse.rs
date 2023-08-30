//! Unstable API for parsing individual MP4 box types.

mod array;
mod co64;
pub mod derive;
pub mod error;
mod ftyp;
mod header;
mod integers;
mod mdia;
mod minf;
mod moov;
mod mp4box;
mod stbl;
mod stco;
mod trak;
mod value;

pub use array::{ArrayEntry, ArrayEntryMut, BoundedArray, UnboundedArray};
pub use co64::Co64Box;
pub use error::ParseError;
pub use ftyp::FtypBox;
pub use header::{box_type, fourcc, BoxHeader, BoxSize, BoxType, BoxUuid, ConstFullBoxHeader, FullBoxHeader};
pub use integers::Mp4Prim;
pub use mdia::{MdiaBox, MdiaChildren, MdiaChildrenRef, MdiaChildrenRefMut};
pub use minf::{MinfBox, MinfChildren, MinfChildrenRef, MinfChildrenRefMut};
pub use moov::{MoovBox, MoovChildren, MoovChildrenRef, MoovChildrenRefMut};
pub use mp4box::{AnyMp4Box, BoxData, Boxes, Mp4Box, ParseBox, ParseBoxes, ParsedBox};
pub use stbl::{StblBox, StblChildren, StblChildrenRef, StblChildrenRefMut, StblCo, StblCoRef, StblCoRefMut};
pub use stco::StcoBox;
pub use trak::{TrakBox, TrakChildren, TrakChildrenRef, TrakChildrenRefMut};
pub use value::{Mp4Value, Mp4ValueReaderExt, Mp4ValueWriterExt};

pub use mediasan_common::parse::FourCC;
pub use mp4san_derive::{ParseBox, ParseBoxes, ParsedBox};

#[cfg(test)]
mod test {
    use bytes::BytesMut;

    use super::*;

    #[derive(Clone, Debug, PartialEq, ParseBox, ParsedBox)]
    #[box_type = b"\xffX0\x00"]
    pub struct NotARealBox {
        pub bar_ax: u64,
        pub foo_by: u32,
    }

    #[derive(Clone, Debug, PartialEq, ParseBox, ParsedBox)]
    #[box_type = 4283969538] // 0xff583002
    pub struct AnotherFakeBox;

    #[derive(Clone, Debug, PartialEq, ParseBox, ParsedBox)]
    #[box_type = "c12fdd3f-1e93-464c-baee-7c4480628f58"]
    pub struct FakeUuidTypeBox;

    #[derive(Clone, Debug, PartialEq, ParseBox, ParsedBox)]
    #[box_type = "xa04"]
    pub struct Fifth;

    #[derive(Clone, Debug, ParseBox, ParsedBox)]
    #[box_type = "test"]
    pub struct ArrayBox {
        pub array_32: BoundedArray<u32, i32>,
        pub array_16: BoundedArray<u16, i16>,
        pub unbounded_array: UnboundedArray<u8>,
    }

    #[test]
    fn test_size_simple() {
        let not_a_real = NotARealBox { bar_ax: u64::MAX, foo_by: u32::MAX };
        assert_eq!(
            not_a_real.encoded_len(),
            (<u64 as Mp4Prim>::encoded_len() + <u32 as Mp4Prim>::encoded_len()) as u64
        );
    }

    #[test]
    fn test_size_exttype() {
        assert_eq!(FakeUuidTypeBox.encoded_len(), 0);
    }

    #[test]
    fn test_type_bytes() {
        assert_eq!(NotARealBox::NAME, BoxType::FourCC(FourCC { value: *b"\xffX0\x00" }));
    }

    #[test]
    fn test_type_compact_int_decimal() {
        assert_eq!(
            AnotherFakeBox::NAME,
            BoxType::FourCC(FourCC { value: 4283969538u32.to_be_bytes() })
        );
    }

    #[test]
    fn test_type_extended() {
        let expected = BoxType::Uuid(BoxUuid { value: 0xc12fdd3f_1e93_464c_baee_7c4480628f58u128.to_be_bytes() });
        assert_eq!(FakeUuidTypeBox::NAME, expected);
    }

    #[test]
    fn test_type_compact_str() {
        assert_eq!(Fifth::NAME, BoxType::FourCC(FourCC { value: *b"xa04" }));
    }

    #[test]
    fn put_buf_empty() {
        let mut buf = vec![];
        FakeUuidTypeBox.put_buf(&mut buf);
        assert_eq!(buf, b"");
    }

    #[test]
    fn put_buf() {
        let mut buf = vec![];
        NotARealBox { bar_ax: 0x0102030405060708, foo_by: 0x090a0b0c }.put_buf(&mut buf);
        assert_eq!(buf, b"\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c");
    }

    #[test]
    fn parse() {
        let mut data = BytesMut::from(&b"\0\0\0\x14\xffX0\x00\x01\x02\x03\x04\x05\x06\x07\x08\x09\x0a\x0b\x0c"[..]);
        let data_len = data.len();
        let header = BoxHeader::parse(&mut data).unwrap();
        assert_eq!(header.box_size(), Some(data_len as u64));
        assert_eq!(
            NotARealBox::parse(&mut data).unwrap(),
            NotARealBox { bar_ax: 0x0102030405060708, foo_by: 0x090a0b0c }
        );
    }
}
