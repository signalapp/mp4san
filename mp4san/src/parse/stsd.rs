#![allow(missing_docs)]

use derive_more::{Deref, DerefMut};

use super::{BoundedBoxes, ParseBox, ParsedBox};

#[derive(Clone, Debug, Default, Deref, DerefMut, ParseBox, ParsedBox)]
#[box_type = "stsd"]
pub struct StsdBox {
    pub children: BoundedBoxes<u32>,
}
