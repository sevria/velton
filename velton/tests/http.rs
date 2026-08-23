//! End-to-end HTTP tests exercising routing, extraction (query/path/header/
//! body), responses, error handling and the docs routes.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::Arc;
use tower::ServiceExt;
use velton::axum::body::Body;
use velton::axum::http::{Request, StatusCode};
use velton::{OpenApi, Router, Server, ToSchema, controller, middleware::Cors};

// --- Request types -------------------------------------------------------------

#[derive(Serialize, Deserialize, ToSchema)]
struct QueryReq {
    #[schema(source = Source::Query)]
    name: String,
    #[schema(source = Source::Query)]
    page: Option<u32>,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct PathReq {
    #[schema(source = Source::Path)]
    id: u64,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct HeaderReq {
    #[schema(source = Source::Header)]
    api_key: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
struct BodyReq {
    name: String,
    email: String,
}

// --- Response types -------------------------------------------------------------

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 200)]
struct OkResponse {
    message: String,
}

#[derive(Serialize, Deserialize, ToSchema)]
#[schema(status_code = 201)]
struct CreatedResponse {
    id: u64,
}

// --- App error -----------------------------------------------------------------

#[derive(Debug)]
struct AppError;

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("boom")
    }
}

impl std::error::Error for AppError {}

// --- Controller ---------------------------------------------------------------

struct TestController {
    service: Arc<TestService>,
}

struct TestService;

#[controller]
impl TestController {
    fn new(service: Arc<TestService>) -> Self {
        Self { service }
    }

    #[endpoint(method = get, path = "/test")]
    async fn query(self, req: QueryReq) -> Result<OkResponse, AppError> {
        let _ = &self.service;
        Ok(OkResponse {
            message: format!("query {} {:?}", req.name, req.page),
        })
    }

    #[endpoint(method = get, path = "/test/:id")]
    async fn path(self, req: PathReq) -> Result<OkResponse, AppError> {
        Ok(OkResponse {
            message: format!("path {}", req.id),
        })
    }

    #[endpoint(method = get, path = "/test/h")]
    async fn header(self, req: HeaderReq) -> Result<OkResponse, AppError> {
        Ok(OkResponse {
            message: format!("header {}", req.api_key),
        })
    }

    #[endpoint(method = post, path = "/test", description = "create something")]
    async fn body(self, req: BodyReq) -> Result<CreatedResponse, AppError> {
        Ok(CreatedResponse {
            id: req.email.len() as u64,
        })
    }

    #[endpoint(method = get, path = "/test/err")]
    async fn err(self) -> Result<OkResponse, AppError> {
        Err(AppError)
    }
}

fn build_router() -> Router {
    Router::builder()
        .openapi(
            OpenApi::builder()
                .name("test")
                .version("1.0.0")
                .server(Server::builder().url("http://localhost:3000").build())
                .build(),
        )
        .controller(TestController::new(Arc::new(TestService)))
        .middleware(Cors::permissive())
        .build()
        .unwrap()
}

async fn call(
    uri: &str,
    method: &str,
    body: Option<Value>,
    headers: &[(&str, &str)],
) -> (StatusCode, Value) {
    let app = build_router().into_axum();
    let mut builder = Request::builder().uri(uri).method(method);
    for (k, v) in headers {
        builder = builder.header(*k, *v);
    }
    let body_text = body.map(|b| b.to_string()).unwrap_or_default();
    let req = builder.body(Body::from(body_text)).unwrap();
    let res = app.oneshot(req).await.unwrap();
    let status = res.status();
    let bytes = velton::axum::body::to_bytes(res.into_body(), 1024 * 1024)
        .await
        .unwrap();
    let value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, value)
}

#[tokio::test]
async fn query_params_are_extracted() {
    let (status, body) = call("/test?name=John", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "query John None");

    let (_, body) = call("/test?name=John&page=2", "GET", None, &[]).await;
    assert_eq!(body["message"], "query John Some(2)");
}

#[tokio::test]
async fn missing_required_query_is_400() {
    let (status, body) = call("/test", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].is_string());
}

#[tokio::test]
async fn path_params_are_extracted() {
    let (status, body) = call("/test/42", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "path 42");
}

#[tokio::test]
async fn headers_are_extracted_with_x_prefix() {
    let (status, body) = call("/test/h", "GET", None, &[("X-Api-Key", "secret")]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["message"], "header secret");
}

#[tokio::test]
async fn missing_required_header_is_400() {
    let (status, body) = call("/test/h", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["message"].is_string());
}

#[tokio::test]
async fn body_is_extracted_as_json() {
    let payload = json!({ "name": "Ada", "email": "ada@example.com" });
    let (status, body) = call("/test", "POST", Some(payload), &[]).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["id"], 15);
}

#[tokio::test]
async fn invalid_body_is_400() {
    let (status, _) = call("/test", "POST", Some(json!({"name": "Ada"})), &[]).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handler_errors_map_to_default_500() {
    let (status, body) = call("/test/err", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    assert_eq!(body["message"], "boom");
}

#[tokio::test]
async fn openapi_json_is_served() {
    let (status, body) = call("/openapi.json", "GET", None, &[]).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["openapi"], "3.0.3");
    assert_eq!(body["info"]["title"], "test");
}

#[tokio::test]
async fn docs_page_is_served() {
    let app = build_router().into_axum();
    let req = Request::builder().uri("/docs").body(Body::empty()).unwrap();
    let res = app.oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
}
