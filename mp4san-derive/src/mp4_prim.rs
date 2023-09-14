use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use synstructure::Structure;

use crate::util::StructureExt;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let parse = input.parse(|_variant, field, _idx| {
        quote_spanned! {
            field.span() => Mp4Prim::parse(&mut buf).while_parsing_type()
        }
    });
    let mut variant_encoded_len = input.variants().iter().map(|variant| {
        variant.bindings().iter().fold(quote!(0), |acc, binding| {
            let ty = &binding.ast().ty;
            quote! { #acc + <#ty>::ENCODED_LEN }
        })
    });
    let first_variant_encoded_len = variant_encoded_len.next();
    let put_buf = input.each(|binding| quote! { buf.put_mp4_value(#binding); });

    let ident = &input.ast().ident;

    input.gen_impl(quote! {
        use std::prelude::v1::*;

        use bytes::{Buf, BufMut, BytesMut};
        use mp4san::Report;
        use mp4san::error::__ResultExt;
        use mp4san::parse::{Mp4Prim, Mp4ValueWriterExt, ParseError};
        use mp4san::parse::error::__ParseResultExt;

        #(if <#ident>::ENCODED_LEN != #variant_encoded_len {
            panic!(concat!(
                "error in #[derive(Mp4Prim)] for ",
                stringify!(#ident),
                ": all variants must have equal encoded length",
            ));
        })*

        #[automatically_derived]
        gen impl Mp4Prim for @Self {
            const ENCODED_LEN: u64 = #first_variant_encoded_len;

            fn parse<B: Buf + AsRef<[u8]>>(mut buf: B) -> Result<Self, Report<ParseError>> {
                if (buf.remaining() as u64) < Self::ENCODED_LEN {
                    return Err(Report::from(ParseError::TruncatedBox)).while_parsing_type();
                }

                #parse
            }

            fn put_buf<B: BufMut>(&self, mut buf: B) {
                match *self { #put_buf }
            }
        }
    })
}
