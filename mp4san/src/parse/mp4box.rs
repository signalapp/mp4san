use std::fmt::Debug;
use std::mem::take;
use std::pin::Pin;
use std::result::Result as StdResult;

use bytes::{Buf, BufMut, BytesMut};
use derive_more::From;
use derive_where::derive_where;
use downcast_rs::{impl_downcast, Downcast};
use dyn_clonable::clonable;
use error_stack::Result;
use futures::io::BufReader;
use futures::{AsyncRead, AsyncReadExt};

use crate::util::IoResultExt;
use crate::{stream_len, stream_position, AsyncSkip, BoxDataTooLarge, Error, MAX_READ_BOX_SIZE};

use super::error::{MultipleBoxes, ParseResultExt, WhileParsingBox};
use super::{BoxHeader, BoxType, ParseError};

#[derive(Debug)]
#[derive_where(Clone; BoxData<T>)]
pub struct Mp4Box<T: ?Sized> {
    parsed_header: BoxHeader,
    pub data: BoxData<T>,
}

pub type AnyMp4Box = Mp4Box<dyn ParsedBox>;

#[derive(Debug, From)]
#[derive_where(Clone; Box<T>)]
pub enum BoxData<T: ?Sized> {
    Bytes(BytesMut),
    Parsed(Box<T>),
}

pub trait ParseBox: Sized {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError>;

    fn box_type() -> BoxType;
}

#[clonable]
pub trait ParsedBox: Clone + Debug + Downcast {
    fn encoded_len(&self) -> u64;

    fn put_buf(&self, out: &mut dyn BufMut);
}

#[derive(Clone, Debug, Default, From)]
pub struct Boxes {
    boxes: Vec<AnyMp4Box>,
}

impl<T: ParsedBox + ?Sized> Mp4Box<T> {
    pub fn with_data(data: BoxData<T>) -> Result<Self, ParseError>
    where
        T: ParseBox,
    {
        let parsed_header = BoxHeader::with_data_size(T::box_type(), data.encoded_len())?;
        Ok(Self { parsed_header, data })
    }

    pub fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let parsed_header = BoxHeader::parse(&mut buf).while_parsing_type::<Self>()?;
        let data = BoxData::get_from_bytes_mut(buf, &parsed_header).while_parsing_type::<Self>()?;
        Ok(Self { parsed_header, data })
    }

    /// Read and parse a box's data assuming its header has already been read.
    pub(crate) async fn read_data<R>(mut reader: Pin<&mut BufReader<R>>, header: BoxHeader) -> StdResult<Self, Error>
    where
        R: AsyncRead + AsyncSkip,
        T: ParseBox,
    {
        let box_data_size = match header.box_data_size()? {
            Some(box_data_size) => box_data_size,
            None => stream_len(reader.as_mut()).await? - stream_position(reader.as_mut()).await?,
        };

        ensure_attach!(
            box_data_size <= MAX_READ_BOX_SIZE,
            ParseError::InvalidInput,
            BoxDataTooLarge(box_data_size),
            WhileParsingBox(header.box_type()),
        );

        let mut buf = BytesMut::zeroed(box_data_size as usize);
        reader.read_exact(&mut buf).await.map_eof(|_| {
            Error::Parse(report_attach!(
                ParseError::TruncatedBox,
                WhileParsingBox(header.box_type())
            ))
        })?;
        Ok(Self { parsed_header: header, data: BoxData::Bytes(buf) })
    }

    pub fn calculated_header(&self) -> BoxHeader {
        let data_len = self.data.encoded_len();
        match self.parsed_header.box_data_size() {
            Ok(Some(parsed_header_data_len)) if parsed_header_data_len != data_len => {
                BoxHeader::with_data_size(self.parsed_header.box_type(), data_len)
                    .expect("parsed box data length cannot overflow a u64")
            }
            _ => self.parsed_header,
        }
    }

    pub fn parse_data_as<U: ParseBox + ParsedBox + Into<Box<T>>>(&mut self) -> Result<Option<&mut U>, ParseError> {
        if self.parsed_header.box_type() != U::box_type() {
            return Ok(None);
        }
        self.data.parse_as()
    }

    pub fn encoded_len(&self) -> u64 {
        self.calculated_header().encoded_len() + self.data.encoded_len()
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        self.calculated_header().put_buf(&mut out);
        self.data.put_buf(&mut out);
    }
}

impl AnyMp4Box {
    pub fn with_bytes(box_type: BoxType, bytes: BytesMut) -> Self {
        let parsed_header = BoxHeader::with_data_size(box_type, bytes.len() as u64).expect("box size overflow");
        Self { parsed_header, data: BoxData::Bytes(bytes) }
    }
}

impl<T: ParsedBox> From<Mp4Box<T>> for AnyMp4Box {
    fn from(from: Mp4Box<T>) -> Self {
        Self { parsed_header: from.parsed_header, data: from.data.into() }
    }
}

//
// BoxData impls
//

