# velton

An ergonomic REST API framework for Rust with first-class **OpenAPI 3.0** support,
built on [axum](https://github.com/tokio-rs/axum).

Declare your API once — request extraction, response serialization, routing and
the OpenAPI document are all derived from your types and controllers.

```rust
use std::sync::Arc;
use velton::{controller, Cors, OpenApi, Response, Router, Schema, Server};

#[derive(Schema)]
pub struct ListUsersRequest {
    // Sources: body (default), query, path, header.
    #[schema(source = Source::Query, example = "John Doe")]
    pub name: String,
}

#[derive(Response, Schema)]
#[response(code = 201, description = "Created")]
pub struct CreateUserResponse {
    pub id: u64,
}

pub struct UserController { my_service: Arc<MyService> }

#[controller("/users")]
impl UserController {
    pub fn new(my_service: Arc<MyService>) -> Self {
        Self { my_service }
    }

    #[get("/")]
    #[openapi(
        description = "This is an example.",
        responses = (
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

## Features

- **Declarative controllers** — `#[controller("/users")]` + `#[get]`, `#[post]`,
  `#[put]`, `#[delete]`, `#[patch]`, `#[options]`, `#[head]` turn an `impl`
  block into a router.
- **One derive for requests** — `#[derive(Schema)]` on a request struct
  generates request extraction (query/path/header/body), OpenAPI parameters and
  request body, and `serde` `Serialize`/`Deserialize` impls.
- **One derive for responses** — `#[derive(Response, Schema)]` +
  `#[response(code = 201, description = "...")]` generates an axum
  `IntoResponse` and OpenAPI response metadata.
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
#[derive(Schema)]
struct GetUserRequest {
    #[schema(source = Source::Path)]
    id: u64,
}

#[controller("/users")]
impl Users {
    #[get("/:id")]
    async fn get(self, req: GetUserRequest) -> Result<UserResponse, Error> { /* ... */ }
}
```

## `#[schema]` attributes

| Attribute                  | Meaning                          |
| -------------------------- | -------------------------------- |
| `source = Source::Query`   | Request source (default body)    |
| `example = "..."`          | Example value                    |
| `description = "..."`      | Field description                |
| `rename = "..."`           | Serialized / header name         |
| `required = true/false`    | Override required (default: non-`Option` fields are required) |
| `default = <lit>`          | Default value                    |
| `format = "..."`           | Schema format (e.g. `"date-time"`) |
| `title = "..."`            | Schema title                     |
| `minimum` / `maximum`      | Numeric bounds                   |
| `min_length` / `max_length`| String length bounds             |
| `pattern = "..."`          | Regex pattern                    |
| `deprecated`               | Mark deprecated                  |

## `#[openapi]` attributes

```rust
#[openapi(
    summary = "List users",
    description = "A longer description.",
    tags = ("users", "admin"),
    operation_id = "listUsers",
    deprecated,
    responses = (BadRequestErrorResponse, InternalServerErrorResponse),
    // request = SomeOtherRequest,   // override the auto-discovered request type
)]
```

The success response is auto-discovered from the handler's return type; additional
responses are listed with `responses = (...)` (each type must derive
`Response, Schema`).

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
- `crates/velton-macros` — the procedural macros (`derive(Schema)`,
  `derive(Response)`, `#[controller]`, route and `#[openapi]` attributes).
- `crates/velton/examples/basic.rs` — runnable getting-started example.
- `crates/velton/tests/` — HTTP and OpenAPI integration tests.

## Notes / limitations (v1)

- `#[derive(Schema)]` also implements `serde::Serialize`/`Deserialize` for the
  type; don't derive serde yourself.
- Only unit-variant enums are supported by `#[derive(Schema)]`; data-carrying
  enums (`oneOf`) are not yet implemented.
- The error handler and body limit are process-global settings configured at
  `build()` time.
- Route attribute macros (`#[get]`, `#[openapi]`, …) and `Source` are consumed
  by the derive/attribute macros and do not need to be imported.
- The crate must be named `velton` (generated code refers to `::velton::...`).
