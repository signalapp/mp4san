use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::Ident;
use synstructure::{Structure, VariantInfo};

use crate::attr::extract_box_type;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let [variant] = input.variants() else {
        // This one _does_ need a semicolon though.
        return quote! {
            std::compile_error!("this trait can only be derived for structs or single-variant enums");
        };
    };
    let box_type = extract_box_type(&input.ast().attrs);
    let read_fn = derive_read_fn(&input.ast().ident, variant);

    input.gen_impl(quote! {
        use std::prelude::v1::*;

        use mp4san::Report;
        use mp4san::error::ResultExt;
        use mp4san::parse::{BoxType, Mp4Value, ParseBox, ParseError};
        use mp4san::parse::error::ParseResultExt;

        const NAME: BoxType = #box_type;

        #[automatically_derived]
        gen impl ParseBox for @Self {
            const NAME: BoxType = #box_type;

            #read_fn
        }
    })
}

fn derive_read_fn(ident: &Ident, variant: &VariantInfo<'_>) -> TokenStream {
    let parse_expr = variant.construct(|field, _idx| {
        let field_ident = &field.ident;
        quote_spanned! {
            field.span() =>
                Mp4Value::parse(&mut *buf)
                .while_parsing_field(<#ident>::NAME, stringify!(#field_ident))?
        }
    });

    quote! {
        fn parse(buf: &mut bytes::BytesMut) -> Result<Self, Report<ParseError>> {
            let parsed = #parse_expr;

            if !buf.is_empty() {
                return Err(Report::from(ParseError::InvalidInput))
                    .attach_printable("extra unparsed data")
                    .while_parsing_box(<#ident>::NAME);
            }

            Ok(parsed)
        }
    }
}
