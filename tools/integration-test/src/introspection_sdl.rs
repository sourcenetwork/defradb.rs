//! The introspection result a GraphQL SDL describes, so a schema printed as
//! SDL (Go's generator fixtures) can be compared with what a node introspects.
//!
//! The value has the shape of `__schema` in the answer to
//! `client_introspection.gql`: root type names, every type with its fields,
//! arguments, input fields and enum values, and every directive.

use graphql_parser::query::Type;
use graphql_parser::schema::{
    parse_schema, Definition, Document, EnumValue, Field, InputValue, TypeDefinition,
};
use serde_json::{json, Value};

type Text = String;

/// `__schema` as introspection would describe the schema `sdl` prints.
///
/// # Panics
///
/// If `sdl` does not parse, since the SDL is a fixture.
pub fn introspection(sdl: &str) -> Value {
    let doc: Document<'_, Text> = parse_schema(sdl).expect("fixture SDL parses");
    let types: Vec<&TypeDefinition<'_, Text>> = doc
        .definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::TypeDefinition(ty) => Some(ty),
            _ => None,
        })
        .collect();
    let kind_of = |name: &str| {
        types
            .iter()
            .find(|ty| type_name(ty) == name)
            .map_or("SCALAR", |ty| kind(ty))
    };
    let schema = doc
        .definitions
        .iter()
        .find_map(|definition| match definition {
            Definition::SchemaDefinition(schema) => Some(schema),
            _ => None,
        });
    let root = |name: Option<&String>| name.map_or(Value::Null, |name| json!({ "name": name }));
    json!({
        "queryType": root(schema.and_then(|s| s.query.as_ref())),
        "mutationType": root(schema.and_then(|s| s.mutation.as_ref())),
        "subscriptionType": root(schema.and_then(|s| s.subscription.as_ref())),
        "types": types.iter().map(|ty| full_type(ty, &kind_of)).collect::<Vec<_>>(),
        "directives": doc.definitions.iter().filter_map(|definition| match definition {
            Definition::DirectiveDefinition(directive) => Some(json!({
                "name": directive.name,
                "description": directive.description,
                "locations": directive.locations.iter().map(|at| at.as_str()).collect::<Vec<_>>(),
                "args": inputs(&directive.arguments, &kind_of),
            })),
            _ => None,
        }).collect::<Vec<_>>(),
    })
}

fn type_name<'d>(ty: &'d TypeDefinition<'_, Text>) -> &'d str {
    match ty {
        TypeDefinition::Scalar(t) => &t.name,
        TypeDefinition::Object(t) => &t.name,
        TypeDefinition::Interface(t) => &t.name,
        TypeDefinition::Union(t) => &t.name,
        TypeDefinition::Enum(t) => &t.name,
        TypeDefinition::InputObject(t) => &t.name,
    }
}

fn kind(ty: &TypeDefinition<'_, Text>) -> &'static str {
    match ty {
        TypeDefinition::Scalar(_) => "SCALAR",
        TypeDefinition::Object(_) => "OBJECT",
        TypeDefinition::Interface(_) => "INTERFACE",
        TypeDefinition::Union(_) => "UNION",
        TypeDefinition::Enum(_) => "ENUM",
        TypeDefinition::InputObject(_) => "INPUT_OBJECT",
    }
}

fn full_type<K>(ty: &TypeDefinition<'_, Text>, kind_of: &K) -> Value
where
    K: Fn(&str) -> &'static str,
{
    let (description, fields, input_fields, enum_values, interfaces) = match ty {
        TypeDefinition::Object(t) => (
            &t.description,
            fields(&t.fields, kind_of),
            Value::Null,
            Value::Null,
            named_refs(&t.implements_interfaces, "INTERFACE"),
        ),
        TypeDefinition::Interface(t) => (
            &t.description,
            fields(&t.fields, kind_of),
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        TypeDefinition::InputObject(t) => (
            &t.description,
            Value::Null,
            inputs(&t.fields, kind_of),
            Value::Null,
            Value::Null,
        ),
        TypeDefinition::Enum(t) => (
            &t.description,
            Value::Null,
            Value::Null,
            enum_values(&t.values),
            Value::Null,
        ),
        TypeDefinition::Scalar(t) => (
            &t.description,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ),
        TypeDefinition::Union(t) => (
            &t.description,
            Value::Null,
            Value::Null,
            Value::Null,
            Value::Null,
        ),
    };
    let possible_types = match ty {
        TypeDefinition::Union(t) => named_refs(&t.types, "OBJECT"),
        _ => Value::Null,
    };
    json!({
        "kind": kind(ty),
        "name": type_name(ty),
        "description": description,
        "fields": fields,
        "inputFields": input_fields,
        "interfaces": interfaces,
        "enumValues": enum_values,
        "possibleTypes": possible_types,
    })
}

fn fields<K>(fields: &[Field<'_, Text>], kind_of: &K) -> Value
where
    K: Fn(&str) -> &'static str,
{
    fields
        .iter()
        .map(|field| {
            let (deprecated, reason) = deprecation(&field.directives);
            json!({
                "name": field.name,
                "description": field.description,
                "args": inputs(&field.arguments, kind_of),
                "type": type_ref(&field.field_type, kind_of),
                "isDeprecated": deprecated,
                "deprecationReason": reason,
            })
        })
        .collect()
}

fn inputs<K>(inputs: &[InputValue<'_, Text>], kind_of: &K) -> Value
where
    K: Fn(&str) -> &'static str,
{
    inputs
        .iter()
        .map(|input| {
            json!({
                "name": input.name,
                "description": input.description,
                "type": type_ref(&input.value_type, kind_of),
                "defaultValue": input.default_value.as_ref().map(ToString::to_string),
            })
        })
        .collect()
}

fn enum_values(values: &[EnumValue<'_, Text>]) -> Value {
    values
        .iter()
        .map(|value| {
            let (deprecated, reason) = deprecation(&value.directives);
            json!({
                "name": value.name,
                "description": value.description,
                "isDeprecated": deprecated,
                "deprecationReason": reason,
            })
        })
        .collect()
}

fn named_refs(names: &[String], kind: &str) -> Value {
    names
        .iter()
        .map(|name| json!({ "kind": kind, "name": name, "ofType": null }))
        .collect()
}

fn type_ref<K>(ty: &Type<'_, Text>, kind_of: &K) -> Value
where
    K: Fn(&str) -> &'static str,
{
    match ty {
        Type::NamedType(name) => json!({ "kind": kind_of(name), "name": name, "ofType": null }),
        Type::ListType(of) => {
            json!({ "kind": "LIST", "name": null, "ofType": type_ref(of, kind_of) })
        }
        Type::NonNullType(of) => {
            json!({ "kind": "NON_NULL", "name": null, "ofType": type_ref(of, kind_of) })
        }
    }
}

/// `@deprecated(reason:)` as introspection reports it.
fn deprecation(directives: &[graphql_parser::schema::Directive<'_, Text>]) -> (bool, Value) {
    let Some(deprecated) = directives.iter().find(|d| d.name == "deprecated") else {
        return (false, Value::Null);
    };
    let reason = deprecated
        .arguments
        .iter()
        .find(|(name, _)| name == "reason")
        .map_or_else(
            || json!("No longer supported"),
            |(_, value)| match value {
                graphql_parser::query::Value::String(text) => json!(text),
                other => json!(other.to_string()),
            },
        );
    (true, reason)
}
