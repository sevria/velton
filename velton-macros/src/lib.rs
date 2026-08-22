//! Procedural macros for the `velton` REST API framework.
//!
//! This crate is not intended to be used directly; use the re-exports from
//! `velton` instead.

use proc_macro::TokenStream;

mod attr;
mod controller;
mod response;
mod schema;

/// Derives OpenAPI schema, request extraction and (de)serialization support.
#[proc_macro_derive(Schema, attributes(schema))]
pub fn derive_schema(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match schema::derive_schema(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Derives axum `IntoResponse` and OpenAPI response metadata.
#[proc_macro_derive(Response, attributes(response))]
pub fn derive_response(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match response::derive_response(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Marks an `impl` block as a controller.
///
/// Generates routes and OpenAPI documentation from the annotated methods.
#[proc_macro_attribute]
pub fn controller(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr = proc_macro2::TokenStream::from(attr);
    let item = proc_macro2::TokenStream::from(item);
    match controller::controller(attr, item) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn route_macro_error() -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "velton: route attributes like `#[get(...)]` must be used inside a `#[controller(...)]` impl block",
    )
    .to_compile_error()
    .into()
}

macro_rules! route_macro {
    ($name:ident) => {
        #[proc_macro_attribute]
        pub fn $name(_attr: TokenStream, _item: TokenStream) -> TokenStream {
            route_macro_error()
        }
    };
}

route_macro!(get);
route_macro!(post);
route_macro!(put);
route_macro!(delete);
route_macro!(patch);
route_macro!(options);
route_macro!(head);
route_macro!(any);
route_macro!(openapi);
