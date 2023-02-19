use std::io::{self, Read};
use std::mem::size_of;

use bytes::BufMut;
use mp4::FourCC;

#[cfg(test)]
use crate::Error;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BoxHeader {
    box_type: BoxType,
    box_size: BoxSize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoxSize {
    UntilEof,
    Size(u32),
    Ext(u64),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BoxType {
    FourCC(mp4::BoxType),
    Uuid([u8; 16]),
}

const UUID: FourCC = FourCC { value: *b"uuid" };

impl BoxHeader {
    pub const MAX_SIZE: u64 = 32;

    pub const fn with_u32_data_size(box_type: mp4::BoxType, data_size: u32) -> Self {
        let box_type = BoxType::FourCC(box_type);
        let header_len = Self { box_type, box_size: BoxSize::Size(0) }.encoded_len() as u32;
        if let Some(box_size) = data_size.checked_add(header_len) {
            return Self { box_type, box_size: BoxSize::Size(box_size) };
        }

        let header_len = Self { box_type, box_size: BoxSize::Ext(0) }.encoded_len();
        Self { box_type, box_size: BoxSize::Ext(data_size as u64 + header_len) }
    }

    #[cfg(test)]
    pub const fn with_data_size(box_type: mp4::BoxType, data_size: u64) -> Result<Self, Error> {
        if data_size <= u32::MAX as u64 {
            return Ok(Self::with_u32_data_size(box_type, data_size as u32));
        }

        let box_type = BoxType::FourCC(box_type);
        let header_len = Self { box_type, box_size: BoxSize::Ext(0) }.encoded_len();
        let Some(box_size) = data_size.checked_add(header_len) else {
            return Err(Error::Parse(mp4::Error::InvalidData("box size too large")));
        };
        Ok(Self { box_type, box_size: BoxSize::Ext(box_size) })
    }

    #[cfg(test)]
    pub const fn until_eof(box_type: mp4::BoxType) -> Self {
        Self { box_type: BoxType::FourCC(box_type), box_size: BoxSize::UntilEof }
    }

    pub fn read<R: Read>(mut input: R) -> Result<Self, io::Error> {
        let mut size = [0; 4];
        input.read_exact(&mut size)?;

        let mut name = [0; 4];
        input.read_exact(&mut name)?;
        let name: mp4::BoxType = u32::from_be_bytes(name).into();

        let size = match u32::from_be_bytes(size) {
            0 => BoxSize::UntilEof,
            1 => {
                let mut size = [0; 8];
                input.read_exact(&mut size)?;
                BoxSize::Ext(u64::from_be_bytes(size))
            }
            size => BoxSize::Size(size),
        };

        let name = match FourCC::from(name) {
            UUID => {
                let mut uuid = [0; 16];
                input.read_exact(&mut uuid)?;
                BoxType::Uuid(uuid)
            }
            _ => BoxType::FourCC(name),
        };

        Ok(Self { box_type: name, box_size: size })
    }

    pub const fn encoded_len(&self) -> u64 {
        let mut size = (size_of::<u32>() + size_of::<u32>()) as u64;
        if let BoxSize::Ext(_) = self.box_size {
            size += size_of::<u64>() as u64;
        }
        if let BoxType::Uuid(_) = self.box_type {
            size += 16;
        }
        size
    }

    pub fn box_data_size(&self) -> Result<Option<u64>, mp4::Error> {
        match self.box_size.size() {
            None => Ok(None),
            Some(size) => size
                .checked_sub(self.encoded_len())
                .ok_or(mp4::Error::InvalidData("Invalid box size: too small"))
                .map(Some),
        }
    }

    pub const fn box_type(&self) -> mp4::BoxType {
        match self.box_type {
            BoxType::FourCC(fourcc) => fourcc,
            BoxType::Uuid(_) => mp4::BoxType::UnknownBox(u32::from_be_bytes(UUID.value)),
        }
    }

    pub fn write<B: BufMut>(&self, mut out: B) {
        match self.box_size {
            BoxSize::UntilEof => out.put_u32(0),
            BoxSize::Ext(_) => out.put_u32(1),
            BoxSize::Size(size) => out.put_u32(size),
        }

        match self.box_type {
            BoxType::FourCC(fourcc) => out.put_u32(fourcc.into()),
            BoxType::Uuid(_) => out.put_u32(UUID.into()),
        };

        if let BoxSize::Ext(size) = self.box_size {
            out.put_u64(size);
        }

        if let BoxType::Uuid(uuid) = self.box_type {
            out.put(&uuid[..]);
        }
    }
}

impl BoxSize {
    pub const fn size(&self) -> Option<u64> {
        match *self {
            BoxSize::UntilEof => None,
            BoxSize::Size(size) => Some(size as u64),
            BoxSize::Ext(size) => Some(size),
        }
    }
}
