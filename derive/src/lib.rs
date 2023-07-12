use proc_macro::TokenStream;

use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use syn::{parse_macro_input, Data, DeriveInput, Ident, Index, Lit, Meta};
use uuid::Uuid;

#[proc_macro_derive(ParseBox, attributes(box_type))]
pub fn derive_parse_box(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    if matches!(input.data, Data::Enum(_) | Data::Union(_)) {
        // This one _does_ need a semicolon though.
        return TokenStream::from(quote! {
            std::compile_error!("this trait can only be derived for structs");
        });
    }
    let box_type = extract_box_type(&input);
    let read_fn = derive_read_fn(&input);

    TokenStream::from(quote! {
        #[automatically_derived]
        impl #impl_generics mp4san::parse::ParseBox for #ident #ty_generics #where_clause {
            fn box_type() -> mp4san::parse::BoxType {
                #box_type
            }

            #read_fn
        }
    })
}

#[proc_macro_derive(ParsedBox, attributes(box_type))]
pub fn derive_parsed_box(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    if matches!(input.data, Data::Enum(_) | Data::Union(_)) {
        // This one _does_ need a semicolon though.
        return TokenStream::from(quote! {
            std::compile_error!("this trait can only be derived for structs");
        });
    }
    let size = sum_box_size(&input);
    let write_fn = derive_write_fn(&input);

    TokenStream::from(quote! {
        #[automatically_derived]
        impl #impl_generics mp4san::parse::ParsedBox for #ident #ty_generics #where_clause {
            fn encoded_len(&self) -> u64 {
                #size
            }

            #write_fn
        }
    })
}

fn derive_write_fn(input: &DeriveInput) -> TokenStream2 {
    let write_fields = match &input.data {
        Data::Struct(struct_data) => {
            let place_expr = struct_data.fields.iter().enumerate().map(|(index, field)| {
                if let Some(ident) = &field.ident {
                    quote_spanned! { field.span() => self.#ident }
                } else {
                    let tuple_index = Index::from(index);
                    quote_spanned! { field.span() => self.#tuple_index }
                }
            });
            quote! { #( mp4san::parse::Mpeg4Int::put_buf(&#place_expr, &mut *out); )* }
        }
        _ => unreachable!(),
    };
    quote! {
        fn put_buf(&self, out: &mut dyn bytes::BufMut) {
            #write_fields
        }
    }
}

fn derive_read_fn(input: &DeriveInput) -> TokenStream2 {
    let ident = &input.ident;
    match &input.data {
        Data::Struct(struct_data) => {
            let mut field_ty = Vec::new();
            let mut field_ident = Vec::new();
            let mut bind_ident = Vec::new();
            for (index, field) in struct_data.fields.iter().enumerate() {
                field_ty.push(field.ty.clone());
                if let Some(ident) = &field.ident {
                    field_ident.push(quote_spanned! { field.span() => #ident });
                    bind_ident.push(ident.clone());
                } else {
                    let tuple_index = Index::from(index);
                    field_ident.push(quote_spanned! { field.span() => #tuple_index });
                    bind_ident.push(Ident::new(&format!("field_{index}"), Span::mixed_site()));
                }
            }
            quote! {
                fn parse(buf: &mut bytes::BytesMut) -> std::result::Result<Self, mp4san::Report<mp4san::parse::ParseError>> {
                    #(
                        let #bind_ident: #field_ty =
                            mp4san::parse::error::__ParseResultExt::while_parsing_field(
                                mp4san::parse::Mpeg4Int::parse(&mut *buf),
                                #ident::box_type(),
                                stringify!(#field_ty),
                            )?;
                    )*
                    if !buf.is_empty() {
                        return
                            mp4san::parse::error::__ParseResultExt::while_parsing_box(
                                mp4san::error::__ResultExt::attach_printable(
                                    Err(ParseError::InvalidInput.into()),
                                    "extra unparsed data",
                                ),
                                #ident::box_type(),
                            );
                    }
                    std::result::Result::Ok(#ident { #( #field_ident: #bind_ident ),* })
                }
            }
        }
        _ => unreachable!(),
    }
}

fn extract_box_type(input: &DeriveInput) -> TokenStream2 {
    let mut iter = input.attrs.iter().filter(|attr| attr.path.is_ident("box_type"));
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
    let lit = match attr.parse_meta() {
        Ok(Meta::NameValue(name_value)) => name_value.lit,
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
                return quote! { mp4san::parse::BoxType::FourCC(mp4san::parse::FourCC { value: #int.to_be_bytes() }) };
            } else {
                return quote! { mp4san::parse::BoxType::Uuid(mp4san::parse::BoxUuid { value: #int.to_be_bytes() }) };
            }
        }
        Lit::Str(string_lit) => {
            let string = string_lit.value();
            if let Ok(uuid) = Uuid::parse_str(&string) {
                let int = uuid.as_u128();
                return quote! { mp4san::parse::BoxType::Uuid(mp4san::parse::BoxUuid { value: #int.to_be_bytes() }) };
            } else if string.len() == 4 {
                return quote! {
                    let type_string = #string_lit;
                    let type_ = std::convert::TryInto::try_into(type_string.as_bytes()).unwrap();
                    mp4san::parse::BoxType::FourCC(mp4san::parse::FourCC { value: type_ })
                };
            }
        }
        Lit::ByteStr(bytes_lit) => {
            let bytes = bytes_lit.value();
            if bytes.len() == 4 {
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

fn sum_box_size(derive_input: &DeriveInput) -> TokenStream2 {
    let sum_expr = match &derive_input.data {
        Data::Struct(struct_data) => {
            let sum_expr = struct_data.fields.iter().map(|field| {
                let ty = &field.ty;
                quote_spanned! { field.span() => <#ty as mp4san::parse::Mpeg4Int>::encoded_len() }
            });
            quote! { #(+ #sum_expr)* }
        }
        _ => unreachable!(),
    };
    quote! {
        0 #sum_expr
    }
}
