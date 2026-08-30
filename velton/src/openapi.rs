//! OpenAPI 3.0 document model.

pub use crate::schema::{Components, Schema};
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use stringcase::title_case;

fn is_false(b: &bool) -> bool {
    !*b
}

/// The root OpenAPI document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenApi {
    pub openapi: String,
    pub info: Info,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub servers: Vec<Server>,
    #[serde(default)]
    pub paths: IndexMap<String, PathItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub components: Option<ComponentsObject>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<Tag>,
}

/// The `components` object, wrapping named reusable objects.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ComponentsObject {
    #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
    pub schemas: Components,
}

impl Default for OpenApi {
    fn default() -> Self {
        OpenApi {
            openapi: "3.0.3".to_string(),
            info: Info::default(),
            servers: Vec::new(),
            paths: IndexMap::new(),
            components: None,
            tags: Vec::new(),
        }
    }
}

impl OpenApi {
    /// Starts building an OpenAPI document.
    pub fn builder() -> OpenApiBuilder {
        OpenApiBuilder::default()
    }

    pub(crate) fn merge(&mut self, paths: Vec<(String, PathItem)>, components: Components) {
        for (path, item) in paths {
            match self.paths.get_mut(&path) {
                Some(existing) => existing.merge(item),
                None => {
                    self.paths.insert(path, item);
                }
            }
        }
        if !components.is_empty() {
            let target = self.components.get_or_insert_with(Default::default);
            for (name, schema) in components {
                target.schemas.entry(name).or_insert(schema);
            }
        }
    }
}

/// Builder for [`OpenApi`].
#[derive(Debug, Default)]
pub struct OpenApiBuilder {
    name: Option<String>,
    version: Option<String>,
    description: Option<String>,
    servers: Vec<Server>,
    tags: Vec<Tag>,
}

impl OpenApiBuilder {
    /// The API title.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// The API version.
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.version = Some(version.into());
        self
    }

    /// A short description of the API.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Adds a server.
    pub fn server(mut self, server: Server) -> Self {
        self.servers.push(server);
        self
    }

    /// Adds a tag.
    pub fn tag(mut self, tag: Tag) -> Self {
        self.tags.push(tag);
        self
    }

    /// Builds the [`OpenApi`] document.
    pub fn build(self) -> OpenApi {
        OpenApi {
            openapi: "3.0.3".to_string(),
            info: Info {
                title: title_case(&self.name.unwrap_or_default()),
                version: self.version.unwrap_or_else(|| "0.1.0".to_string()),
                description: self.description,
            },
            servers: self.servers,
            paths: IndexMap::new(),
            components: None,
            tags: self.tags,
        }
    }
}

/// OpenAPI info object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Info {
    pub title: String,
    pub version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// OpenAPI server object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server {
    pub url: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

impl Server {
    /// Starts building a server object.
    pub fn builder() -> ServerBuilder {
        ServerBuilder::default()
    }
}

/// Builder for [`Server`].
#[derive(Debug, Default)]
pub struct ServerBuilder {
    url: Option<String>,
    description: Option<String>,
}

impl ServerBuilder {
    /// The server URL.
    pub fn url(mut self, url: impl Into<String>) -> Self {
        self.url = Some(url.into());
        self
    }

    /// An optional description.
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }

    /// Builds the [`Server`].
    pub fn build(self) -> Server {
        Server {
            url: self.url.unwrap_or_default(),
            description: self.description,
        }
    }
}

/// OpenAPI tag object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tag {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

/// A path item holding operations for each HTTP method.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub get: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub put: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delete: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub patch: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub options: Option<Operation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<Operation>,
}

impl PathItem {
    /// Merges `other` into `self`, overwriting any present fields.
    pub fn merge(&mut self, other: PathItem) {
        if other.summary.is_some() {
            self.summary = other.summary;
        }
        if other.description.is_some() {
            self.description = other.description;
        }
        if other.get.is_some() {
            self.get = other.get;
        }
        if other.post.is_some() {
            self.post = other.post;
        }
        if other.put.is_some() {
            self.put = other.put;
        }
        if other.delete.is_some() {
            self.delete = other.delete;
        }
        if other.patch.is_some() {
            self.patch = other.patch;
        }
        if other.options.is_some() {
            self.options = other.options;
        }
        if other.head.is_some() {
            self.head = other.head;
        }
    }
}

/// A single API operation.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Operation {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation_id: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub tags: Vec<String>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub parameters: Vec<Parameter>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_body: Option<RequestBody>,
    #[serde(default)]
    pub responses: IndexMap<String, Response>,
}

/// Where a parameter appears.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ParameterIn {
    #[default]
    Query,
    Path,
    Header,
    Cookie,
}

/// An operation parameter.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Parameter {
    pub name: String,
    #[serde(rename = "in")]
    pub r#in: ParameterIn,
    #[serde(skip_serializing_if = "is_false", default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
}

/// A request body.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequestBody {
    #[serde(skip_serializing_if = "is_false", default)]
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub content: Content,
}

/// Media type content for a body or response.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Content {
    #[serde(rename = "application/json", skip_serializing_if = "Option::is_none")]
    pub json: Option<MediaType>,
}

impl Content {
    /// Creates `application/json` content for the given schema.
    pub fn json(schema: Schema) -> Content {
        Content {
            json: Some(MediaType {
                schema: Some(schema),
                example: None,
            }),
        }
    }
}

/// A media type descriptor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MediaType {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schema: Option<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
}

/// A response descriptor.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Response {
    pub description: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<Content>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::ToSchema;

    #[test]
    fn builder_builds_document() {
        let doc = OpenApi::builder()
            .name("my-app")
            .version("0.1.0")
            .description("Lorem ipsum")
            .server(Server::builder().url("http://localhost:3000").build())
            .build();
        let json = serde_json::to_value(&doc).unwrap();
        assert_eq!(json["openapi"], "3.0.3");
        assert_eq!(json["info"]["title"], "my-app");
        assert_eq!(json["info"]["version"], "0.1.0");
        assert_eq!(json["servers"][0]["url"], "http://localhost:3000");
    }

    #[test]
    fn parameter_serializes_in() {
        let p = Parameter {
            name: "name".to_string(),
            r#in: ParameterIn::Query,
            required: true,
            schema: Some(String::schema()),
            ..Default::default()
        };
        let json = serde_json::to_value(&p).unwrap();
        assert_eq!(json["in"], "query");
        assert_eq!(json["name"], "name");
    }

    #[test]
    fn content_json_key() {
        let c = Content::json(String::schema());
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json["application/json"]["schema"]["type"], "string");
    }
}
