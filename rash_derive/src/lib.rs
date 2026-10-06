extern crate proc_macro;

use proc_macro::TokenStream;
use quote::quote;
use syn::{Expr, ExprLit, ExprTuple, Lit, Token, parse_macro_input, punctuated::Punctuated};

/// Implementation of the `#[derive(FieldNames)]` derive macro.
///
/// Add a constant and a method with the names of the struct fields (raw identifiers like
/// `r#loop` are returned as `loop`). Fields marked with `#[field_names(skip)]` are left out.
/// ```
/// # use std::collections::HashSet;
/// pub const FIELD_NAMES: &'static [&'static str] = &[];
/// pub fn get_field_names() -> HashSet<String>
/// # {
/// # HashSet::new()
/// # }
/// ```
#[proc_macro_derive(FieldNames, attributes(field_names))]
pub fn derive_field_names(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);

    let name = &input.ident;
    let generics = &input.generics; // Handle lifetimes and generics here
    let where_clause = &generics.where_clause;

    let mut field_names = Vec::new();
    for field in &input.fields {
        match is_skipped(field) {
            Ok(true) => continue,
            Ok(false) => {}
            Err(error) => return error.to_compile_error().into(),
        }
        if let Some(ident) = &field.ident {
            field_names.push(ident.to_string().trim_start_matches("r#").to_owned());
        }
    }

    quote! {
        impl #generics #name #generics #where_clause {
            /// Field names.
            pub const FIELD_NAMES: &'static [&'static str] = &[#(#field_names),*];

            /// Return field names.
            pub fn get_field_names() -> std::collections::HashSet<String> {
                Self::FIELD_NAMES.iter().map(ToString::to_string).collect()
            }
        }
    }
    .into()
}

/// Whether a field has the `#[field_names(skip)]` attribute.
fn is_skipped(field: &syn::Field) -> syn::Result<bool> {
    let mut skipped = false;
    for attr in field
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("field_names"))
    {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                skipped = true;
                Ok(())
            } else {
                Err(meta.error("unsupported field_names attribute, expected `skip`"))
            }
        })?;
    }
    Ok(skipped)
}

/// Implementation of the `#[derive(DocJsonSchema)]` derive macro.
/// This also requires #[derive(JsonSchema)] from schemars.
///
/// Add a new method which returns a JsonSchema
/// ```
/// # use schemars::Schema;
/// pub fn get_json_schema() -> Schema
/// # {
/// #   unimplemented!()
/// # }
/// ```
#[cfg(feature = "docs")]
#[proc_macro_derive(DocJsonSchema)]
pub fn derive_doc_json_schema(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::ItemStruct);

    let name = &input.ident;

    quote! {
        impl #name {
            /// Return Json Schema.
            pub fn get_json_schema() -> schemars::Schema {
                let settings = schemars::generate::SchemaSettings::default().with(|s| {
                    s.inline_subschemas = true;
                });
                let generator = settings.into_generator();
                generator.into_root_schema_for::<#name>()
            }
        }
    }
    .into()
}

#[cfg(not(doctest))]
/// Macro to generate a function that adds lookup functions to a `minijinja::Environment`.
///
/// This macro generates an `add_lookup_functions` function that registers multiple lookup
/// functions into a `minijinja::Environment`. Each function can be conditionally compiled
/// based on the presence of a corresponding feature flag if specified in the tuple.
///
/// Additionally, when the `docs` feature is enabled, it will generate a `LOOKUPS` constant that
/// lists all the lookup function names.
///
/// # Example
///
/// Assuming you have three modules `lookup1`, `lookup2`, and `lookup3`, each with a `function`
/// that you want to add to the environment, you would use the macro like this:
///
/// ```rust
/// mod lookup1;
/// mod lookup2;
/// mod lookup3;
///
/// use my_macro::generate_lookup_functions;
///
/// generate_lookup_functions!((lookup1, true), (lookup2, false), (lookup3, true));
/// ```
///
/// This will generate the following function:
///
/// ```rust
/// pub fn add_lookup_functions(env: &mut minijinja::Environment<'static>) {
///     #[cfg(feature = "lookup1")]
///     env.add_function("lookup1", lookup1::function);
///
///
///     #[cfg(feature = "lookup2")]
///
///     #[cfg(feature = "lookup2")]
///     env.add_function("lookup2", lookup2::function);
///
///     #[cfg(feature = "lookup3")]
///     env.add_function("lookup3", lookup3::function);
/// }
/// ```
///
/// When the `docs` feature is enabled, it will also generate the following constant:
///
/// ```rust
/// #[cfg(feature = "docs")]
/// const LOOKUPS: &[&str] = &[
///     "lookup1",
///     "lookup2",
///     "lookup3",
/// ];
/// ```
///
/// You can control which functions are included by specifying the corresponding features
/// in your `Cargo.toml`:
///
/// ```toml
/// [features]
/// lookup1 = []
/// lookup2 = []
/// lookup3 = []
/// docs = []
/// ```
///
/// When building your crate with the `docs` feature, the `LOOKUPS` constant will be included:
///
/// ```sh
/// cargo build --features "docs"
/// ```
#[proc_macro]
pub fn generate_lookup_functions(input: TokenStream) -> TokenStream {
    // Parse the input tokens into a punctuated list of tuples separated by commas
    let tuples =
        parse_macro_input!(input with Punctuated::<ExprTuple, Token![,]>::parse_terminated);

    let mut add_functions = Vec::new();
    let mut lookup_names = Vec::new();

    for tuple in tuples.iter() {
        if let (
            Some(Expr::Path(path)),
            Some(Expr::Lit(ExprLit {
                lit: Lit::Bool(lit_bool),
                ..
            })),
        ) = (tuple.elems.first(), tuple.elems.last())
        {
            let func_name = path.path.segments.first().unwrap().ident.to_string(); // Extract function name
            lookup_names.push(func_name.clone());

            if lit_bool.value {
                add_functions.push(quote! {
                    #[cfg(feature = #func_name)]
                    env.add_function(#func_name, #path::function);
                });
            } else {
                add_functions.push(quote! {
                    env.add_function(#func_name, #path::function);
                });
            }
        }
    }

    quote! {
        pub fn add_lookup_functions(env: &mut minijinja::Environment<'static>) {
            #(#add_functions)*
        }

        #[cfg(feature = "docs")]
        pub const LOOKUPS: &[&str] = &[
            #(#lookup_names),*
        ];
    }
    .into()
}
