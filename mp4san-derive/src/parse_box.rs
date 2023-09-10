use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::Ident;
use synstructure::{Structure, VariantInfo};

use crate::attr::extract_box_type;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let box_type = extract_box_type(&input.ast().attrs);
    let parse_fn = derive_parse_fn(&input);

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

            #parse_fn
        }
    })
}

fn derive_parse_variant(ident: &Ident, variant: &VariantInfo<'_>) -> TokenStream {
    let parse_expr = variant.construct(|field, idx| {
        let field_ident = &field.ident;
        let parse_expr = quote_spanned! {
            field.span() =>
                Mp4Value::parse(&mut *buf)
                .while_parsing_field(<#ident>::NAME, stringify!(#field_ident))
        };
        match idx {
            0 => quote_spanned! {
                field.span() => match #parse_expr {
                    Ok(parsed) => parsed,
                    Err(err) => break 'parse_variant Err(err),
                }
            },
            _ => quote_spanned! { field.span() => #parse_expr? },
        }
    });

    let label = (!variant.bindings().is_empty()).then(|| quote! { 'parse_variant: });
    quote_spanned! {
        variant.ast().ident.span() => #label {
            let parsed = #parse_expr;

            if !buf.is_empty() {
                return Err(Report::from(ParseError::InvalidInput))
                    .attach_printable("extra unparsed data")
                    .while_parsing_box(<#ident>::NAME);
            }

            #[allow(clippy::needless_return)]
            return Ok(parsed);
        }
    }
}

fn derive_parse_fn(input: &Structure<'_>) -> TokenStream {
    let variants = input.variants().iter();
    let mut parse_variant = variants.map(|variant| derive_parse_variant(&input.ast().ident, variant));
    let last_parse_variant = parse_variant.next_back();
    quote! {
        fn parse(buf: &mut BytesMut) -> Result<Self, Report<ParseError>> {
            #(let _: Result<Self, Report<ParseError>> = #parse_variant;)*
            #last_parse_variant
        }
    }
}
