//! Unstable API for parsing individual MP4 box types.

pub(self) mod co;
mod co64;
pub(crate) mod error;
mod fourcc;
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

pub use co64::{Co64Box, Co64Entry};
pub use error::ParseError;
pub use fourcc::FourCC;
pub use ftyp::FtypBox;
pub use header::{box_type, BoxHeader, BoxSize, BoxType, BoxUuid, FullBoxHeader};
pub use integers::{Mpeg4Int, Mpeg4IntReaderExt, Mpeg4IntWriterExt};
pub use mdia::MdiaBox;
pub use minf::MinfBox;
pub use moov::MoovBox;
pub use mp4box::{AnyMp4Box, BoxData, Boxes, Mp4Box, ParseBox, ParsedBox};
pub use stbl::{StblBox, StblCoMut};
pub use stco::{StcoBox, StcoEntry};
pub use trak::TrakBox;
