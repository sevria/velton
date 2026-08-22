//! Request extraction: the [`Source`] enum, the [`RequestSchema`] trait and
//! helpers used by generated extraction code.

use crate::openapi::{Parameter, RequestBody};
use serde::de::DeserializeOwned;
use std::sync::atomic::{AtomicUsize, Ordering};

/// The HTTP source a request field is extracted from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Body,
    Query,
    Path,
    Header,
}

impl Source {
    pub fn label(self) -> &'static str {
        match self {
            Source::Body => "body",
            Source::Query => "query",
            Source::Path => "path",
            Source::Header => "header",
        }
    }
}

/// Implemented by request structs (via `#[derive(ToSchema)]`).
///
/// Produces the OpenAPI parameters and request body for an operation.
pub trait RequestSchema {
    /// The query/path/header parameters declared by this request.
    fn parameters() -> Vec<Parameter>;

    /// The JSON request body, if this request has any body fields.
    fn request_body() -> Option<RequestBody>;
}

const DEFAULT_BODY_LIMIT: usize = 2 * 1024 * 1024;

static BODY_LIMIT: AtomicUsize = AtomicUsize::new(DEFAULT_BODY_LIMIT);

/// The maximum request body size, in bytes. Defaults to 2 MiB.
pub fn body_limit() -> usize {
    BODY_LIMIT.load(Ordering::Relaxed)
}

/// Sets the maximum request body size (global for the process).
pub fn set_body_limit(bytes: usize) {
    BODY_LIMIT.store(bytes, Ordering::Relaxed);
}

/// Parses a header value into `T`.
///
/// First tries the value as plain JSON (numbers, booleans), then falls back to
/// treating it as a JSON string (enums, UUIDs, plain strings).
pub fn parse_header_value<T: DeserializeOwned>(value: &str) -> Result<T, String> {
    if let Ok(v) = serde_json::from_str::<T>(value) {
        return Ok(v);
    }
    let escaped = value.replace('\\', "\\\\").replace('"', "\\\"");
    serde_json::from_str::<T>(&format!("\"{escaped}\"")).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_plain_string() {
        assert_eq!(parse_header_value::<String>("John").unwrap(), "John");
    }

    #[test]
    fn header_number() {
        assert_eq!(parse_header_value::<u64>("123").unwrap(), 123);
    }

    #[test]
    fn header_bool() {
        assert!(parse_header_value::<bool>("true").unwrap());
    }
}
