//! `#[controller("/base")]` codegen.
//!
//! Reads route (`#[get]`, `#[post]`, ...) and `#[openapi]` attributes on the
//! impl's methods, rewrites `self` receivers to `self: Arc<Self>`, and
//! generates a `Controller` impl (OpenAPI) plus an internal `__Router` impl.

use crate::attr::OpenApiAttr;
use proc_macro2::TokenStream;
use quote::quote;

#[derive(Clone, Copy)]
enum Method {
    Get,
    Post,
    Put,
    Delete,
    Patch,
    Options,
    Head,
    Any,
}

impl Method {
    fn routing_fn(&self) -> TokenStream {
        match self {
            Method::Get => quote!(::velton::axum::routing::get),
            Method::Post => quote!(::velton::axum::routing::post),
            Method::Put => quote!(::velton::axum::routing::put),
            Method::Delete => quote!(::velton::axum::routing::delete),
            Method::Patch => quote!(::velton::axum::routing::patch),
            Method::Options => quote!(::velton::axum::routing::options),
            Method::Head => quote!(::velton::axum::routing::head),
            Method::Any => quote!(::velton::axum::routing::any),
        }
    }

    fn path_item_field(&self) -> Option<syn::Ident> {
        let name = match self {
            Method::Get => "get",
            Method::Post => "post",
            Method::Put => "put",
            Method::Delete => "delete",
            Method::Patch => "patch",
            Method::Options => "options",
            Method::Head => "head",
            Method::Any => return None,
        };
        Some(syn::Ident::new(name, proc_macro2::Span::call_site()))
    }
}

struct Handler {
    method: Method,
    name: syn::Ident,
    args: Vec<(syn::Ident, syn::Type)>,
    ret: syn::ReturnType,
    openapi_path: String,
    openapi: OpenApiAttr,
}

pub fn controller(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let base: syn::LitStr = syn::parse2(attr)?;
    let base_path = base.value();

    let mut input: syn::ItemImpl = syn::parse2(item)?;
    if input.trait_.is_some() {
        return Err(syn::Error::new_spanned(
            &input.self_ty,
            "velton: `#[controller]` requires an inherent impl block",
        ));
    }
    let self_ty = input.self_ty.clone();

    let mut handlers: Vec<Handler> = Vec::new();
    for item in input.items.iter_mut() {
        let syn::ImplItem::Fn(f) = item else { continue };
        let Some((method, sub_path)) = extract_method_attr(&mut f.attrs)? else {
            continue;
        };
        let openapi = extract_openapi_attr(&mut f.attrs)?;
        rewrite_receiver(&mut f.sig)?;
        let args = handler_args(&f.sig)?;
        let ret = f.sig.output.clone();
        let full_path = join_paths(&base_path, &sub_path);
        let openapi_path = convert_path(&full_path);
        handlers.push(Handler {
            method,
            name: f.sig.ident.clone(),
            args,
            ret,
            openapi_path,
            openapi,
        });
    }

    let routes_impl = routes_impl(&self_ty, &handlers);
    let openapi_impl = openapi_impl(&self_ty, &handlers);

    Ok(quote! {
        #input
        #routes_impl
        #openapi_impl
    })
}

fn routes_impl(self_ty: &syn::Type, handlers: &[Handler]) -> TokenStream {
    let route_fns: Vec<TokenStream> = handlers
        .iter()
        .map(|h| {
            let name = &h.name;
            let routing = h.method.routing_fn();
            let route_path = &h.openapi_path;
            let arg_pats: Vec<&syn::Ident> = h.args.iter().map(|(i, _)| i).collect();
            let arg_tys: Vec<&syn::Type> = h.args.iter().map(|(_, t)| t).collect();
            let call = quote!(__velton_me.#name(#(#arg_pats),*));
            let body = match &h.ret {
                syn::ReturnType::Type(_, ty) => {
                    if is_result_type(ty) {
                        quote! {
                            match #call.await {
                                ::std::result::Result::Ok(__ok) => {
                                    ::velton::axum::response::IntoResponse::into_response(__ok)
                                }
                                ::std::result::Result::Err(__err) => {
                                    ::velton::error::handle_error(__err)
                                }
                            }
                        }
                    } else {
                        quote! {
                            ::velton::axum::response::IntoResponse::into_response(#call.await)
                        }
                    }
                }
                syn::ReturnType::Default => {
                    quote!(::velton::axum::response::IntoResponse::into_response(#call.await))
                }
            };
            quote! {
                {
                    let __velton_me = ::std::sync::Arc::clone(&me);
                    ::velton::axum::Router::new().route(
                        #route_path,
                        #routing(move |#(#arg_pats: #arg_tys),*| async move {
                            #body
                        }),
                    )
                }
            }
        })
        .collect();

    quote! {
        impl ::velton::controller::__Router for #self_ty {
            fn __router(self: ::std::sync::Arc<Self>) -> ::velton::axum::Router {
                let me = self;
                #[allow(unused_mut)]
                let mut __velton_router = ::velton::axum::Router::new();
                #( __velton_router = __velton_router.merge(#route_fns); )*
                __velton_router
            }
        }
    }
}

