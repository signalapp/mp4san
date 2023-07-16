#![allow(missing_docs)]

use super::{FourCC, ParseBox, ParsedBox, UnboundedArray};

#[derive(Clone, Debug, ParseBox, ParsedBox)]
#[box_type = "ftyp"]
pub struct FtypBox {
    pub major_brand: FourCC,
    pub minor_version: u32,
    pub compatible_brands: UnboundedArray<FourCC>,
}

impl FtypBox {
    pub fn new(major_brand: FourCC, minor_version: u32, compatible_brands: impl IntoIterator<Item = FourCC>) -> Self {
        Self { major_brand, minor_version, compatible_brands: compatible_brands.into_iter().collect() }
    }

    pub fn compatible_brands(&self) -> impl Iterator<Item = FourCC> + ExactSizeIterator + '_ {
        self.compatible_brands.entries().map(|entry| entry.get().unwrap())
    }
}
