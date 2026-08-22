//! `#[derive(Response)]` codegen.
//!
//! Generates an axum `IntoResponse` implementation (JSON with the configured
//! status code) and a `ResponseSchema` implementation for OpenAPI.

use crate::attr::ResponseAttr;
use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

pub fn derive_response(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let attr = ResponseAttr::from_attrs(&input.attrs)?;
    let code = attr.code.unwrap_or(200);
    let description = attr.description.unwrap_or_else(|| "OK".to_string());

    let status = quote! {
        ::velton::axum::http::StatusCode::from_u16(#code)
            .expect("velton: invalid status code in `#[response(code = ...)]`")
    };

    Ok(quote! {
        impl ::velton::axum::response::IntoResponse for #name {
            fn into_response(self) -> ::velton::axum::response::Response {
                ::velton::axum::response::IntoResponse::into_response((
                    #status,
                    ::velton::axum::Json(self),
                ))
            }
        }

        impl ::velton::response::ResponseSchema for #name {
            fn status() -> ::velton::axum::http::StatusCode {
                #status
            }
            fn description() -> &'static str {
                #description
            }
        }
    })
}
