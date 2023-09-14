use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use synstructure::Structure;

use crate::util::StructureExt;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let parse = input.parse(|_variant, field, _idx| {
        quote_spanned! {
            field.span() => Mp4Value::parse(&mut *buf).while_parsing_type::<Self>()
        }
    });
    let encoded_len = input.fold(quote!(0), |acc, binding| quote! { #acc + #binding.encoded_len() });
    let put_buf = input.each(|binding| quote! { buf.put_mp4_value(#binding); });

    input.gen_impl(quote! {
        use std::prelude::v1::*;

        use bytes::{BufMut, BytesMut};
        use mp4san::Report;
        use mp4san::error::__ResultExt;
        use mp4san::parse::{Mp4Value, Mp4ValueWriterExt, ParseError};
        use mp4san::parse::error::__ParseResultExt;

        #[automatically_derived]
        gen impl Mp4Value for @Self {
            fn parse(buf: &mut BytesMut) -> Result<Self, Report<ParseError>> {
                #parse
            }

            fn encoded_len(&self) -> u64 {
                match *self { #encoded_len }
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                match *self { #put_buf }
            }
        }
    })
}
