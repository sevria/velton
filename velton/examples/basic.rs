//! A runnable version of the velton getting-started example.

use std::sync::Arc;

use velton::{Cors, OpenApi, Response, Router, Schema, Server, controller};

// `#[get]`, `#[post]`, `#[openapi]` and `Source` are only referenced inside
// attributes consumed by `#[controller]`/`#[derive(Schema)]`, so they do not
// need to be imported.

// --- Requests -----------------------------------------------------------------

#[derive(Schema)]
pub struct ListUsersRequest {
    #[schema(source = Source::Query, example = "John Doe")]
    pub name: String,
}

#[derive(Schema)]
pub struct GetUserRequest {
    #[schema(source = Source::Path, example = 42)]
    pub id: u64,
}

#[derive(Schema)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
}

// --- Responses ----------------------------------------------------------------

#[derive(Response, Schema)]
pub struct ListUsersResponse {
    #[schema(example = "User fetched successfully")]
    pub message: String,
}

#[derive(Response, Schema)]
#[response(code = 201, description = "User created")]
pub struct CreateUserResponse {
    pub id: u64,
}

#[derive(Response, Schema)]
#[response(code = 400)]
pub struct BadRequestErrorResponse {
    pub message: String,
}

#[derive(Response, Schema)]
#[response(code = 401)]
pub struct UnauthorizedErrorResponse {
    pub message: String,
}

#[derive(Response, Schema)]
#[response(code = 500)]
pub struct InternalServerErrorResponse {
    pub message: String,
}

// --- App error (any `std::error::Error` works) --------------------------------

#[derive(Debug)]
pub struct AppError;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("app error")
    }
}

impl std::error::Error for AppError {}

// --- Controller ----------------------------------------------------------------

pub struct MyService;

pub struct UserController {
    my_service: Arc<MyService>,
}

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
    async fn list(self, req: ListUsersRequest) -> Result<ListUsersResponse, AppError> {
        let _ = &self.my_service;
        Ok(ListUsersResponse {
            message: format!("Hello, {}!", req.name),
        })
    }

    #[get("/:id")]
    async fn get(self, req: GetUserRequest) -> Result<ListUsersResponse, AppError> {
        let _ = &self.my_service;
        Ok(ListUsersResponse {
            message: format!("user #{}", req.id),
        })
    }

    #[post("/")]
    async fn create(self, req: CreateUserRequest) -> Result<CreateUserResponse, AppError> {
        let _ = &self.my_service;
        let _ = &req;
        Ok(CreateUserResponse { id: 1 })
    }
}

// --- Main ----------------------------------------------------------------------

#[tokio::main]
async fn main() -> Result<(), velton::Error> {
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

    router.run().await
}
