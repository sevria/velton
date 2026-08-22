//! Parsing helpers for `#[schema]` and `#[endpoint]` attributes.

use proc_macro2::TokenStream;
use quote::quote;
use syn::Path;

/// The source of a request field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Body,
    Query,
    Path,
    Header,
}

/// Parsed `#[schema(...)]` attribute (on a field or on the container).
#[derive(Debug, Clone, Default)]
pub struct SchemaAttr {
    pub source: Option<Source>,
    pub example: Option<syn::Lit>,
    pub description: Option<String>,
    pub status_code: Option<u16>,
    pub rename: Option<String>,
    pub required: Option<bool>,
    pub default: Option<syn::Expr>,
    pub format: Option<String>,
    pub deprecated: bool,
    pub minimum: Option<f64>,
    pub maximum: Option<f64>,
    pub min_length: Option<usize>,
    pub max_length: Option<usize>,
    pub pattern: Option<String>,
    pub title: Option<String>,
}

fn parse_lit_f64(lit: &syn::Lit, what: &str) -> syn::Result<f64> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse(),
        syn::Lit::Float(f) => f.base10_parse(),
        other => Err(syn::Error::new_spanned(
            other,
            format!("velton: `{what}` must be a number"),
        )),
    }
}

fn parse_lit_usize(lit: &syn::Lit, what: &str) -> syn::Result<usize> {
    match lit {
        syn::Lit::Int(i) => i.base10_parse(),
        other => Err(syn::Error::new_spanned(
            other,
            format!("velton: `{what}` must be an integer"),
        )),
    }
}

impl SchemaAttr {
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut out = SchemaAttr::default();
        for attr in attrs {
            if !attr.path().is_ident("schema") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("source") {
                    let p: Path = meta.value()?.parse()?;
                    let ident = p
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    out.source = Some(match ident.as_str() {
                        "Body" => Source::Body,
                        "Query" => Source::Query,
                        "Path" => Source::Path,
                        "Header" => Source::Header,
                        other => {
                            return Err(meta.error(format!(
                                "velton: unknown source `{other}`, expected `Source::Body`, `Source::Query`, `Source::Path` or `Source::Header`"
                            )));
                        }
                    });
                } else if meta.path.is_ident("example") {
                    out.example = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("description") {
                    out.description = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("status_code") {
                    let lit: syn::LitInt = meta.value()?.parse()?;
                    out.status_code = Some(lit.base10_parse()?);
                } else if meta.path.is_ident("rename") {
                    out.rename = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("required") {
                    let v: syn::LitBool = meta.value()?.parse()?;
                    out.required = Some(v.value);
                } else if meta.path.is_ident("default") {
                    out.default = Some(meta.value()?.parse()?);
                } else if meta.path.is_ident("format") {
                    out.format = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("deprecated") {
                    if meta.input.peek(syn::Token![=]) {
                        let v: syn::LitBool = meta.value()?.parse()?;
                        out.deprecated = v.value;
                    } else {
                        out.deprecated = true;
                    }
                } else if meta.path.is_ident("minimum") {
                    let lit: syn::Lit = meta.value()?.parse()?;
                    out.minimum = Some(parse_lit_f64(&lit, "minimum")?);
                } else if meta.path.is_ident("maximum") {
                    let lit: syn::Lit = meta.value()?.parse()?;
                    out.maximum = Some(parse_lit_f64(&lit, "maximum")?);
                } else if meta.path.is_ident("min_length") {
                    let lit: syn::Lit = meta.value()?.parse()?;
                    out.min_length = Some(parse_lit_usize(&lit, "min_length")?);
                } else if meta.path.is_ident("max_length") {
                    let lit: syn::Lit = meta.value()?.parse()?;
                    out.max_length = Some(parse_lit_usize(&lit, "max_length")?);
                } else if meta.path.is_ident("pattern") {
                    out.pattern = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("title") {
                    out.title = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else {
                    return Err(meta.error(format!(
                        "velton: unknown `#[schema]` attribute `{}`",
                        meta.path.get_ident().map(|i| i.to_string()).unwrap_or_default()
                    )));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}

/// Produces an expression evaluating to a `serde_json::Value` for an example/default literal.
pub fn lit_value_expr(lit: &syn::Lit) -> TokenStream {
    quote!(::velton::serde_json::json!(#lit))
}

/// Produces an expression evaluating to a `serde_json::Value` for a `default` expression.
pub fn expr_value_expr(expr: &syn::Expr) -> TokenStream {
    quote!(::velton::serde_json::json!(#expr))
}

/// True if the type is `Option<...>`.
pub fn is_option(ty: &syn::Type) -> bool {
    option_inner(ty).is_some()
}

/// Returns the inner type of `Option<T>`, if any.
pub fn option_inner(ty: &syn::Type) -> Option<&syn::Type> {
    let syn::Type::Path(tp) = ty else {
        return None;
    };
    let seg = tp.path.segments.last()?;
    if seg.ident != "Option" {
        return None;
    }
    let syn::PathArguments::AngleBracketed(args) = &seg.arguments else {
        return None;
    };
    match args.args.first() {
        Some(syn::GenericArgument::Type(t)) => Some(t),
        _ => None,
    }
}

/// Parsed `#[endpoint(...)]` attribute on a controller handler method.
#[derive(Debug, Clone, Default)]
pub struct EndpointAttr {
    pub method: Option<String>,
    pub path: Option<String>,
    pub description: Option<String>,
    pub error_responses: Vec<syn::Type>,
}

impl EndpointAttr {
    pub fn from_attrs(attrs: &[syn::Attribute]) -> syn::Result<Self> {
        let mut out = EndpointAttr::default();
        for attr in attrs {
            if !attr.path().is_ident("endpoint") {
                continue;
            }
            attr.parse_nested_meta(|meta| {
                if meta.path.is_ident("method") {
                    let p: Path = meta.value()?.parse()?;
                    let ident = p
                        .segments
                        .last()
                        .map(|s| s.ident.to_string())
                        .unwrap_or_default();
                    out.method = Some(ident);
                } else if meta.path.is_ident("path") {
                    out.path = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("description") {
                    out.description = Some(meta.value()?.parse::<syn::LitStr>()?.value());
                } else if meta.path.is_ident("error_responses") {
                    let stream = meta.value()?;
                    let content;
                    syn::parenthesized!(content in stream);
                    let list = content.parse_terminated(
                        <syn::Type as syn::parse::Parse>::parse,
                        syn::Token![,],
                    )?;
                    out.error_responses = list.into_iter().collect();
                } else {
                    return Err(meta.error(format!(
                        "velton: unknown `#[endpoint]` attribute `{}`",
                        meta.path
                            .get_ident()
                            .map(|i| i.to_string())
                            .unwrap_or_default()
                    )));
                }
                Ok(())
            })?;
        }
        Ok(out)
    }
}
