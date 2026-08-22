//! `#[derive(ToSchema)]` codegen.
//!
//! For structs this generates:
//! * `impl ToSchema` (a `$ref` plus recursive component registration),
//! * `impl RequestSchema` (OpenAPI parameters + request body),
//! * `impl FromRequest` (extraction from body/query/path/header),
//! * serde `Serialize`/`Deserialize` impls (via hidden delegation structs),
//!   unless the user already derives them, and
//! * when `#[schema(status_code = ...)]` is present, `impl IntoResponse` and
//!   `impl ResponseSchema` (response support).

use crate::attr::{SchemaAttr, Source, expr_value_expr, is_option, lit_value_expr, option_inner};
use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use std::collections::HashSet;
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

/// The names of all derive attributes on the item, if visible.
fn derive_names(input: &DeriveInput) -> HashSet<String> {
    let mut names = HashSet::new();
    for attr in &input.attrs {
        if !attr.path().is_ident("derive") {
            continue;
        }
        if let Ok(list) = attr.parse_args_with(
            syn::punctuated::Punctuated::<syn::Path, syn::Token![,]>::parse_terminated,
        ) {
            for path in list {
                if let Some(seg) = path.segments.last() {
                    names.insert(seg.ident.to_string());
                }
            }
        }
    }
    names
}

/// Fields of a struct with their parsed `#[schema]` attrs.
struct FieldSpec<'a> {
    ident: &'a syn::Ident,
    ty: &'a syn::Type,
    attrs: SchemaAttr,
    serde: Vec<&'a syn::Attribute>,
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
            serde: field
                .attrs
                .iter()
                .filter(|a| a.path().is_ident("serde"))
                .collect(),
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

fn container_serde_attrs(input: &DeriveInput) -> Vec<&syn::Attribute> {
    input
        .attrs
        .iter()
        .filter(|a| a.path().is_ident("serde"))
        .collect()
}

