use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use synstructure::Structure;

use crate::attr::extract_box_type;
use crate::util::StructureExt;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let box_type = extract_box_type(&input.ast().attrs);

    let ident = &input.ast().ident;
    let parse = input.parse(|variant, field, idx| {
        let field_ident = &field.ident;
        let check_buf_empty = (idx == variant.bindings().len() - 1).then(|| {
            quote! {
                if !buf.is_empty() {
                    return Err(Report::from(ParseError::InvalidInput))
                        .attach_printable("extra unparsed data")
                        .while_parsing_box(<#ident>::NAME);
                }
            }
        });
        quote_spanned! {
            field.span() => {
                #[allow(clippy::let_and_return)]
                let parsed = Mp4Value::parse(&mut *buf).while_parsing_field(<#ident>::NAME, stringify!(#field_ident));
                if parsed.is_ok() {
                    #check_buf_empty
                }
                parsed
            }
        }
    });

    input.gen_impl(quote! {
        use std::prelude::v1::*;

        use bytes::BytesMut;
        use mp4san::Report;
        use mp4san::error::ResultExt;
        use mp4san::parse::{BoxType, Mp4Value, ParseBox, ParseError, FourCC};
        use mp4san::parse::error::ParseResultExt;

        #[automatically_derived]
        gen impl ParseBox for @Self {
            const NAME: BoxType = #box_type;

            fn parse(buf: &mut BytesMut) -> Result<Self, Report<ParseError>> {
                #parse
            }
        }
    })
}
