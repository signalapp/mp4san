//! Unstable API for parsing individual MP4 box types.

#[macro_use]
mod macros;

mod array;
mod co64;
pub mod derive;
pub mod error;
mod ftyp;
mod hdlr;
mod header;
mod integers;
mod mdhd;
mod mdia;
mod minf;
mod moov;
mod mp4box;
mod mvhd;
mod stbl;
mod stco;
mod string;
mod tkhd;
mod trak;
mod value;

pub use array::{ArrayEntry, ArrayEntryMut, BoundedArray, UnboundedArray};
pub use co64::Co64Box;
pub use error::ParseError;
pub use ftyp::FtypBox;
pub use hdlr::HdlrBox;
pub use header::{box_type, fourcc, BoxFlags, BoxHeader, BoxSize, BoxType, BoxUuid, ConstFullBoxHeader, FullBoxHeader};
pub use integers::{
    ConstI16, ConstI32, ConstI64, ConstI8, ConstU16, ConstU32, ConstU64, ConstU8, Mp4Prim, Mp4Transform,
};
pub use mdhd::MdhdBox;
pub use mdia::{MdiaBox, MdiaChildren, MdiaChildrenRef, MdiaChildrenRefMut};
pub use minf::{MinfBox, MinfChildren, MinfChildrenRef, MinfChildrenRefMut};
pub use moov::{MoovBox, MoovChildren, MoovChildrenRef, MoovChildrenRefMut};
pub use mp4box::{AnyMp4Box, BoxData, Boxes, Mp4Box, ParseBox, ParseBoxes, ParsedBox};
pub use mvhd::MvhdBox;
pub use stbl::{StblBox, StblChildren, StblChildrenRef, StblChildrenRefMut, StblCo, StblCoRef, StblCoRefMut};
pub use stco::StcoBox;
pub use string::Mp4String;
pub use tkhd::TkhdBox;
pub use trak::{TrakBox, TrakChildren, TrakChildrenRef, TrakChildrenRefMut};
pub use value::{Mp4Value, Mp4ValueReaderExt, Mp4ValueWriterExt};

pub use mediasan_common::parse::FourCC;
pub use mp4san_derive::{ParseBox, ParseBoxes, ParsedBox};

#[cfg(test)]
mod test {
    use std::fmt::Debug;

    use assert_matches::assert_matches;
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

    #[derive(Clone, Debug, ParseBox, ParsedBox)]
    #[box_type = "vers"]
    pub enum VersionedBox {
        V0 {
            _parsed_header: ConstFullBoxHeader,
            foo: u32,
        },
        V1(ConstFullBoxHeader<1, 101>, u64),
        V2(ConstFullBoxHeader<2, 102>),
    }

    #[derive(Clone, Debug, ParseBox, ParsedBox)]
    #[box_type = "vers"]
    pub enum StrictVersionedBox {
        V0(ConstFullBoxHeader),
        V1 { parsed_header: ConstFullBoxHeader<1> },
    }

    fn parse_box<T: ParseBox>(bytes: impl AsRef<[u8]>) -> T {
        T::parse(&mut BytesMut::from(bytes.as_ref())).unwrap()
    }

    fn parse_box_err<T: ParseBox + Debug>(bytes: impl AsRef<[u8]>) -> ParseError {
        T::parse(&mut BytesMut::from(bytes.as_ref())).unwrap_err().into_inner()
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
    fn versioned() {
        assert_matches!(
            parse_box([0, 0, 0, 0, 0, 0, 0, 32]),
            VersionedBox::V0 { _parsed_header: ConstFullBoxHeader, foo: 32 }
        );
        assert_matches!(
            parse_box([1, 0, 0, 101, 0, 0, 0, 0, 0, 0, 0, 64]),
            VersionedBox::V1(ConstFullBoxHeader, 64)
        );
        assert_matches!(parse_box([2, 0, 0, 102]), VersionedBox::V2(ConstFullBoxHeader));
        assert_matches!(parse_box_err::<VersionedBox>([2, 0, 0, 0]), ParseError::InvalidInput);
        assert_matches!(
            parse_box_err::<VersionedBox>([3, 0, 0, 103, 104]),
            ParseError::InvalidInput
        );
    }

    #[test]
    fn versioned_truncated() {
        assert_matches!(parse_box_err::<VersionedBox>([]), ParseError::TruncatedBox);
        assert_matches!(parse_box_err::<VersionedBox>([0, 0, 0, 0]), ParseError::TruncatedBox);
        assert_matches!(
            parse_box_err::<VersionedBox>([1, 0, 0, 101, 0, 0, 0, 0, 0, 0, 0]),
            ParseError::TruncatedBox
        );
    }

    #[test]
    fn versioned_extra() {
        assert_matches!(parse_box_err::<VersionedBox>([0; 9]), ParseError::InvalidInput);
        assert_matches!(
            parse_box_err::<VersionedBox>([1, 0, 0, 101, 0, 0, 0, 0, 0, 0, 0, 0, 0]),
            ParseError::InvalidInput
        );
        assert_matches!(
            parse_box_err::<VersionedBox>([2, 0, 0, 102, 1]),
            ParseError::InvalidInput
        );
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
