//! Procedural macros for the `velton` REST API framework.
//!
//! This crate is not intended to be used directly; use the re-exports from
//! `velton` instead.

use proc_macro::TokenStream;

mod attr;
mod controller;
mod schema;

/// Derives OpenAPI schema, request extraction and response support
/// (`IntoResponse` + `ResponseSchema`, enabled by a container-level
/// `#[schema(...)]`; `status_code` and `description` default to `200` /
/// `"OK"`).
///
/// `Serialize`/`Deserialize` are not generated; derive them from the `serde`
/// crate yourself.
#[proc_macro_derive(ToSchema, attributes(schema))]
pub fn derive_to_schema(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as syn::DeriveInput);
    match schema::derive_schema(input) {
        Ok(ts) => ts.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

/// Marks an `impl` block as a controller.
///
/// Endpoints are declared on the impl's methods with `#[endpoint(...)]`.
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

/// Declares a single endpoint on a controller method.
///
/// Takes `method`, `path`, `description` and `error_responses`; the
/// OpenAPI operation id is derived from the function name. Must be used inside
/// a `#[controller]` impl block.
#[proc_macro_attribute]
pub fn endpoint(_attr: TokenStream, _item: TokenStream) -> TokenStream {
    route_macro_error()
}

fn route_macro_error() -> TokenStream {
    syn::Error::new(
        proc_macro2::Span::call_site(),
        "velton: `#[endpoint(...)]` must be used inside a `#[controller]` impl block",
    )
    .to_compile_error()
    .into()
}
