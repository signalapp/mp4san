use proc_macro2::TokenStream;
use quote::{quote, quote_spanned, ToTokens};
use syn::spanned::Spanned;
use syn::Field;
use synstructure::{Structure, VariantInfo};

pub trait StructureExt {
    fn construct<F: FnMut(&VariantInfo, usize) -> T, T: ToTokens>(&self, fun: F) -> TokenStream;
    fn parse<F: FnMut(&VariantInfo, &Field, usize) -> T, T: ToTokens>(&self, fun: F) -> TokenStream;
}

impl StructureExt for Structure<'_> {
    fn construct<F: FnMut(&VariantInfo, usize) -> T, T: ToTokens>(&self, mut fun: F) -> TokenStream {
        let variants = self.variants().iter().enumerate();
        variants.fold(quote!({}), |acc, (variant_idx, variant)| {
            let parse_variant = fun(variant, variant_idx);
            if variant_idx != self.variants().len() - 1 {
                quote! { #acc let _ = { #parse_variant }; }
            } else {
                quote! { #acc #parse_variant }
            }
        })
    }
    fn parse<F: FnMut(&VariantInfo, &Field, usize) -> T, T: ToTokens>(&self, mut fun: F) -> TokenStream {
        self.construct(|variant, _idx| {
            let parse_expr = variant.construct(|field, idx| {
                let parse_expr = fun(variant, field, idx);
                match idx {
                    0 => quote_spanned! {
                        field.span() => {
                            let parse_result = { #parse_expr };
                            match parse_result {
                                Ok(parsed) => parsed,
                                Err(err) => break 'parse_variant Err::<Self, _>(err),
                            }
                        }
                    },
                    _ => quote_spanned! { field.span() => { #parse_expr }? },
                }
            });

            let label = (!variant.bindings().is_empty()).then(|| quote! { 'parse_variant: });
            quote_spanned! {
                variant.ast().ident.span() => #label {
                    let parsed = { #parse_expr };
                    #[allow(clippy::needless_return)]
                    return Ok(parsed);
                }
            }
        })
    }
}
