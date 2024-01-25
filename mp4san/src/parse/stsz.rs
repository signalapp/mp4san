#![allow(missing_docs)]

use std::num::NonZeroU32;

use super::{BoundedArray, ConstFullBoxHeader, ConstU32, Mp4Value, ParseBox, ParseError, ParsedBox};
use crate::error::Result;

#[derive(Clone, Debug, Default, ParseBox, ParsedBox)]
#[box_type = "stsz"]
pub struct StszBox {
    _parsed_header: ConstFullBoxHeader,
    entries: StszEntries,
}

#[derive(Clone, Debug, Mp4Value)]
enum StszEntries {
    VariableSize {
        _not_fixed_size: ConstU32,
        entries: BoundedArray<u32, u32>,
    },
    FixedSize {
        size: NonZeroU32,
        number_of_samples: u32,
    },
}

impl Default for StszEntries {
    fn default() -> Self {
        Self::VariableSize { _not_fixed_size: Default::default(), entries: Default::default() }
    }
}

impl StszBox {
    pub fn sample_sizes(&self) -> impl ExactSizeIterator<Item = Result<u32, ParseError>> + '_ {
        let (mut variable_iter, fixed_size, count) = match &self.entries {
            StszEntries::FixedSize { size, number_of_samples } => (None, u32::from(*size), *number_of_samples),
            StszEntries::VariableSize { _not_fixed_size, entries } => {
                let variable_iter = entries.entries().map(|entry| entry.get());
                (Some(variable_iter), 0, entries.entry_count())
            }
        };

        // Handrolled "Either" here:
        (0..count).map(move |_| {
            variable_iter
                .as_mut()
                .map_or(Ok(fixed_size), |iter| iter.next().expect("matches count"))
        })
    }
}
