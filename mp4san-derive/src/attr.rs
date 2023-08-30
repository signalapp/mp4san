use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{Attribute, Expr, Lit, LitByteStr, Meta, MetaNameValue};
use uuid::Uuid;

pub(crate) fn extract_box_type(attrs: &[Attribute]) -> TokenStream {
    let mut iter = attrs.iter().filter(|attr| attr.path().is_ident("box_type"));
    let Some(attr) = iter.next() else {
        // When emitting compiler errors, no semicolon should be placed after `compile_error!()`:
        // doing so will generate extraneous errors (type mismatch errors, Rust parse errors, or the
        // like) in addition to the error we intend to emit.
        return quote! { std::compile_error!("missing `#[box_type]` attribute") };
    };
    if let Some(extra_attr) = iter.next() {
        return quote_spanned! { extra_attr.span() =>
            std::compile_error!("more than one `#[box_type]` attribute is not allowed")
        };
    }
    let lit = match &attr.meta {
        Meta::NameValue(MetaNameValue { value: Expr::Lit(expr_lit), .. }) => &expr_lit.lit,
        _ => {
            return quote_spanned! { attr.span() =>
                std::compile_error!("`box_type` attribute must be of the form `#[box_type = ...]`")
            }
        }
    };
    match &lit {
        Lit::Int(int_lit) => {
            let int = match int_lit.base10_parse::<u128>() {
                Ok(int) => int,
                Err(error) => return error.into_compile_error(),
            };
            if let Ok(int) = u32::try_from(int) {
                let bytes_lit = LitByteStr::new(&int.to_be_bytes(), int_lit.span());
                return quote! { mp4san::parse::BoxType::FourCC(mp4san::parse::FourCC { value: *#bytes_lit }) };
            } else {
                let bytes_lit = LitByteStr::new(&int.to_be_bytes(), int_lit.span());
                return quote! { mp4san::parse::BoxType::Uuid(mp4san::parse::BoxUuid { value: *#bytes_lit }) };
            }
        }
        Lit::Str(string_lit) => {
            if let Ok(uuid) = Uuid::parse_str(&string_lit.value()) {
                let bytes_lit = LitByteStr::new(&uuid.as_u128().to_be_bytes(), string_lit.span());
                return quote! { mp4san::parse::BoxType::Uuid(mp4san::parse::BoxUuid { value: *#bytes_lit }) };
            } else if string_lit.value().len() == 4 {
                let bytes_lit = LitByteStr::new(string_lit.value().as_bytes(), string_lit.span());
                return quote! {
                    mp4san::parse::BoxType::FourCC(mp4san::parse::FourCC { value: *#bytes_lit })
                };
            }
        }
        Lit::ByteStr(bytes_lit) => {
            if bytes_lit.value().len() == 4 {
                return quote! {
                    mp4san::parse::BoxType::FourCC(mp4san::parse::FourCC { value: *#bytes_lit })
                };
            }
        }
        _ => {}
    }
    quote_spanned! { lit.span() => std::compile_error!(concat!(
        r#"malformed `box_type` attribute input: try `"moov"`, `b"moov"`, or `0x6d6f6f76` for a"#,
        r#" compact type, or `"a7b5465c-7eac-4caa-b744-bdc340127d37"` or"#,
        r#" `0xa7b5465c_7eac_4caa_b744_bdc340127d37` for an extended type"#,
    )) }
}
