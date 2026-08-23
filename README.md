# velton

An ergonomic REST API framework for Rust with first-class **OpenAPI 3.0** support,
built on [axum](https://github.com/tokio-rs/axum).

Declare your API once — request extraction, response serialization, routing and
the OpenAPI document are all derived from your types and controllers.

```rust
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use velton::{controller, Cors, OpenApi, Router, Server, ToSchema};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ListUsersRequest {
    // Sources: body (default), query, path, header.
    #[schema(source = Source::Query, example = "John Doe")]
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 201, description = "Created")]
pub struct CreateUserResponse {
    pub id: u64,
}

pub struct UserController { my_service: Arc<MyService> }

#[controller]
impl UserController {
    pub fn new(my_service: Arc<MyService>) -> Self {
        Self { my_service }
    }

    #[endpoint(
        method = get,
        path = "/users",
        description = "This is an example.",
        error_responses = (
            BadRequestErrorResponse,
            UnauthorizedErrorResponse,
            InternalServerErrorResponse,
        ),
    )]
    async fn list(self, req: ListUsersRequest) -> Result<ListUsersResponse, Error> {
        Ok(ListUsersResponse { message: format!("Hello, {}!", req.name) })
    }
}

let openapi = OpenApi::builder()
    .name("my-app")
    .version("0.1.0")
    .description("Lorem ipsum sit amet dolor.")
    .server(Server::builder().url("http://localhost:3000").build())
    .build();

let router = Router::builder()
    .openapi(openapi)
    .controller(UserController::new(Arc::new(MyService)))
    .middleware(Cors::permissive())
    .build()?;

router.run().await?;
```

See `crates/velton/examples/basic.rs` for a fully runnable version.

## Serde is up to you

`#[derive(ToSchema)]` does **not** implement `serde::Serialize`/`Deserialize`
for you. Add `serde` (with the `derive` feature) to your own dependencies and
derive both traits alongside `ToSchema` on every type used as a request or
response:

```toml
[dependencies]
serde = { version = "1", features = ["derive"] }
```

```rust
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub name: String,
}
```

Response bodies are serialized through your `Serialize` impl; request
bodies/query/path parameters are deserialized through your `Deserialize` impl
(via `serde_json` and `serde_urlencoded`). velton still uses serde internally,
but it is no longer bundled as part of your public types.

## Features

- **Declarative controllers** — `#[controller]` + `#[endpoint(method = ...,
path = ...)]` turn an `impl` block into a router. Each endpoint declares its
  full HTTP method and path.
- **One derive for requests** — `#[derive(ToSchema)]` on a request struct
  generates request extraction (query/path/header/body) plus the OpenAPI
  parameters and request body. (You derive `serde::Serialize`/`Deserialize`
  yourself; see [Serde is up to you](#serde-is-up-to-you).)
- **One derive for responses** — `#[derive(ToSchema)]` +
  `#[schema(status_code = 201, description = "...")]` generates an axum
  `IntoResponse` and OpenAPI response metadata, serialized via your `Serialize`
  impl.
- **OpenAPI 3.0 document** — assembled automatically from your controllers,
  served at `/openapi.json`, with an interactive **Scalar** UI at `/docs`.
- **Error-agnostic handlers** — return `Result<T, YourError>`; a default error
  handler maps any `std::error::Error` to `500 { "message": ... }`, overridable
  via `RouterBuilder::error_handler`.
- **Middleware** — any tower layer, e.g. `.middleware(Cors::permissive())`.

## Request sources

Each field of a request struct declares where its value comes from:

| `#[schema(source = ...)]` | Location           |
| ------------------------- | ------------------ |
| `Source::Body` (default)  | JSON request body  |
| `Source::Query`           | URL query string   |
| `Source::Path`            | URL path parameter |
| `Source::Header`          | HTTP header        |

Headers follow the convention `my_field` → `X-My-Field` (e.g. `api_key` →
`X-Api-Key`), overridable with `#[schema(rename = "X-Custom")]`.

Path parameters in routes use `:name` (or `{name}`) and are matched to `Source::Path`
fields by name:

```rust
#[derive(Serialize, Deserialize, ToSchema)]
struct GetUserRequest {
    #[schema(source = Source::Path)]
    id: u64,
}

#[controller]
impl Users {
    #[endpoint(method = get, path = "/users/:id")]
    async fn get(self, req: GetUserRequest) -> Result<UserResponse, Error> { /* ... */ }
}
```

## `#[schema]` attributes

| Attribute                   | Meaning                                                       |
| --------------------------- | ------------------------------------------------------------- |
| `source = Source::Query`    | Request source (default body)                                 |
| `example = "..."`           | Example value                                                 |
| `description = "..."`       | Field / response description                                  |
| `status_code = 201`         | Response status code (container)                              |
| `rename = "..."`            | OpenAPI name / header name (also the request wire name)       |
| `required = true/false`     | Override required (default: non-`Option` fields are required) |
| `default = <lit>`           | Default value                                                 |
| `format = "..."`            | Schema format (e.g. `"date-time"`)                            |
| `title = "..."`             | Schema title                                                  |
| `minimum` / `maximum`       | Numeric bounds                                                |
| `min_length` / `max_length` | String length bounds                                          |
| `pattern = "..."`           | Regex pattern                                                 |
| `deprecated`                | Mark deprecated                                               |

## `#[endpoint]` attributes

Each handler method is annotated with a single `#[endpoint(...)]`:

```rust
#[endpoint(
    method = get,                                    // required: get/post/put/delete/patch/options/head/any
    path = "/users",                                // required: the full path
    description = "A longer description.",
    error_responses = (BadRequestErrorResponse, InternalServerErrorResponse),
)]
async fn list(self, req: ListUsersRequest) -> Result<ListUsersResponse, Error> { ... }
```

The OpenAPI `operationId` is derived automatically from the handler function
name. The success response is auto-discovered from the handler's return type;
additional responses are listed with `error_responses = (...)` (each type
must derive `ToSchema` with a `#[schema(status_code = ...)]`).

## Error handling

Handlers may return any `std::error::Error`:

```rust
async fn get(self, req: GetUserRequest) -> Result<UserResponse, MyError> { ... }
```

The default error handler returns `500` with `{ "message": "<error>" }`. To map
errors yourself, register a handler on the router:

```rust
use velton::error::{ErrorHandler, ExtractionRejection};

fn my_error_handler(err: &(dyn std::error::Error + Send + Sync)) -> axum::response::Response {
    axum::response::IntoResponse::into_response((
        axum::http::StatusCode::BAD_REQUEST,
        axum::Json(serde_json::json!({ "error": err.to_string() })),
    ))
}

Router::builder().error_handler(my_error_handler).build()?;
```

Extraction failures (missing required fields, invalid JSON, …) return `400` with
`{ "message": "..." }`.

## Serving

```rust
let router = Router::builder()
    .openapi(openapi)
    .controller(users)
    .middleware(Cors::permissive())
    .bind("127.0.0.1:8080")   // defaults to 127.0.0.1:3000
    .body_limit(4 * 1024 * 1024)
    .build()?;

router.run().await?; // serves routes + /openapi.json + /docs
```

## Custom types

Implement `velton::schema::ToSchema` for types without a derive (third-party
types like UUIDs, dates, …):

```rust
use velton::schema::{Components, Schema, SchemaType, ToSchema};

impl ToSchema for uuid::Uuid {
    fn schema() -> Schema {
        Schema {
            schema_type: Some(SchemaType::String),
            format: Some("uuid".to_string()),
            ..Default::default()
        }
    }
}
```

## Workspace layout

- `crates/velton` — the framework runtime.
- `crates/velton-macros` — the procedural macros (`derive(ToSchema)`,
  `#[controller]` and `#[endpoint]` attributes).
- `crates/velton/examples/basic.rs` — runnable getting-started example.
- `crates/velton/tests/` — HTTP and OpenAPI integration tests.

## Notes / limitations (v1)

- `#[derive(ToSchema)]` does not implement `serde::Serialize`/`Deserialize`;
  install `serde` yourself and derive both traits on every request/response
  type (see [Serde is up to you](#serde-is-up-to-you)).
- Only unit-variant enums are supported by `#[derive(ToSchema)]`; data-carrying
  enums (`oneOf`) are not yet implemented.
- The error handler and body limit are process-global settings configured at
  `build()` time.
- `#[endpoint]` and `Source` are consumed by the attribute macros and do not
  need to be imported.
- The crate must be named `velton` (generated code refers to `::velton::...`).
