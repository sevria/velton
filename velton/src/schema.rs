//! OpenAPI 3.0 schema types and the [`Schema`] trait.

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

/// The `type` value of an OpenAPI schema object.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SchemaType {
    String,
    Number,
    Integer,
    Boolean,
    Object,
    Array,
    Null,
}

fn is_false(b: &bool) -> bool {
    !*b
}

/// An OpenAPI 3.0 schema object.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Schema {
    #[serde(skip_serializing_if = "Option::is_none", rename = "$ref")]
    pub reference: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "type")]
    pub schema_type: Option<SchemaType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub example: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default: Option<Value>,
    #[serde(skip_serializing_if = "is_false", default)]
    pub nullable: bool,
    #[serde(skip_serializing_if = "is_false", default)]
    pub deprecated: bool,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub required: Vec<String>,
    #[serde(skip_serializing_if = "IndexMap::is_empty", default)]
    pub properties: IndexMap<String, Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub items: Option<Box<Schema>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub minimum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maximum: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_length: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub multiple_of: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_items: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_items: Option<usize>,
    #[serde(skip_serializing_if = "Option::is_none", rename = "enum")]
    pub enum_values: Option<Vec<Value>>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub all_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub one_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub any_of: Vec<Schema>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub additional_properties: Option<Box<Schema>>,
}

impl Schema {
    /// A `$ref` to a named component schema.
    pub fn reference(name: impl Into<String>) -> Schema {
        Schema {
            reference: Some(format!("#/components/schemas/{}", name.into())),
            ..Default::default()
        }
    }

    /// An object schema with the given properties.
    pub fn object(properties: Vec<(String, Schema)>, required: Vec<String>) -> Schema {
        Schema {
            schema_type: Some(SchemaType::Object),
            properties: properties.into_iter().collect(),
            required,
            ..Default::default()
        }
    }
}

/// A map of named component schemas (insertion-ordered).
pub type Components = IndexMap<String, Schema>;

/// Types that can produce an OpenAPI schema.
///
/// * Primitives and containers return inline schemas.
/// * Named types (usually via `#[derive(Schema)]`) return a `$ref` from
///   [`ToSchema::schema`] and register themselves (and any nested types) via
///   [`ToSchema::schemas`].
pub trait ToSchema {
    /// Returns the OpenAPI schema object for this type.
    fn schema() -> Schema;

    /// Registers this type and any nested types into `components`.
    ///
    /// The default implementation is a no-op, which is correct for primitives.
    fn schemas(_components: &mut Components) {}
}

macro_rules! impl_primitive {
    ($ty:ty, $variant:ident) => {
        impl ToSchema for $ty {
            fn schema() -> Schema {
                Schema {
                    schema_type: Some(SchemaType::$variant),
                    ..Default::default()
                }
            }
        }
    };
}

impl_primitive!(bool, Boolean);
impl_primitive!(i8, Integer);
impl_primitive!(i16, Integer);
impl_primitive!(i32, Integer);
impl_primitive!(i64, Integer);
impl_primitive!(i128, Integer);
impl_primitive!(isize, Integer);
impl_primitive!(u8, Integer);
impl_primitive!(u16, Integer);
impl_primitive!(u32, Integer);
impl_primitive!(u64, Integer);
impl_primitive!(u128, Integer);
impl_primitive!(usize, Integer);
impl_primitive!(f32, Number);
impl_primitive!(f64, Number);

impl ToSchema for String {
    fn schema() -> Schema {
        Schema {
            schema_type: Some(SchemaType::String),
            ..Default::default()
        }
    }
}

impl ToSchema for &'static str {
    fn schema() -> Schema {
        <String as ToSchema>::schema()
    }
}

impl ToSchema for () {
    fn schema() -> Schema {
        Schema::default()
    }
}

impl ToSchema for Value {
    fn schema() -> Schema {
        Schema::default()
    }
}

impl<T: ToSchema> ToSchema for Option<T> {
    fn schema() -> Schema {
        let mut schema = T::schema();
        schema.nullable = true;
        schema
    }

    fn schemas(components: &mut Components) {
        T::schemas(components);
    }
}

impl<T: ToSchema> ToSchema for Vec<T> {
    fn schema() -> Schema {
        Schema {
            schema_type: Some(SchemaType::Array),
            items: Some(Box::new(T::schema())),
            ..Default::default()
        }
    }

    fn schemas(components: &mut Components) {
        T::schemas(components);
    }
}

impl<V: ToSchema> ToSchema for HashMap<String, V> {
    fn schema() -> Schema {
        Schema {
            schema_type: Some(SchemaType::Object),
            additional_properties: Some(Box::new(V::schema())),
            ..Default::default()
        }
    }

    fn schemas(components: &mut Components) {
        V::schemas(components);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn string_schema() {
        let s = String::schema();
        assert_eq!(s.schema_type, Some(SchemaType::String));
        assert!(s.reference.is_none());
    }

    #[test]
    fn option_makes_nullable() {
        let s = <Option<String>>::schema();
        assert_eq!(s.schema_type, Some(SchemaType::String));
        assert!(s.nullable);
    }

    #[test]
    fn vec_is_array() {
        let s = <Vec<i32>>::schema();
        assert_eq!(s.schema_type, Some(SchemaType::Array));
        let items = s.items.unwrap();
        assert_eq!(items.schema_type, Some(SchemaType::Integer));
    }

    #[test]
    fn object_serializes_properties_as_object() {
        let schema = Schema::object(
            vec![("name".to_string(), String::schema())],
            vec!["name".to_string()],
        );
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["type"], "object");
        assert_eq!(json["properties"]["name"]["type"], "string");
        assert_eq!(json["required"][0], "name");
    }

    #[test]
    fn reference_serializes_ref() {
        let schema = Schema::reference("Foo");
        let json = serde_json::to_value(&schema).unwrap();
        assert_eq!(json["$ref"], "#/components/schemas/Foo");
    }
}
