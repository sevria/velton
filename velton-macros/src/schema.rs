//! `#[derive(ToSchema)]` codegen.
//!
//! For structs this generates:
//! * `impl ToSchema` (a `$ref` plus recursive component registration),
//! * `impl RequestSchema` (OpenAPI parameters + request body),
//! * `impl FromRequest` (extraction from body/query/path/header), and
//! * when the struct carries a `#[schema(...)]` container attribute,
//!   `impl IntoResponse` and `impl ResponseSchema` (response support;
//!   `status_code` and `description` default to `200` / `"OK"`).
//!
//! Serde `Serialize`/`Deserialize` are **not** generated here — derive them
//! from the `serde` crate on your types (velton no longer bundles serde).

use crate::attr::{SchemaAttr, Source, expr_value_expr, is_option, lit_value_expr, option_inner};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{Data, DeriveInput, Fields};

pub fn derive_schema(input: DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    match &input.data {
        Data::Struct(data) => derive_struct(&input, name, data),
        Data::Enum(data) => derive_enum(&input, name, data),
        Data::Union(_) => Err(syn::Error::new_spanned(
            name,
            "velton: `ToSchema` cannot be derived for unions",
        )),
    }
}

/// Fields of a struct with their parsed `#[schema]` attrs.
struct FieldSpec<'a> {
    ident: &'a syn::Ident,
    ty: &'a syn::Type,
    attrs: SchemaAttr,
}

fn struct_fields(data: &syn::DataStruct) -> syn::Result<Vec<FieldSpec<'_>>> {
    let mut fields = Vec::new();
    for field in &data.fields {
        let ident = field.ident.as_ref().ok_or_else(|| {
            syn::Error::new_spanned(
                field,
                "velton: tuple structs are not supported by `#[derive(ToSchema)]`",
            )
        })?;
        fields.push(FieldSpec {
            ident,
            ty: &field.ty,
            attrs: SchemaAttr::from_attrs(&field.attrs)?,
        });
    }
    Ok(fields)
}

fn check_no_generics(input: &DeriveInput) -> syn::Result<()> {
    if !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            "velton: generic types are not yet supported by `#[derive(ToSchema)]`",
        ));
    }
    Ok(())
}

/// `X-MyField` header name for a snake_case field, unless renamed.
fn header_name(ident: &str, rename: Option<&str>) -> String {
    match rename {
        Some(r) => r.to_string(),
        None => format!("X-{}", upper_camel_case(ident)),
    }
}

fn upper_camel_case(input: &str) -> String {
    input
        .split(['_', '-'])
        .filter(|w| !w.is_empty())
        .map(|word| {
            let mut chars = word.chars();
            let first = chars
                .next()
                .map(|c| c.to_uppercase().collect::<String>())
                .unwrap_or_default();
            format!("{first}{}", chars.as_str())
        })
        .collect::<Vec<_>>()
        .join("-")
}

