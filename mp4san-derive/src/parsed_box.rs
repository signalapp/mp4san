use proc_macro2::TokenStream;
use quote::{quote, quote_spanned};
use syn::spanned::Spanned;
use synstructure::Structure;

pub(crate) fn derive(input: Structure) -> TokenStream {
    let size = input.fold(0u64, |acc, binding| quote! { #acc + Mp4Value::encoded_len(#binding) });
    let write_fn = derive_write_fn(&input);

    input.gen_impl(quote! {
        use std::prelude::v1::*;

        use mp4san::parse::{Mp4Value, ParsedBox};

        #[automatically_derived]
        gen impl ParsedBox for @Self {
            fn encoded_len(&self) -> u64 {
                match *self { #size }
            }

            #write_fn
        }
    })
}

fn derive_write_fn(input: &Structure) -> TokenStream {
    let write_fields = input.each(|binding| {
        quote_spanned! { binding.ast().span() => Mp4Value::put_buf(#binding, &mut *out); }
    });
    quote! {
        fn put_buf(&self, out: &mut dyn bytes::BufMut) {
            match *self { #write_fields }
        }
    }
}
