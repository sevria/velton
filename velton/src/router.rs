//! The [`Router`] and [`RouterBuilder`].

use crate::controller::ControllerBox;
use crate::docs::{DOCS_PATH, OPENAPI_PATH};
use crate::error::{Error, ErrorHandler};
use crate::extract::set_body_limit;
use crate::openapi::OpenApi;
use axum::Json;
use axum::response::Html;
use axum::routing::get;
use std::net::SocketAddr;

const DEFAULT_BIND: &str = "127.0.0.1:3000";

/// A function that applies a tower layer to an axum router.
type LayerFn = Box<dyn Fn(axum::Router) -> axum::Router + Send + Sync>;

/// A built velton router.
#[derive(Clone)]
pub struct Router {
    app: axum::Router,
    openapi: OpenApi,
    bind: SocketAddr,
}

impl Router {
    /// Starts building a router.
    pub fn builder() -> RouterBuilder {
        RouterBuilder::default()
    }

    /// The assembled OpenAPI document (before serving).
    pub fn openapi_doc(&self) -> &OpenApi {
        &self.openapi
    }

    /// Binds to `bind` and serves HTTP until shutdown.
    ///
    /// The router serves the OpenAPI document at [`OPENAPI_PATH`] and the
    /// interactive docs at [`DOCS_PATH`] in addition to controller routes.
    pub async fn run(self) -> Result<(), Error> {
        let listener = tokio::net::TcpListener::bind(self.bind)
            .await
            .map_err(|source| Error::Bind {
                addr: self.bind,
                source,
            })?;
        log::info!("velton: listening on http://{}", self.bind);
        log::info!("velton: openapi at http://{}{}", self.bind, OPENAPI_PATH);
        log::info!("velton: docs at http://{}{}", self.bind, DOCS_PATH);
        axum::serve(listener, self.app)
            .await
            .map_err(|e| Error::Serve(Box::new(e)))?;
        Ok(())
    }

    /// Extracts the underlying axum router (used by tests and advanced users).
    #[doc(hidden)]
    pub fn into_axum(self) -> axum::Router {
        self.app
    }
}

/// Builder for [`Router`].
#[derive(Default)]
pub struct RouterBuilder {
    openapi: OpenApi,
    controllers: Vec<ControllerBox>,
    layers: Vec<LayerFn>,
    bind: Option<String>,
    error_handler: Option<ErrorHandler>,
    body_limit: Option<usize>,
}

impl RouterBuilder {
    /// Sets the base OpenAPI document.
    pub fn openapi(mut self, openapi: OpenApi) -> Self {
        self.openapi = openapi;
        self
    }

    /// Adds a controller (via `#[controller(...)]`).
    pub fn controller<C: crate::controller::__Router>(mut self, controller: C) -> Self {
        self.controllers.push(ControllerBox::new(controller));
        self
    }

    /// Applies a tower layer (e.g. `Cors::permissive()`) to the router.
    pub fn middleware<L>(mut self, layer: L) -> Self
    where
        L: tower::Layer<axum::routing::Route> + Clone + Send + Sync + 'static,
        L::Service: tower::Service<
                axum::http::Request<axum::body::Body>,
                Response = axum::response::Response,
                Error = std::convert::Infallible,
            > + Clone
            + Send
            + Sync
            + 'static,
        <L::Service as tower::Service<axum::http::Request<axum::body::Body>>>::Future:
            Send + 'static,
    {
        let layer = layer.clone();
        self.layers.push(Box::new(move |router: axum::Router| {
            router.layer(layer.clone())
        }));
        self
    }

    /// Sets the bind address (defaults to `127.0.0.1:3000`).
    pub fn bind(mut self, addr: impl Into<String>) -> Self {
        self.bind = Some(addr.into());
        self
    }

    /// Sets a custom handler for errors returned by controller handlers.
    pub fn error_handler(mut self, handler: ErrorHandler) -> Self {
        self.error_handler = Some(handler);
        self
    }

    /// Sets the maximum request body size in bytes.
    pub fn body_limit(mut self, bytes: usize) -> Self {
        self.body_limit = Some(bytes);
        self
    }

    /// Builds the router, merging controllers, OpenAPI docs and middleware.
    pub fn build(mut self) -> Result<Router, Error> {
        if let Some(handler) = self.error_handler.take() {
            crate::error::set_error_handler(handler).map_err(|()| {
                Error::OpenApi("an error handler is already configured".to_string())
            })?;
        }
        if let Some(limit) = self.body_limit.take() {
            set_body_limit(limit);
        }

        let mut app = axum::Router::new();
        for controller in &self.controllers {
            app = app.merge(controller.build_router());
        }

        // Assemble the OpenAPI document.
        let mut openapi = self.openapi;
        for controller in &self.controllers {
            let (paths, components) = controller.openapi();
            openapi.merge(paths, components);
        }

        // Docs routes.
        let doc = openapi.clone();
        app = app.route(OPENAPI_PATH, get(move || async move { Json(doc) }));
        app = app.route(
            DOCS_PATH,
            get(|| async { Html(crate::docs::scalar_html()) }),
        );

        // Apply middleware last so it wraps every route.
        for layer in self.layers {
            app = layer(app);
        }

        let bind = self.bind.unwrap_or_else(|| DEFAULT_BIND.to_string());
        let addr: SocketAddr = bind
            .parse()
            .map_err(|e| Error::OpenApi(format!("invalid bind address `{bind}`: {e}")))?;

        Ok(Router {
            app,
            openapi,
            bind: addr,
        })
    }
}
