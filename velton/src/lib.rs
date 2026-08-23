//! velton — an ergonomic REST API framework with first-class OpenAPI support,
//! built on top of [`axum`].

pub use axum;
pub use indexmap;
pub use serde_json;
pub use serde_urlencoded;

pub mod controller;
pub mod docs;
pub mod error;
pub mod extract;
pub mod middleware;
pub mod openapi;
pub mod response;
pub mod router;
pub mod schema;

pub use controller::Controller;
pub use error::Error;
pub use extract::{RequestSchema, Source};
pub use openapi::{OpenApi, Server};
pub use response::ResponseSchema;
pub use router::{Router, RouterBuilder};
pub use schema::{Schema, SchemaType, ToSchema};

// Derive macro and trait share the `ToSchema` name via different namespaces.
pub use velton_macros::{ToSchema, controller, endpoint};
