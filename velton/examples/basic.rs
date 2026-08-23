//! A runnable version of the velton getting-started example.

use serde::{Deserialize, Serialize};
use std::sync::Arc;

use velton::{OpenApi, Router, Server, ToSchema, controller, middleware::Cors};

// `#[endpoint]` and `Source` are only referenced inside attributes consumed by
// `#[controller]`/`#[derive(ToSchema)]`, so they do not need to be imported.

// --- Requests -----------------------------------------------------------------

#[derive(Serialize, Deserialize, ToSchema)]
pub struct ListUsersRequest {
    #[schema(source = Source::Query, example = "John Doe")]
    pub name: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct GetUserRequest {
    #[schema(source = Source::Path, example = 42)]
    pub id: u64,
}

#[derive(Serialize, Deserialize, ToSchema)]
pub struct CreateUserRequest {
    pub name: String,
    pub email: String,
}

// --- Responses ----------------------------------------------------------------

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 200)]
pub struct ListUsersResponse {
    #[schema(example = "User fetched successfully")]
    pub message: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 201, description = "User created")]
pub struct CreateUserResponse {
    pub id: u64,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 400)]
pub struct BadRequestErrorResponse {
    pub message: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 401)]
pub struct UnauthorizedErrorResponse {
    pub message: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 500)]
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
    async fn list(self, req: ListUsersRequest) -> Result<ListUsersResponse, AppError> {
        let _ = &self.my_service;
        Ok(ListUsersResponse {
            message: format!("Hello, {}!", req.name),
        })
    }

    #[endpoint(method = get, path = "/users/:id")]
    async fn get(self, req: GetUserRequest) -> Result<ListUsersResponse, AppError> {
        let _ = &self.my_service;
        Ok(ListUsersResponse {
            message: format!("user #{}", req.id),
        })
    }

    #[endpoint(method = post, path = "/users")]
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