impl<T: ParsedBox + ?Sized> BoxData<T> {
    pub fn get_from_bytes_mut(buf: &mut BytesMut, header: &BoxHeader) -> Result<Self, ParseError> {
        match header.box_data_size()? {
            None => Ok(Self::Bytes(take(buf))),
            Some(box_data_size) => match box_data_size.try_into() {
                Ok(box_data_size) => {
                    ensure_attach!(
                        box_data_size <= buf.len(),
                        ParseError::TruncatedBox,
                        WhileParsingBox(header.box_type())
                    );
                    Ok(Self::Bytes(buf.split_to(box_data_size)))
                }
                Err(_) => Err(report_attach!(
                    ParseError::InvalidInput,
                    "box too large",
                    WhileParsingBox(header.box_type())
                )),
            },
        }
    }

    pub fn parse(&mut self) -> Result<&mut T, ParseError>
    where
        T: ParseBox + Sized,
    {
        if let BoxData::Bytes(data) = self {
            let parsed = T::parse(data).while_parsing_type::<BoxData<T>>()?;
            ensure_attach!(
                data.is_empty(),
                ParseError::InvalidInput,
                "extra unparsed data",
                WhileParsingBox(T::box_type()),
            );
            *self = Self::Parsed(Box::new(parsed));
        }
        match self {
            BoxData::Parsed(parsed) => Ok(parsed),
            BoxData::Bytes(_) => unreachable!(),
        }
    }

    fn parse_as<U: ParseBox + ParsedBox + Into<Box<T>>>(&mut self) -> Result<Option<&mut U>, ParseError> {
        if let BoxData::Bytes(data) = self {
            let parsed = U::parse(data).while_parsing_type::<BoxData<U>>()?;
            ensure_attach!(
                data.is_empty(),
                ParseError::InvalidInput,
                "extra unparsed data",
                WhileParsingBox(U::box_type()),
            );
            *self = Self::Parsed(parsed.into());
        }
        match self {
            BoxData::Parsed(parsed) => {
                // This is subtle because of the generic T; we have to make sure Downcast::as_any_mut is called on T and
                // not on Box<T>, or the subsequent Any::downcast_mut::<U>() will fail.
                Ok(<T>::as_any_mut(parsed).downcast_mut())
            }
            BoxData::Bytes(_) => unreachable!(),
        }
    }

    pub fn encoded_len(&self) -> u64 {
        match self {
            BoxData::Bytes(bytes) => bytes.len() as u64,
            BoxData::Parsed(parsed) => parsed.encoded_len(),
        }
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        match self {
            BoxData::Bytes(data) => out.put(&data[..]),
            BoxData::Parsed(parsed) => parsed.put_buf(&mut out),
        }
    }
}

impl<T: ParsedBox> From<BoxData<T>> for BoxData<dyn ParsedBox> {
    fn from(from: BoxData<T>) -> Self {
        match from {
            BoxData::Bytes(bytes) => BoxData::Bytes(bytes),
            BoxData::Parsed(parsed) => BoxData::Parsed(parsed),
        }
    }
}

impl<T: ParsedBox + Sized> From<T> for BoxData<T> {
    fn from(from: T) -> Self {
        BoxData::Parsed(Box::new(from))
    }
}

//
// ParsedBox impls
//

impl_downcast!(ParsedBox);

impl<'a, T: ParsedBox + 'a> From<T> for Box<dyn ParsedBox + 'a> {
    fn from(from: T) -> Self {
        Box::new(from)
    }
}

//
// Boxes impls
//

impl Boxes {
    pub fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let mut boxes = Vec::new();
        while buf.has_remaining() {
            boxes.push(Mp4Box::parse(buf).while_parsing_type::<Self>()?);
        }
        Ok(Self { boxes })
    }

    pub fn box_types(&self) -> impl Iterator<Item = BoxType> + ExactSizeIterator + '_ {
        self.boxes.iter().map(|mp4box| mp4box.parsed_header.box_type())
    }

    pub fn encoded_len(&self) -> u64 {
        self.boxes.iter().map(Mp4Box::encoded_len).sum()
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        for mp4box in &self.boxes {
            mp4box.put_buf(&mut out);
        }
    }

    pub fn get_mut<T: ParseBox + ParsedBox>(&mut self) -> impl Iterator<Item = Result<&mut T, ParseError>> {
        self.boxes
            .iter_mut()
            .flat_map(|mp4box| mp4box.parse_data_as().transpose())
    }

    pub fn get_one_mut<T: ParseBox + ParsedBox>(&mut self) -> Result<&mut T, ParseError> {
        ensure_attach!(
            self.box_types().filter(|box_type| *box_type == T::box_type()).count() <= 1,
            ParseError::InvalidBoxLayout,
            MultipleBoxes(T::box_type()),
        );
        self.get_mut()
            .next()
            .ok_or_else(|| ParseError::MissingRequiredBox(T::box_type()))?
    }
}