/// Serde tokens for a field: `#[schema(rename)]` wins, else the user's serde attrs.
fn field_serde_tokens(field: &FieldSpec<'_>) -> TokenStream {
    if let Some(r) = &field.attrs.rename {
        quote!(#[serde(rename = #r)])
    } else {
        let serde = &field.serde;
        quote!(#(#serde)*)
    }
}

/// `#[serde(rename = "...")]` tokens for extraction hidden structs (schema rename only).
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

    let serde_impls = serde_struct_impls(input, name, &fields)?;
    let schema_impl = struct_to_schema_impl(name, &fields)?;
    let request_impl = struct_request_schema_impl(name, &fields)?;
    let from_request_impl = struct_from_request_impl(name, &fields)?;
    let response_impl = struct_response_impl(name, &container)?;

    Ok(quote! {
        #serde_impls
        #schema_impl
        #request_impl
        #from_request_impl
        #response_impl
    })
}

fn serde_struct_impls(
    input: &DeriveInput,
    name: &syn::Ident,
    fields: &[FieldSpec<'_>],
) -> syn::Result<TokenStream> {
    let derived = derive_names(input);
    let has_ser = derived.contains("Serialize");
    let has_de = derived.contains("Deserialize");

    if has_ser && has_de {
        return Ok(TokenStream::new());
    }

    let container = container_serde_attrs(input);
    let serde_hidden = format_ident!("__velton_serde_{}", name);
    let de_hidden = format_ident!("__velton_de_{}", name);

    let ser = if has_ser {
        TokenStream::new()
    } else {
        let field_defs: Vec<TokenStream> = fields
            .iter()
            .map(|f| {
                let serde = field_serde_tokens(f);
                let ident = f.ident;
                let ty = f.ty;
                quote!(#serde #ident: &'a #ty,)
            })
            .collect();
        let field_inits: Vec<TokenStream> = fields
            .iter()
            .map(|f| {
                let ident = f.ident;
                quote!(#ident: &self.#ident,)
            })
            .collect();
        quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #[derive(::velton::serde::Serialize)]
            #[serde(#(#container),*)]
            struct #serde_hidden<'a> {
                #(#field_defs)*
            }
            impl ::velton::serde::Serialize for #name {
                fn serialize<S: ::velton::serde::Serializer>(
                    &self,
                    serializer: S,
                ) -> ::std::result::Result<S::Ok, S::Error> {
                    ::velton::serde::Serialize::serialize(
                        &#serde_hidden {
                            #(#field_inits)*
                        },
                        serializer,
                    )
                }
            }
        }
    };

    let de = if has_de {
        TokenStream::new()
    } else {
        let field_defs: Vec<TokenStream> = fields
            .iter()
            .map(|f| {
                let serde = field_serde_tokens(f);
                let ident = f.ident;
                let ty = f.ty;
                quote!(#serde #ident: #ty,)
            })
            .collect();
        let field_inits: Vec<TokenStream> = fields
            .iter()
            .map(|f| {
                let ident = f.ident;
                quote!(#ident: __v.#ident,)
            })
            .collect();
        quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #[derive(::velton::serde::Deserialize)]
            #[serde(#(#container),*)]
            struct #de_hidden {
                #(#field_defs)*
            }
            impl<'de> ::velton::serde::Deserialize<'de> for #name {
                fn deserialize<D: ::velton::serde::Deserializer<'de>>(
                    deserializer: D,
                ) -> ::std::result::Result<Self, D::Error> {
                    ::velton::serde::Deserialize::deserialize(deserializer)
                        .map(|__v: #de_hidden| #name {
                            #(#field_inits)*
                        })
                }
            }
        }
    };

    Ok(quote! {
        #ser
        #de
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

/// Generates axum `IntoResponse` and `ResponseSchema` impls when the container
/// has `#[schema(status_code = ...)]`; otherwise it is a no-op.
fn struct_response_impl(name: &syn::Ident, attrs: &SchemaAttr) -> syn::Result<TokenStream> {
    let Some(code) = attrs.status_code else {
        return Ok(TokenStream::new());
    };
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
            #[derive(::velton::serde::Deserialize)]
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
            #[derive(::velton::serde::Deserialize)]
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
            #[derive(::velton::serde::Deserialize)]
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

    let serde_hidden = format_ident!("__velton_serde_{}", name);

    // serde delegation for unit enums (unless already derived).
    let derived = derive_names(input);
    let has_ser = derived.contains("Serialize");
    let has_de = derived.contains("Deserialize");
    let serde_impls = if has_ser && has_de {
        TokenStream::new()
    } else {
        let container = container_serde_attrs(input);
        let serde_attrs = if has_ser {
            quote!()
        } else {
            quote!(#[derive(::velton::serde::Serialize)])
        };
        let de_attrs = if has_de {
            quote!()
        } else {
            quote!(#[derive(::velton::serde::Deserialize)])
        };
        let ser_map: Vec<TokenStream> = data
            .variants
            .iter()
            .map(|v| {
                let v = &v.ident;
                quote!(#name::#v => #serde_hidden::#v)
            })
            .collect();
        let de_map: Vec<TokenStream> = data
            .variants
            .iter()
            .map(|v| {
                let v = &v.ident;
                quote!(#serde_hidden::#v => #name::#v)
            })
            .collect();
        let ser_impl = if has_ser {
            TokenStream::new()
        } else {
            quote! {
                impl ::velton::serde::Serialize for #name {
                    fn serialize<S: ::velton::serde::Serializer>(
                        &self,
                        serializer: S,
                    ) -> ::std::result::Result<S::Ok, S::Error> {
                        let __v = match self {
                            #(#ser_map),*
                        };
                        ::velton::serde::Serialize::serialize(&__v, serializer)
                    }
                }
            }
        };
        let de_impl = if has_de {
            TokenStream::new()
        } else {
            quote! {
                impl<'de> ::velton::serde::Deserialize<'de> for #name {
                    fn deserialize<D: ::velton::serde::Deserializer<'de>>(
                        deserializer: D,
                    ) -> ::std::result::Result<Self, D::Error> {
                        let __v = ::velton::serde::Deserialize::deserialize(deserializer)?;
                        ::std::result::Result::Ok(match __v {
                            #(#de_map),*
                        })
                    }
                }
            }
        };
        quote! {
            #[doc(hidden)]
            #[allow(non_camel_case_types, dead_code)]
            #serde_attrs
            #de_attrs
            #[serde(#(#container),*)]
            enum #serde_hidden {
                #(#variants,)*
            }
            #ser_impl
            #de_impl
        }
    };

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

    Ok(quote! {
        #serde_impls
        #schema_impl
    })
}
