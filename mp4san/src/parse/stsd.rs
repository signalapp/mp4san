#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use super::{BoundedBoxes, ConstFullBoxHeader, ParseBox, ParsedBox};

#[derive(Clone, Debug, Default, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "stsd"]
pub struct StsdBox {
    pub _parsed_header: ConstFullBoxHeader,
    #[deref]
    #[deref_mut]
    pub children: BoundedBoxes<u32>,
}
