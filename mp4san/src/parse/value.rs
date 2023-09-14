#![allow(missing_docs)]

use bytes::{BufMut, BytesMut};

use crate::error::Result;

use super::{Mp4Prim, ParseError};

pub trait Mp4Value: Sized {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError>;
    fn encoded_len(&self) -> u64;
    fn put_buf<B: BufMut>(&self, buf: B);

    fn to_bytes(&self) -> BytesMut {
        let mut buf = BytesMut::with_capacity(self.encoded_len() as usize);
        self.put_buf(&mut buf);
        buf
    }

    fn to_vec(&self) -> Vec<u8> {
        self.to_bytes().to_vec()
    }
}

pub trait Mp4ValueReaderExt {
    fn get_mp4_value<T: Mp4Value>(&mut self) -> Result<T, ParseError>;
}

pub trait Mp4ValueWriterExt: BufMut {
    fn put_mp4_value<T: Mp4Value>(&mut self, value: &T) {
        value.put_buf(self)
    }
}

impl<T: Mp4Prim> Mp4Value for T {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        Self::parse(buf)
    }
    fn encoded_len(&self) -> u64 {
        Self::ENCODED_LEN
    }
    fn put_buf<B: BufMut>(&self, buf: B) {
        self.put_buf(buf)
    }
}

impl Mp4ValueReaderExt for BytesMut {
    fn get_mp4_value<T: Mp4Value>(&mut self) -> Result<T, ParseError> {
        Mp4Value::parse(self)
    }
}

impl<B: BufMut> Mp4ValueWriterExt for B {}
