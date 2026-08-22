//! Interactive API docs page (Scalar).

/// The path the OpenAPI JSON document is served at.
pub const OPENAPI_PATH: &str = "/openapi.json";

/// The path the interactive docs UI is served at.
pub const DOCS_PATH: &str = "/docs";

/// Returns the Scalar docs page, loading `@scalar/api-reference` from a CDN and
/// pointing it at `OPENAPI_PATH`.
pub fn scalar_html() -> &'static str {
    r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>API Reference</title>
    <style>
      html, body, #api-reference { height: 100%; margin: 0; }
    </style>
  </head>
  <body>
    <script id="api-reference" data-url="/openapi.json"></script>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
  </body>
</html>"#
}
