#![allow(missing_docs)]

use std::ops::{Deref, DerefMut};
use std::str;

use bytes::{BufMut, BytesMut};

use crate::error::Result;

use super::{Mp4Value, ParseError};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Mp4String {
    data: BytesMut,
}

impl Deref for Mp4String {
    type Target = str;

    fn deref(&self) -> &Self::Target {
        str::from_utf8(&self.data).unwrap_or_else(|_| unreachable!())
    }
}

impl DerefMut for Mp4String {
    fn deref_mut(&mut self) -> &mut Self::Target {
        str::from_utf8_mut(&mut self.data).unwrap_or_else(|_| unreachable!())
    }
}

impl Mp4Value for Mp4String {
    fn parse(buf: &mut BytesMut) -> Result<Self, ParseError> {
        let mut data = buf.split();
        ensure_attach!(
            data.last() == Some(&0),
            ParseError::InvalidInput,
            "string not null-terminated"
        );
        let _ = data.split_off(data.len() - 1);
        if let Err(err) = str::from_utf8(&data) {
            bail_attach!(ParseError::InvalidInput, err);
        }
        Ok(Self { data })
    }

    fn encoded_len(&self) -> u64 {
        self.data.len() as u64 + 1
    }

    fn put_buf<B: BufMut>(&self, mut buf: B) {
        buf.put(&self.data[..]);
        buf.put_u8(0);
    }
}
