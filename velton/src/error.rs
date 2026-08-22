//! Framework errors, extraction rejections and the default error handler.

use crate::extract::Source;
use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::sync::OnceLock;

/// Errors returned by the framework.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("failed to bind to {addr}: {source}")]
    Bind {
        addr: std::net::SocketAddr,
        source: std::io::Error,
    },
    #[error("http server error: {0}")]
    Serve(Box<dyn std::error::Error + Send + Sync>),
    #[error("openapi error: {0}")]
    OpenApi(String),
}

/// A rejection produced by request extraction (mapped to HTTP 400).
#[derive(Debug)]
pub enum ExtractionRejection {
    MissingField { name: String, source: Source },
    Query(String),
    Path(String),
    Header(String),
    Body(String),
}

impl IntoResponse for ExtractionRejection {
    fn into_response(self) -> Response {
        let message = match &self {
            ExtractionRejection::MissingField { name, source } => {
                format!("missing required {} field `{name}`", source.label())
            }
            ExtractionRejection::Query(msg) => format!("invalid query: {msg}"),
            ExtractionRejection::Path(msg) => format!("invalid path: {msg}"),
            ExtractionRejection::Header(msg) => format!("invalid header: {msg}"),
            ExtractionRejection::Body(msg) => format!("invalid request body: {msg}"),
        };
        (StatusCode::BAD_REQUEST, Json(json!({ "message": message }))).into_response()
    }
}

/// A function that converts a handler error into an HTTP response.
pub type ErrorHandler = fn(&(dyn std::error::Error + Send + Sync)) -> Response;

static ERROR_HANDLER: OnceLock<ErrorHandler> = OnceLock::new();

/// Sets the global error handler. Returns an error if one is already set.
#[allow(clippy::result_unit_err)]
pub fn set_error_handler(handler: ErrorHandler) -> Result<(), ()> {
    ERROR_HANDLER.set(handler).map_err(|_| ())
}

/// Converts a handler error into a response using the configured handler, or a
/// default 500 JSON response.
pub fn handle_error<E>(err: E) -> Response
where
    E: std::error::Error + Send + Sync + 'static,
{
    if let Some(handler) = ERROR_HANDLER.get() {
        return handler(&err);
    }
    (
        StatusCode::INTERNAL_SERVER_ERROR,
        Json(json!({ "message": err.to_string() })),
    )
        .into_response()
}