/// Builds an expression that decorates `<Ty as ToSchema>::schema()`.
fn decorated_schema(field_ty: &syn::Type, attrs: &SchemaAttr) -> TokenStream {
    let base = quote!(<#field_ty as ::velton::schema::ToSchema>::schema());
    let mut stmts: Vec<TokenStream> = Vec::new();
    if let Some(d) = &attrs.description {
        stmts.push(quote!(__schema.description = Some(#d.to_string());));
    }
    if let Some(lit) = &attrs.example {
        let value = lit_value_expr(lit);
        stmts.push(quote!(__schema.example = Some(#value);));
    }
    if let Some(f) = &attrs.format {
        stmts.push(quote!(__schema.format = Some(#f.to_string());));
    }
    if let Some(t) = &attrs.title {
        stmts.push(quote!(__schema.title = Some(#t.to_string());));
    }
    if let Some(d) = &attrs.default {
        let value = expr_value_expr(d);
        stmts.push(quote!(__schema.default = Some(#value);));
    }
    if attrs.deprecated {
        stmts.push(quote!(__schema.deprecated = true;));
    }
    if let Some(v) = attrs.minimum {
        stmts.push(quote!(__schema.minimum = Some(#v);));
    }
    if let Some(v) = attrs.maximum {
        stmts.push(quote!(__schema.maximum = Some(#v);));
    }
    if let Some(v) = attrs.min_length {
        stmts.push(quote!(__schema.min_length = Some(#v);));
    }
    if let Some(v) = attrs.max_length {
        stmts.push(quote!(__schema.max_length = Some(#v);));
    }
    if let Some(v) = &attrs.pattern {
        stmts.push(quote!(__schema.pattern = Some(#v.to_string());));
    }
    quote!({
        let mut __schema = #base;
        #(#stmts)*
        __schema
    })
}

/// `#[serde(rename = "...")]` tokens for extraction hidden structs, so requests
/// deserialize using the wire names declared by `#[schema(rename = ...)]`.
fn extraction_rename_tokens(field: &FieldSpec<'_>) -> TokenStream {
    if let Some(r) = &field.attrs.rename {
        quote!(#[serde(rename = #r)])
    } else {
        quote!()
    }
}

fn derive_struct(
    input: &DeriveInput,
    name: &syn::Ident,
    data: &syn::DataStruct,
) -> syn::Result<TokenStream> {
    check_no_generics(input)?;
    let fields = struct_fields(data)?;
    let container = SchemaAttr::from_attrs(&input.attrs)?;
    // A container-level `#[schema(...)]` marks the struct as a response type
    // (it is the response configuration point). `status_code` and
    // `description` are optional and default to `200` / `"OK"`.
    let is_response = input
        .attrs
        .iter()
        .any(|attr| attr.path().is_ident("schema"));

    let schema_impl = struct_to_schema_impl(name, &fields)?;
    let request_impl = struct_request_schema_impl(name, &fields)?;
    let from_request_impl = struct_from_request_impl(name, &fields)?;
    let response_impl = struct_response_impl(name, &container, is_response)?;

    Ok(quote! {
        #schema_impl
        #request_impl
        #from_request_impl
        #response_impl
    })
}

fn struct_to_schema_impl(name: &syn::Ident, fields: &[FieldSpec<'_>]) -> syn::Result<TokenStream> {
    let name_str = name.to_string();

    let mut property_tuples = Vec::new();
    let mut required_names = Vec::new();
    let mut nested_calls = Vec::new();

    for f in fields {
        let prop_name = f
            .attrs
            .rename
            .clone()
            .unwrap_or_else(|| f.ident.to_string());
        let schema_expr = decorated_schema(f.ty, &f.attrs);
        property_tuples.push(quote!((#prop_name.to_string(), #schema_expr)));
        let is_required = !is_option(f.ty) && f.attrs.required.unwrap_or(true);
        if is_required {
            required_names.push(quote!(#prop_name.to_string()));
        }
        let ty = f.ty;
        nested_calls.push(quote!(<#ty as ::velton::schema::ToSchema>::schemas(components);));
    }

    Ok(quote! {
        impl ::velton::schema::ToSchema for #name {
            fn schema() -> ::velton::schema::Schema {
                ::velton::schema::Schema::reference(#name_str)
            }
            fn schemas(components: &mut ::velton::schema::Components) {
                #(#nested_calls)*
                components.insert(#name_str.to_string(), {
                    ::velton::schema::Schema::object(
                        ::std::vec![#(#property_tuples),*],
                        ::std::vec![#(#required_names),*],
                    )
                });
            }
        }
    })
}

/// Generates axum `IntoResponse` and `ResponseSchema` impls when the struct
/// carries a container-level `#[schema(...)]` attribute (the response
/// marker). `status_code` and `description` are optional and default to `200`
/// and `"OK"` when absent. The generated `IntoResponse` serializes the body
/// with `Json`, so response types must derive `serde::Serialize`.
fn struct_response_impl(
    name: &syn::Ident,
    attrs: &SchemaAttr,
    is_response: bool,
) -> syn::Result<TokenStream> {
    if !is_response {
        return Ok(TokenStream::new());
    }
    let code = attrs.status_code.unwrap_or(200);
    let description = attrs
        .description
        .clone()
        .unwrap_or_else(|| "OK".to_string());
    let status = quote! {
        ::velton::axum::http::StatusCode::from_u16(#code)
            .expect("velton: invalid status code in `#[schema(status_code = ...)]`")
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

fn struct_request_schema_impl(
    name: &syn::Ident,
    fields: &[FieldSpec<'_>],
) -> syn::Result<TokenStream> {
    let mut param_code: Vec<TokenStream> = Vec::new();
    let mut body_props: Vec<TokenStream> = Vec::new();
    let mut body_required: Vec<TokenStream> = Vec::new();
    let mut has_required_body = false;

    for f in fields {
        let source = f.attrs.source.unwrap_or(Source::Body);
        let name = f
            .attrs
            .rename
            .clone()
            .unwrap_or_else(|| f.ident.to_string());
        let schema = decorated_schema(f.ty, &f.attrs);
        let description = match &f.attrs.description {
            Some(d) => quote!(Some(#d.to_string())),
            None => quote!(None),
        };
        let example = match &f.attrs.example {
            Some(lit) => {
                let value = lit_value_expr(lit);
                quote!(Some(#value))
            }
            None => quote!(None),
        };

        match source {
            Source::Query => {
                let required = !is_option(f.ty) && f.attrs.required.unwrap_or(true);
                param_code.push(quote! {
                    params.push(::velton::openapi::Parameter {
                        name: #name.to_string(),
                        r#in: ::velton::openapi::ParameterIn::Query,
                        required: #required,
                        description: #description,
                        example: #example,
                        schema: Some(#schema),
                    });
                });
            }
            Source::Path => {
                param_code.push(quote! {
                    params.push(::velton::openapi::Parameter {
                        name: #name.to_string(),
                        r#in: ::velton::openapi::ParameterIn::Path,
                        required: true,
                        description: #description,
                        example: #example,
                        schema: Some(#schema),
                    });
                });
            }
            Source::Header => {
                let header = header_name(&f.ident.to_string(), f.attrs.rename.as_deref());
                let required = !is_option(f.ty) && f.attrs.required.unwrap_or(true);
                param_code.push(quote! {
                    params.push(::velton::openapi::Parameter {
                        name: #header.to_string(),
                        r#in: ::velton::openapi::ParameterIn::Header,
                        required: #required,
                        description: #description,
                        example: #example,
                        schema: Some(#schema),
                    });
                });
            }
            Source::Body => {
                let is_required = !is_option(f.ty) && f.attrs.required.unwrap_or(true);
                body_props.push(quote!((#name.to_string(), #schema)));
                if is_required {
                    body_required.push(quote!(#name.to_string()));
                    has_required_body = true;
                }
            }
        }
    }

    let request_body = if body_props.is_empty() {
        quote!(None)
    } else {
        quote! {
            Some(::velton::openapi::RequestBody {
                required: #has_required_body,
                description: None,
                content: ::velton::openapi::Content::json(
                    ::velton::schema::Schema::object(
                        ::std::vec![#(#body_props),*],
                        ::std::vec![#(#body_required),*],
                    )
                ),
            })
        }
    };

    Ok(quote! {
        impl ::velton::extract::RequestSchema for #name {
            fn parameters() -> ::std::vec::Vec<::velton::openapi::Parameter> {
                let mut params: ::std::vec::Vec<::velton::openapi::Parameter> = ::std::vec::Vec::new();
                #(#param_code)*
                params
            }
            fn request_body() -> ::std::option::Option<::velton::openapi::RequestBody> {
                #request_body
            }
        }
    })
}

fn struct_from_request_impl(
    name: &syn::Ident,
    fields: &[FieldSpec<'_>],
) -> syn::Result<TokenStream> {
    let query_hidden = format_ident!("__velton_query_{}", name);
    let path_hidden = format_ident!("__velton_path_{}", name);
    let body_hidden = format_ident!("__velton_body_{}", name);

    let mut stmts: Vec<TokenStream> = Vec::new();
    let mut bindings: Vec<TokenStream> = Vec::new();

    let mut query_fields: Vec<&FieldSpec> = Vec::new();
    let mut path_fields: Vec<&FieldSpec> = Vec::new();
    let mut header_fields: Vec<&FieldSpec> = Vec::new();
    let mut body_fields: Vec<&FieldSpec> = Vec::new();

    for f in fields {
        match f.attrs.source.unwrap_or(Source::Body) {
            Source::Query => query_fields.push(f),
            Source::Path => path_fields.push(f),
            Source::Header => header_fields.push(f),
            Source::Body => body_fields.push(f),
        }
    }

    if !query_fields.is_empty() {
        let field_defs: Vec<TokenStream> = query_fields
            .iter()
            .map(|f| {
                let serde = extraction_rename_tokens(f);
                let ident = f.ident;
                let ty = f.ty;
                quote!(#serde #ident: #ty,)
            })
            .collect();
        let field_names: Vec<&syn::Ident> = query_fields.iter().map(|f| f.ident).collect();
        stmts.push(quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #[derive(::serde::Deserialize)]
            struct #query_hidden {
                #(#field_defs)*
            }
            let __q: #query_hidden = ::velton::serde_urlencoded::from_str(
                parts.uri.query().unwrap_or(""),
            ).map_err(|__e| ::velton::error::ExtractionRejection::Query(__e.to_string()))?;
        });
        for ident in field_names {
            bindings.push(quote!(#ident: __q.#ident));
        }
    }

    if !path_fields.is_empty() {
        let field_defs: Vec<TokenStream> = path_fields
            .iter()
            .map(|f| {
                let serde = extraction_rename_tokens(f);
                let ident = f.ident;
                let ty = f.ty;
                quote!(#serde #ident: #ty,)
            })
            .collect();
        let field_names: Vec<&syn::Ident> = path_fields.iter().map(|f| f.ident).collect();
        stmts.push(quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #[derive(::serde::Deserialize)]
            struct #path_hidden {
                #(#field_defs)*
            }
            let __p: #path_hidden = <::velton::axum::extract::Path::<#path_hidden> as ::velton::axum::extract::FromRequestParts<S>>::from_request_parts(&mut parts, state)
                .await
                .map_err(|__e| ::velton::error::ExtractionRejection::Path(__e.to_string()))?
                .0;
        });
        for ident in field_names {
            bindings.push(quote!(#ident: __p.#ident));
        }
    }

    for f in &header_fields {
        let ident = f.ident;
        let ty = f.ty;
        // `HeaderName::from_static` requires lowercase (HTTP/2 charset);
        // lookups are case-insensitive.
        let header =
            header_name(&ident.to_string(), f.attrs.rename.as_deref()).to_ascii_lowercase();
        if let Some(inner) = option_inner(ty) {
            bindings.push(quote! {
                #ident: {
                    match parts.headers.get(::velton::axum::http::header::HeaderName::from_static(#header)) {
                        Some(__v) => {
                            let __s = __v.to_str().map_err(|__e| ::velton::error::ExtractionRejection::Header(__e.to_string()))?;
                            Some(::velton::extract::parse_header_value::<#inner>(__s).map_err(::velton::error::ExtractionRejection::Header)?)
                        }
                        None => None,
                    }
                }
            });
        } else {
            bindings.push(quote! {
                #ident: {
                    match parts.headers.get(::velton::axum::http::header::HeaderName::from_static(#header)) {
                        Some(__v) => {
                            let __s = __v.to_str().map_err(|__e| ::velton::error::ExtractionRejection::Header(__e.to_string()))?;
                            ::velton::extract::parse_header_value::<#ty>(__s).map_err(::velton::error::ExtractionRejection::Header)?
                        }
                        None => return Err(::velton::error::ExtractionRejection::MissingField {
                            name: #header.to_string(),
                            source: ::velton::extract::Source::Header,
                        }),
                    }
                }
            });
        }
    }

    if !body_fields.is_empty() {
        let field_defs: Vec<TokenStream> = body_fields
            .iter()
            .map(|f| {
                let serde = extraction_rename_tokens(f);
                let ident = f.ident;
                let ty = f.ty;
                quote!(#serde #ident: #ty,)
            })
            .collect();
        let field_names: Vec<&syn::Ident> = body_fields.iter().map(|f| f.ident).collect();
        stmts.push(quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #[derive(::serde::Deserialize)]
            struct #body_hidden {
                #(#field_defs)*
            }
            let __bytes = ::velton::axum::body::to_bytes(body, ::velton::extract::body_limit()).await
                .map_err(|__e| ::velton::error::ExtractionRejection::Body(__e.to_string()))?;
            let __b: #body_hidden = ::velton::serde_json::from_slice(&__bytes)
                .map_err(|__e| ::velton::error::ExtractionRejection::Body(__e.to_string()))?;
        });
        for ident in field_names {
            bindings.push(quote!(#ident: __b.#ident));
        }
    }

    let split = if body_fields.is_empty() {
        quote!(let (mut parts, _body) = req.into_parts();)
    } else {
        quote!(let (mut parts, body) = req.into_parts();)
    };

    Ok(quote! {
        impl<S> ::velton::axum::extract::FromRequest<S> for #name
        where
            S: ::std::marker::Send + ::std::marker::Sync,
        {
            type Rejection = ::velton::error::ExtractionRejection;

            async fn from_request(
                req: ::velton::axum::extract::Request,
                state: &S,
            ) -> ::std::result::Result<Self, Self::Rejection> {
                #split
                #(#stmts)*
                Ok(#name {
                    #(#bindings),*
                })
            }
        }
    })
}

fn derive_enum(
    input: &DeriveInput,
    name: &syn::Ident,
    data: &syn::DataEnum,
) -> syn::Result<TokenStream> {
    check_no_generics(input)?;

    let variants: Vec<&syn::Ident> = data.variants.iter().map(|v| &v.ident).collect();

    for v in &data.variants {
        if !matches!(v.fields, Fields::Unit) {
            return Err(syn::Error::new_spanned(
                v,
                "velton: only unit-variant enums are supported by `#[derive(ToSchema)]`",
            ));
        }
    }

    let enum_values: Vec<TokenStream> = variants
        .iter()
        .map(|v| quote!(::velton::serde_json::json!(#v)))
        .collect();

    let schema_impl = quote! {
        impl ::velton::schema::ToSchema for #name {
            fn schema() -> ::velton::schema::Schema {
                ::velton::schema::Schema {
                    schema_type: Some(::velton::schema::SchemaType::String),
                    enum_values: Some(::std::vec![#(#enum_values),*]),
                    ..::velton::schema::Schema::default()
                }
            }
            fn schemas(_components: &mut ::velton::schema::Components) {}
        }
    };

    Ok(schema_impl)
}
