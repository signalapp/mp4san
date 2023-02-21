use std::fmt::Debug;
use std::mem::take;

use bytes::{Buf, BufMut, BytesMut};
use derive_where::derive_where;
use downcast_rs::{impl_downcast, Downcast};
use dyn_clonable::clonable;

use super::{BoxHeader, BoxType, ParseError};

#[derive(Debug)]
#[derive_where(Clone; BoxData<T>)]
pub struct Mp4Box<T: ?Sized> {
    pub header: BoxHeader,
    pub data: BoxData<T>,
}

pub type AnyMp4Box = Mp4Box<dyn ParsedBox>;

#[derive(Debug)]
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

#[derive(Clone, Debug, Default)]
pub struct Boxes {
    pub boxes: Vec<AnyMp4Box>,
}

impl<T: ParsedBox + ?Sized> Mp4Box<T> {
    pub fn with_data(data: T) -> Result<Self, ParseError>
    where
        T: ParseBox,
    {
        let header = BoxHeader::with_data_size(T::box_type(), data.encoded_len())?;
        Ok(Self { header, data: BoxData::Parsed(Box::new(data)) })
    }

    pub fn parse(mut buf: &mut BytesMut) -> Result<Self, ParseError> {
        let header = BoxHeader::parse(&mut buf)?;
        let data = BoxData::get_from_bytes_mut(buf, &header)?;
        Ok(Self { header, data })
    }

    pub fn parse_data_as<U: ParseBox + ParsedBox + Into<Box<T>>>(&mut self) -> Result<Option<&mut U>, ParseError> {
        if self.header.box_type() != U::box_type() {
            return Ok(None);
        }
        self.data.parse_as()
    }

    pub fn encoded_len(&self) -> u64 {
        self.header.encoded_len() + self.data.encoded_len() as u64
    }

    pub fn put_buf<B: BufMut>(&self, mut out: B) {
        self.header.put_buf(&mut out);
        self.data.put_buf(&mut out);
    }
}

impl<T: ParsedBox + ?Sized> BoxData<T> {
    pub fn get_from_bytes_mut(buf: &mut BytesMut, header: &BoxHeader) -> Result<Self, ParseError> {
        match header.box_data_size()? {
            None => Ok(Self::Bytes(take(buf))),
            Some(box_data_size) => match box_data_size.try_into() {
                Ok(box_data_size) if box_data_size > buf.len() => Err(ParseError::TruncatedBox),
                Ok(box_data_size) => Ok(Self::Bytes(buf.split_to(box_data_size))),
                Err(_) => Err(ParseError::InvalidInput("box too large")),
            },
        }
    }

    pub fn parse(&mut self) -> Result<&mut T, ParseError>
    where
        T: ParseBox + Sized,
    {
        if let BoxData::Bytes(data) = self {
            let parsed = T::parse(data)?;
            if !data.is_empty() {
                return Err(ParseError::InvalidInput("extra unparsed box data"));
            }
            *self = Self::Parsed(Box::new(parsed));
        }
        match self {
            BoxData::Parsed(parsed) => Ok(parsed),
            BoxData::Bytes(_) => unreachable!(),
        }
    }

    fn parse_as<U: ParseBox + ParsedBox + Into<Box<T>>>(&mut self) -> Result<Option<&mut U>, ParseError> {
        if let BoxData::Bytes(data) = self {
            let parsed = U::parse(data)?;
            if !data.is_empty() {
                return Err(ParseError::InvalidInput("extra unparsed box data"));
            }
            *self = Self::Parsed(parsed.into());
        }
        match self {
            BoxData::Parsed(parsed) => Ok(parsed.as_any_mut().downcast_mut()),
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
            boxes.push(Mp4Box::parse(buf)?);
        }
        Ok(Self { boxes })
    }

    pub fn box_types(&self) -> impl Iterator<Item = BoxType> + ExactSizeIterator + '_ {
        self.boxes.iter().map(|mp4box| mp4box.header.box_type())
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
        if self.box_types().filter(|box_type| *box_type == T::box_type()).count() > 1 {
            return Err(ParseError::InvalidBoxLayout("multiple boxes of same type"));
        }
        self.get_mut()
            .next()
            .ok_or_else(|| ParseError::MissingRequiredBox(T::box_type()))?
    }
}
