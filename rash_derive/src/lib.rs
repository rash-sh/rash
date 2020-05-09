#![deny(warnings)]
extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;

/// Implementation of the `#[derive(FieldNames)]` derive macro.
#[proc_macro_derive(FieldNames, attributes(field_names))]
pub fn derive_field_names(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);

    let name = &input.ident;
    let field_names: Vec<String> = input
        .fields
        .iter()
        .map(|field| field.ident.clone().unwrap().to_string())
        .collect();

    let expanded = quote::quote! {
        impl #name {
            pub fn get_field_names() -> std::collections::HashSet<String> {
                [#(#field_names),*].iter().map(ToString::to_string).collect::<std::collections::HashSet<String>>()
            }
        }
    };
    TokenStream::from(expanded)
}
