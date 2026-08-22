//! OpenAPI document structure tests.

use serde_json::Value;
use velton::{OpenApi, Router, Server, ToSchema, controller};

#[derive(ToSchema)]
struct ListRequest {
    #[schema(source = Source::Query, example = "John Doe", description = "filter by name")]
    name: String,
    #[schema(source = Source::Query)]
    page: Option<u32>,
}

#[derive(ToSchema)]
struct GetRequest {
    #[schema(source = Source::Path)]
    id: u64,
}

#[derive(ToSchema)]
struct CreateRequest {
    name: String,
    email: String,
}

#[derive(ToSchema)]
struct Nested {
    count: u32,
}

#[derive(ToSchema)]
#[schema(status_code = 201, description = "User created")]
struct UserResponse {
    id: u64,
    nested: Nested,
}

#[derive(ToSchema)]
#[schema(status_code = 400)]
struct BadRequest {
    message: String,
}

#[derive(Debug)]
struct AppError;
impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("err")
    }
}
impl std::error::Error for AppError {}

struct UsersController;

#[controller]
impl UsersController {
    #[endpoint(
        method = get,
        path = "/users",
        description = "list users",
        error_responses = (BadRequest,),
    )]
    async fn list(self, req: ListRequest) -> Result<UserResponse, AppError> {
        let _ = &req;
        Ok(UserResponse {
            id: 1,
            nested: Nested { count: 1 },
        })
    }

    #[endpoint(method = get, path = "/users/:id")]
    async fn get(self, req: GetRequest) -> Result<UserResponse, AppError> {
        Ok(UserResponse {
            id: req.id,
            nested: Nested { count: 1 },
        })
    }

    #[endpoint(method = post, path = "/users")]
    async fn create(self, req: CreateRequest) -> Result<UserResponse, AppError> {
        let _ = &req;
        Ok(UserResponse {
            id: 2,
            nested: Nested { count: 1 },
        })
    }
}

fn doc() -> Value {
    let openapi = OpenApi::builder()
        .name("users-api")
        .version("0.1.0")
        .description("users api")
        .server(Server::builder().url("http://localhost:3000").build())
        .build();

    let router = Router::builder()
        .openapi(openapi)
        .controller(UsersController)
        .build()
        .unwrap();

    serde_json::to_value(router.openapi_doc()).unwrap()
}

#[test]
fn document_header() {
    let doc = doc();
    assert_eq!(doc["openapi"], "3.0.3");
    assert_eq!(doc["info"]["title"], "users-api");
    assert_eq!(doc["info"]["version"], "0.1.0");
    assert_eq!(doc["servers"][0]["url"], "http://localhost:3000");
}

#[test]
fn paths_are_merged_by_method() {
    let doc = doc();
    let users = &doc["paths"]["/users"];
    assert!(users["get"].is_object());
    assert!(users["post"].is_object());
    let users_id = &doc["paths"]["/users/{id}"];
    assert!(users_id["get"].is_object());
}

#[test]
fn query_parameters_are_documented() {
    let doc = doc();
    let params = &doc["paths"]["/users"]["get"]["parameters"];
    assert_eq!(params.as_array().unwrap().len(), 2);

    let name = params
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "name")
        .expect("name parameter");
    assert_eq!(name["in"], "query");
    assert_eq!(name["required"], true);
    assert_eq!(name["example"], "John Doe");
    assert_eq!(name["description"], "filter by name");
    assert_eq!(name["schema"]["type"], "string");

    let page = params
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "page")
        .expect("page parameter");
    // `required: false` is omitted (OpenAPI defaults to false when absent).
    assert!(page.get("required").is_none() || page["required"] == false);
    assert_eq!(page["schema"]["type"], "integer");
    assert_eq!(page["schema"]["nullable"], true);
}

#[test]
fn path_parameters_are_documented() {
    let doc = doc();
    let params = &doc["paths"]["/users/{id}"]["get"]["parameters"];
    assert_eq!(params.as_array().unwrap().len(), 1);
    let id = &params[0];
    assert_eq!(id["name"], "id");
    assert_eq!(id["in"], "path");
    assert_eq!(id["required"], true);
}

#[test]
fn request_body_is_documented() {
    let doc = doc();
    let rb = &doc["paths"]["/users"]["post"]["requestBody"];
    assert_eq!(rb["required"], true);
    let schema = &rb["content"]["application/json"]["schema"];
    assert_eq!(schema["type"], "object");
    assert!(schema["properties"]["name"].is_object());
    assert!(schema["properties"]["email"].is_object());
}

#[test]
fn success_and_extra_responses_are_documented() {
    let doc = doc();
    let responses = &doc["paths"]["/users"]["get"]["responses"];
    assert!(responses["201"].is_object());
    assert_eq!(responses["201"]["description"], "User created");
    assert!(responses["400"].is_object());
}

#[test]
fn components_are_collected_recursively() {
    let doc = doc();
    let schemas = &doc["components"]["schemas"];
    assert!(schemas["UserResponse"].is_object());
    // Nested struct referenced from UserResponse must also be registered.
    assert!(schemas["Nested"].is_object());
    assert_eq!(schemas["Nested"]["properties"]["count"]["type"], "integer");
}

#[test]
fn operation_metadata_is_documented() {
    let doc = doc();
    let get = &doc["paths"]["/users"]["get"];
    assert_eq!(get["description"], "list users");
    // Operation id is derived from the handler function name.
    assert_eq!(get["operationId"], "list");
}
