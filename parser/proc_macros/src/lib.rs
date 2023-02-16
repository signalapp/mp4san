use proc_macro::TokenStream;

use proc_macro2::TokenStream as TokenStream2;
use quote::{quote, quote_spanned};
use syn::{parse_macro_input, Data, DeriveInput};
use syn::spanned::Spanned;

#[proc_macro_derive(Mp4Box)]
pub fn derive_mp4_box(input: TokenStream) -> TokenStream {
    let input = parse_macro_input!(input as DeriveInput);
    let ident = &input.ident;
    let (impl_generics, ty_generics, where_clause) = input.generics.split_for_impl();

    let size = sum_box_size(&input);

    let expanded = quote! {
        impl #impl_generics mp4san_isomparse::Mp4Box for #ident #ty_generics #where_clause {
            fn size(&self) -> std::primitive::u64 {
                std::convert::TryFrom::try_from(#size).unwrap()
            }
        }
    };

    TokenStream::from(expanded)
}

fn sum_box_size(derive_input: &DeriveInput) -> TokenStream2 {
    let sum_expr = match &derive_input.data {
        Data::Struct(struct_data) => {
            let sum_expr = struct_data.fields.iter().map(|field| {
                let ty = &field.ty;
                quote_spanned! { field.span() => std::mem::size_of::<#ty>() }
            });
            quote! { #(+ #sum_expr)* }
        },
        Data::Enum(enum_data) => {
            let enum_ident = &derive_input.ident;
            let arms = enum_data.variants.iter().map(|variant| {
                let ident = &variant.ident;
                let sum_expr = variant.fields.iter().map(|field| {
                    let ty = &field.ty;
                    quote_spanned! { field.span() => std::mem::size_of::<#ty>() }
                });
                quote! {
                    #enum_ident::#ident { .. } => { 0 #(+ #sum_expr)* },
                }
            });
            quote! { + match self { #(#arms)* } }
        },
        Data::Union(_) => panic!("this trait cannot be derived for unions"),
    };
    quote! { std::mem::size_of::<std::primitive::u32>() #sum_expr }
}
