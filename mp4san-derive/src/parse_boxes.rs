use proc_macro2::TokenStream;
use quote::{format_ident, quote, quote_spanned};
use syn::spanned::Spanned;
use syn::token::Mut;
use syn::{parse2, Data, DataEnum, DataStruct, DeriveInput, Field, Fields};
use synstructure::{BindStyle, Structure, VariantAst, VariantInfo};

use crate::attr::extract_box_type;

pub(crate) fn derive(input: Structure) -> TokenStream {
    if let Data::Union(_) = &input.ast().data {
        return quote! {
            std::compile_error!("this trait cannot be derived for unions");
        };
    };
    let [variant] = input.variants() else {
        return quote! {
            std::compile_error!("this trait can only be derived for structs or single-variant enums");
        };
    };

    let box_type = extract_box_type(&input.ast().attrs);

    let ref_item @ DeriveInput { ident: ref_item_ident, generics: ref_item_generics, .. } =
        &parse2(derive_ref_item(&input, false)).unwrap();
    let ref_variant = Structure::new(ref_item).variants()[0].clone();

    let ref_mut_item @ DeriveInput { ident: ref_mut_item_ident, generics: ref_mut_item_generics, .. } =
        &parse2(derive_ref_item(&input, true)).unwrap();
    let ref_mut_variant = Structure::new(ref_mut_item).variants()[0].clone();

    let validate_fn_body = derive_parse_fn_body(variant, None, ParseFnKind::Validate);
    let parse_fn_body = derive_parse_fn_body(variant, Some(&ref_mut_variant), ParseFnKind::Parse);
    let parsed_fn_body = derive_parse_fn_body(variant, Some(&ref_variant), ParseFnKind::Parsed);
    let into_iter_fn = derive_into_iter_fn(&input);

    let parse_boxes_impl = input.gen_impl(quote! {
        use std::vec;
        use std::prelude::v1::*;

        use mp4san::error::Report;
        use mp4san::parse::{AnyMp4Box, Boxes, BoxType, Mp4Box, ParseBoxes, ParseError};
        use mp4san::parse::derive::parse_boxes::{Field, Accumulator};
        use mp4san::parse::error::__ParseResultExt;

        type FieldType<T> = <T as Field>::Type;
        type FieldAccumulator<T, U> = <T as Field>::Accumulator<U>;
        type FieldAccumulatorUnwrapped<T, U> = <<T as Field>::Accumulator<U> as Accumulator<U>>::Unwrapped;

        const NAME: BoxType = #box_type;

        #[automatically_derived]
        gen impl ParseBoxes for @Self {
            type Ref<'a> = #ref_item_ident #ref_item_generics;
            type RefMut<'a> = #ref_mut_item_ident #ref_mut_item_generics;
            type IntoIter = vec::IntoIter<AnyMp4Box>;

            fn validate<'boxes>(boxes: &'boxes mut [AnyMp4Box]) -> Result<(), Report<ParseError>> {
                #validate_fn_body
            }

            fn parse<'boxes>(boxes: &'boxes mut [AnyMp4Box]) -> Result<Self::RefMut<'boxes>, Report<ParseError>>
            where
                Self: 'boxes,
            {
                #parse_fn_body
            }

            fn parsed<'boxes>(boxes: &'boxes [AnyMp4Box]) -> Self::Ref<'boxes>
            where
                Self: 'boxes,
            {
                let parsed_impl = |boxes: &'boxes [AnyMp4Box]| -> Result<Self::Ref<'boxes>, Report<ParseError>> {
                    #parsed_fn_body
                };
                parsed_impl(boxes).unwrap()
            }

            #into_iter_fn
        }
    });

    quote! {
        #parse_boxes_impl

        #[derive(Clone, Debug)]
        #[allow(clippy::type_complexity)]
        #ref_item

        #[derive(Debug)]
        #[allow(clippy::type_complexity)]
        #ref_mut_item
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ParseFnKind {
    Validate,
    Parse,
    Parsed,
}

fn derive_parse_fn_body(
    variant: &VariantInfo<'_>,
    return_variant: Option<&VariantInfo<'_>>,
    parse_fn_kind: ParseFnKind,
) -> TokenStream {
    let declare_field = variant.bindings().iter().map(|binding| {
        let Field { ty, ident, .. } = binding.ast();
        quote_spanned! { ident.span() => let mut #binding: FieldAccumulator<#ty, _> = Default::default(); }
    });

    let parse_match_arm = variant.bindings().iter().map(|binding| {
        let Field { ty, .. } = binding.ast();
        let parse_expr = match parse_fn_kind {
            ParseFnKind::Validate => quote! { mp4box.parse_data_as()?.map(drop::<&mut FieldType<#ty>>) },
            ParseFnKind::Parse => quote! { mp4box.parse_data_as()? },
            ParseFnKind::Parsed => quote! { mp4box.data.parsed() },
        };
        quote! {
            box_type if box_type == <FieldType<#ty>>::NAME && !Accumulator::is_full(&#binding) => {
                if let Some(field) = #parse_expr {
                    Accumulator::push(&mut #binding, field);
                }
            },
        }
    });

    let unwrap_field = variant.bindings().iter().map(|binding| {
        let Field { ty, ident, .. } = binding.ast();
        quote_spanned! {
            ident.span() =>
                let #binding = Accumulator::unwrap(#binding)
                    .ok_or_else(|| Report::from(ParseError::MissingRequiredBox(<FieldType<#ty>>::NAME)))
                    .while_parsing_child(NAME, <FieldType<#ty>>::NAME)?;
        }
    });

    let return_value = match return_variant {
        Some(return_variant) => return_variant.construct(|_field, idx| &variant.bindings()[idx]),
        None => quote!(()),
    };

    quote! {
        #(#declare_field)*
        for mp4box in boxes {
            match mp4box.box_type() {
                #(#parse_match_arm)*
                _ => (),
            }
        }
        #(#unwrap_field)*
        Ok(#return_value)
    }
}

fn derive_into_iter_fn(input: &Structure) -> TokenStream {
    let variant_len = input.variants().iter().map(|variant| variant.bindings().len());
    let field_count = variant_len.sum::<usize>();

    let push_fields = input
        .clone()
        .bind_with(|_| BindStyle::Move)
        .fold(quote!(), |acc, binding| {
            let push_fields = quote_spanned! {
                binding.ast().span() =>
                    for mp4box in Field::into_iter(#binding) {
                        fields.push(AnyMp4Box::from(Mp4Box::with_parsed(Box::new(mp4box))?));
                    }
            };
            quote! { #acc #push_fields }
        });

    quote! {
        fn try_into_iter(self) -> Result<Self::IntoIter, Report<ParseError>> {
            let mut fields = Vec::with_capacity(#field_count);
            match self { #push_fields }
            Ok(IntoIterator::into_iter(fields))
        }
    }
}

fn derive_ref_item(input: &Structure, mutable: bool) -> TokenStream {
    let DeriveInput { ident, vis, data, .. } = input.ast();

    let ident = match mutable {
        false => format_ident!("{ident}Ref", span = ident.span()),
        true => format_ident!("{ident}RefMut", span = ident.span()),
    };

    let lt_a = (!input.variants().iter().all(|variant| variant.bindings().is_empty())).then(|| quote! { 'a });
    let path = quote! { mp4san::parse::derive::parse_boxes:: };

    let mutability = mutable.then(Mut::default);
    let variants = input.variants().iter().fold(quote!(), |acc, variant| {
        let VariantAst { ident, discriminant, .. } = variant.ast();
        let discriminant = discriminant
            .as_ref()
            .map(|(eq, expr)| quote! { #eq #expr })
            .unwrap_or_default();
        let fields = variant.bindings().iter().fold(quote!(), |acc, binding| {
            let Field { vis, ident, colon_token, ty, .. } = binding.ast();
            let ty_as_field = quote_spanned! { ident.span() => #ty as #path Field };
            let ty_ref = quote_spanned! { ident.span() => &'a #mutability <#ty_as_field>::Type };
            let declare_field = quote_spanned! {
                ident.span() => #vis #ident #colon_token
                    <<#ty_as_field>::Accumulator<#ty_ref> as #path Accumulator<#ty_ref>>::Unwrapped,
            };
            quote! { #acc #declare_field }
        });
        match &input.ast().data {
            Data::Enum { .. } => match &variant.ast().fields {
                Fields::Unnamed { .. } => quote! { #acc #ident #discriminant(#fields), },
                Fields::Named { .. } => quote! { #acc #ident #discriminant { #fields }, },
                Fields::Unit { .. } => quote! { #acc #ident #discriminant, },
            },
            Data::Union { .. } | Data::Struct { .. } => quote! { #acc #fields },
        }
    });

    match data {
        Data::Enum(DataEnum { enum_token, .. }) => quote! { #vis #enum_token #ident<#lt_a> { #variants } },
        Data::Struct(DataStruct { struct_token, fields, .. }) => match fields {
            Fields::Named(_) => quote! { #vis #struct_token #ident<#lt_a> { #variants } },
            Fields::Unnamed(_) => quote! { #vis #struct_token #ident<#lt_a>(#variants); },
            Fields::Unit => quote! { #vis #struct_token #ident<#lt_a>; },
        },
        Data::Union(_) => unreachable!(),
    }
}

#[cfg(test)]
mod test {
    use syn::parse2;

    use super::*;

    fn test_derive(input: TokenStream) -> TokenStream {
        derive(Structure::new(&parse2(input).unwrap()))
    }

    #[test]
    fn empty_struct() {
        test_derive(quote! {
            struct Empty {}
        });
    }

    #[test]
    fn unit_struct() {
        test_derive(quote! {
            struct Empty;
        });
    }
}