fn openapi_impl(self_ty: &syn::Type, handlers: &[Handler]) -> TokenStream {
    let fns: Vec<TokenStream> = handlers
        .iter()
        .map(|h| {
            let path = &h.openapi_path;
            let req_ty = h
                .openapi
                .request
                .clone()
                .or_else(|| h.args.first().map(|(_, t)| t.clone()));
            let resp_ty = success_type(&h.ret);

            let mut op_code: Vec<TokenStream> = Vec::new();
            if let Some(s) = &h.openapi.summary {
                op_code.push(quote!(__operation.summary = Some(#s.to_string());));
            }
            if let Some(d) = &h.openapi.description {
                op_code.push(quote!(__operation.description = Some(#d.to_string());));
            }
            if let Some(o) = &h.openapi.operation_id {
                op_code.push(quote!(__operation.operation_id = Some(#o.to_string());));
            }
            if !h.openapi.tags.is_empty() {
                let tags = &h.openapi.tags;
                op_code.push(quote!(__operation.tags = ::std::vec![#(#tags.to_string()),*];));
            }
            if h.openapi.deprecated {
                op_code.push(quote!(__operation.deprecated = true;));
            }

            let mut schema_calls: Vec<TokenStream> = Vec::new();
            if let Some(req) = &req_ty {
                op_code.push(
                    quote!(__operation.parameters = <#req as ::velton::extract::RequestSchema>::parameters();),
                );
                op_code.push(
                    quote!(__operation.request_body = <#req as ::velton::extract::RequestSchema>::request_body();),
                );
                schema_calls.push(quote!(<#req as ::velton::schema::ToSchema>::schemas(&mut components);));
            }

            let mut responses: Vec<TokenStream> = Vec::new();
            if let Some(resp) = &resp_ty {
                responses.push(quote! {
                    responses.insert(
                        <#resp as ::velton::response::ResponseSchema>::status().as_u16().to_string(),
                        ::velton::response::response_for::<#resp>(),
                    );
                });
                schema_calls.push(quote!(<#resp as ::velton::schema::ToSchema>::schemas(&mut components);));
            }
            for extra in &h.openapi.responses {
                responses.push(quote! {
                    responses.insert(
                        <#extra as ::velton::response::ResponseSchema>::status().as_u16().to_string(),
                        ::velton::response::response_for::<#extra>(),
                    );
                });
                schema_calls.push(quote!(<#extra as ::velton::schema::ToSchema>::schemas(&mut components);));
            }

            let field = match h.method.path_item_field() {
                Some(f) => f,
                None => return quote!(),
            };

            quote! {
                {
                    let mut __operation = ::velton::openapi::Operation::default();
                    #(#op_code)*
                    {
                        let mut responses: ::velton::indexmap::IndexMap<::std::string::String, ::velton::openapi::Response> = ::velton::indexmap::IndexMap::new();
                        #(#responses)*
                        __operation.responses = responses;
                    }
                    #(#schema_calls)*
                    let __item = ::velton::openapi::PathItem {
                        #field: Some(__operation),
                        ..::velton::openapi::PathItem::default()
                    };
                    if let Some((_, __existing)) = paths.iter_mut().find(|(__p, _)| __p == #path) {
                        __existing.merge(__item);
                    } else {
                        paths.push((#path.to_string(), __item));
                    }
                }
            }
        })
        .collect();

    quote! {
        impl ::velton::controller::Controller for #self_ty {
            fn openapi(
                &self,
            ) -> (
                ::std::vec::Vec<(::std::string::String, ::velton::openapi::PathItem)>,
                ::velton::openapi::Components,
            ) {
                let mut paths: ::std::vec::Vec<(::std::string::String, ::velton::openapi::PathItem)> = ::std::vec::Vec::new();
                let mut components = ::velton::openapi::Components::new();
                #(#fns)*
                (paths, components)
            }
        }
    }
}

fn success_type(ret: &syn::ReturnType) -> Option<syn::Type> {
    match ret {
        syn::ReturnType::Type(_, ty) => Some(result_ok_type(ty).unwrap_or_else(|| (**ty).clone())),
        syn::ReturnType::Default => None,
    }
}

fn result_ok_type(ty: &syn::Type) -> Option<syn::Type> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Result" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    match args.args.first() {
        Some(syn::GenericArgument::Type(t)) => Some(t.clone()),
        _ => None,
    }
}

fn is_result_type(ty: &syn::Type) -> bool {
    let syn::Type::Path(tp) = ty else {
        return false;
    };
    tp.path
        .segments
        .last()
        .map(|seg| seg.ident == "Result")
        .unwrap_or(false)
}

fn extract_method_attr(attrs: &mut Vec<syn::Attribute>) -> syn::Result<Option<(Method, String)>> {
    let mut found = None;
    let mut idx = None;
    for (i, attr) in attrs.iter().enumerate() {
        let Some(ident) = attr.path().get_ident() else {
            continue;
        };
        let method = match ident.to_string().as_str() {
            "get" => Some(Method::Get),
            "post" => Some(Method::Post),
            "put" => Some(Method::Put),
            "delete" => Some(Method::Delete),
            "patch" => Some(Method::Patch),
            "options" => Some(Method::Options),
            "head" => Some(Method::Head),
            "any" => Some(Method::Any),
            _ => None,
        };
        if let Some(m) = method {
            let path: syn::LitStr = attr.parse_args()?;
            found = Some((m, path.value()));
            idx = Some(i);
            break;
        }
    }
    if let Some(i) = idx {
        attrs.remove(i);
    }
    Ok(found)
}

fn extract_openapi_attr(attrs: &mut Vec<syn::Attribute>) -> syn::Result<OpenApiAttr> {
    let mut out = OpenApiAttr::default();
    let mut idx = None;
    for (i, attr) in attrs.iter().enumerate() {
        if attr.path().is_ident("openapi") {
            out = OpenApiAttr::from_attrs(std::slice::from_ref(attr))?;
            idx = Some(i);
            break;
        }
    }
    if let Some(i) = idx {
        attrs.remove(i);
    }
    Ok(out)
}

fn rewrite_receiver(sig: &mut syn::Signature) -> syn::Result<()> {
    if sig.inputs.is_empty() {
        return Err(syn::Error::new_spanned(
            &sig,
            "velton: controller handler methods must take `self`",
        ));
    }
    let first = sig.inputs.first_mut().unwrap();
    match first {
        syn::FnArg::Receiver(_) => {
            *first = syn::parse_quote!(self: ::std::sync::Arc<Self>);
            Ok(())
        }
        syn::FnArg::Typed(t) => {
            if let syn::Pat::Ident(pi) = &*t.pat
                && pi.ident == "self"
            {
                // Already typed (e.g. `self: Arc<Self>`); keep as-is.
                return Ok(());
            }
            Err(syn::Error::new_spanned(
                &t.pat,
                "velton: controller handler methods must take `self` as the first argument",
            ))
        }
    }
}

fn handler_args(sig: &syn::Signature) -> syn::Result<Vec<(syn::Ident, syn::Type)>> {
    let mut args = Vec::new();
    for input in sig.inputs.iter().skip(1) {
        let syn::FnArg::Typed(t) = input else {
            continue;
        };
        let syn::Pat::Ident(pi) = &*t.pat else {
            return Err(syn::Error::new_spanned(
                &t.pat,
                "velton: unsupported handler argument pattern",
            ));
        };
        args.push((pi.ident.clone(), (*t.ty).clone()));
    }
    Ok(args)
}

/// Converts `:param` route params to OpenAPI/axum `{param}` syntax.
fn convert_path(path: &str) -> String {
    let mut out = String::new();
    let mut chars = path.chars().peekable();
    while let Some(c) = chars.next() {
        if c == ':' {
            let mut name = String::new();
            while let Some(&n) = chars.peek() {
                if n.is_alphanumeric() || n == '_' {
                    name.push(n);
                    chars.next();
                } else {
                    break;
                }
            }
            out.push('{');
            out.push_str(&name);
            out.push('}');
        } else {
            out.push(c);
        }
    }
    out
}

/// Joins a controller base path and a method sub-path, normalizing slashes.
fn join_paths(base: &str, sub: &str) -> String {
    let base = base.trim_end_matches('/');
    let sub = sub.trim_start_matches('/');
    if base.is_empty() && sub.is_empty() {
        "/".to_string()
    } else if base.is_empty() {
        format!("/{sub}")
    } else if sub.is_empty() {
        base.to_string()
    } else {
        format!("{base}/{sub}")
    }
}
