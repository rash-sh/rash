#![deny(warnings)]
extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;

/// Implementation of the `#[derive(FieldNames)]` derive macro.
///
/// Add a new method which return field names
/// ```
/// # use std::collections::HashSet;
/// pub fn get_field_names() -> HashSet<String>
/// # {
/// # HashSet::new()
/// # }
/// ```
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
            /// Return field names.
            pub fn get_field_names() -> std::collections::HashSet<String> {
                [#(#field_names),*].iter().map(ToString::to_string).map(|s| s.replace("r#", "")).collect::<std::collections::HashSet<String>>()
            }
        }
    };
    TokenStream::from(expanded)
}

/// Implementation of the `#[derive(DocJsonSchema)]` derive macro.
/// This also requires #[derive(JsonSchema)] from schemars.
///
/// Add a new method which returns a JsonSchema
/// ```
/// # use schemars::schema::RootSchema;
/// pub fn get_json_schema() -> RootSchema
/// # {
/// #   unimplemented!()
/// # }
/// ```
#[cfg(feature = "docs")]
#[proc_macro_derive(DocJsonSchema)]
pub fn derive_doc_json_schema(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);

    let name = &input.ident;

    let expanded = quote::quote! {
        impl #name {
            /// Return Json Schema.
            pub fn get_json_schema() -> schemars::schema::RootSchema {
                let settings = schemars::gen::SchemaSettings::default().with(|s| {
                    s.inline_subschemas = true;
                });
                let gen = settings.into_generator();
                gen.into_root_schema_for::<#name>()
            }
        }
    };
    TokenStream::from(expanded)
}
