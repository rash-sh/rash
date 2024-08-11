#![deny(warnings)]
extern crate proc_macro;
extern crate quote;
extern crate syn;

use proc_macro::TokenStream;
use quote::quote;
use syn::{parse_macro_input, punctuated::Punctuated, Ident, Token};

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

#[cfg(not(doctest))]
/// Macro to generate a function that adds lookup functions to a `minijinja::Environment`.
///
/// This macro generates an `add_lookup_functions` function that registers multiple lookup
/// functions into a `minijinja::Environment`, with each function being conditionally compiled
/// based on the presence of a corresponding feature flag.
///
/// # Example
///
/// Assuming you have three modules `lookup1`, `lookup2`, and `lookup3`, each with a `function`
/// that you want to add to the environment, you would use the macro like this:
///
/// ```
/// mod lookup1;
/// mod lookup2;
/// mod lookup3;
///
/// use rash_derive::generate_lookup_functions;
///
/// generate_lookup_functions!(lookup1, lookup2, lookup3);
/// ```
///
/// This will generate the following function:
///
/// ```rust
/// pub fn add_lookup_functions(env: &mut minijinja::Environment<'static>) {
///     #[cfg(feature = "lookup1")]
///     env.add_function("lookup1", lookup1::function);
///
///     #[cfg(feature = "lookup2")]
///     env.add_function("lookup2", lookup2::function);
///
///     #[cfg(feature = "lookup3")]
///     env.add_function("lookup3", lookup3::function);
/// }
/// ```
///
/// You can then control which functions are included by specifying the corresponding features
/// in your `Cargo.toml`:
///
/// ```toml
/// [features]
/// lookup1 = []
/// lookup2 = []
/// lookup3 = []
/// ```
///
/// When building your crate, you can enable the desired features:
///
/// ```sh
/// cargo build --features "lookup1 lookup2"
/// ```
#[proc_macro]
pub fn generate_lookup_functions(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a punctuated list of identifiers separated by commas
    let function_names =
        parse_macro_input!(input with Punctuated::<Ident, Token![,]>::parse_terminated);

    let mut add_functions = Vec::new();

    // Iterate through each identifier and generate the corresponding function call with #[cfg]
    for func in function_names.iter() {
        let func_name_str = func.to_string(); // Convert the identifier to a string
        add_functions.push(quote! {
            #[cfg(feature = #func_name_str)]
            env.add_function(#func_name_str, #func::function);
        });
    }

    // Generate the output function code
    let output = quote! {
        pub fn add_lookup_functions(env: &mut minijinja::Environment<'static>) {
            #(#add_functions)*
        }
    };

    // Convert the generated code back into a TokenStream and return it
    output.into()
}
