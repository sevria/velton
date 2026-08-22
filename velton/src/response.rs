//! Response support.

use crate::openapi::{Content, Response};
use crate::schema::{Components, ToSchema};
use axum::http::StatusCode;

/// Implemented by response types (via `#[derive(ToSchema)]` with
/// `#[schema(status_code = ...)]`).
pub trait ResponseSchema: ToSchema {
    /// The HTTP status code returned by this response.
    fn status() -> StatusCode;

    /// The response description.
    fn description() -> &'static str;
}

/// Builds the OpenAPI [`Response`] descriptor for a [`ResponseSchema`] type.
pub fn response_for<T: ResponseSchema>() -> Response {
    Response {
        description: T::description().to_string(),
        content: Some(Content::json(T::schema())),
    }
}

/// Registers a response type and its nested schemas into `components`.
pub fn register_response<T: ResponseSchema>(components: &mut Components) {
    T::schemas(components);
}
