//! `@chunk`: adds the generated chunk collection and the parent's list field.
//!
//! For `body: String @chunk` on `Article`, it adds a type `Article__body_chunk`
//! with `source`, `text` and `hash`, and a field `_body_chunks` on `Article`.
//! This runs before relations are resolved, so both are built like any other type.

use graphql_parser::schema::Directive;
use schema::{
    chunk_collection_name, chunk_list_field, CType, ChunkSource, ChunkedField, CollectionVersion,
    CHUNK_HASH_FIELD, CHUNK_SOURCE_FIELD, CHUNK_TEXT_FIELD,
};

use crate::error::{QueryError, Result};

use super::directives::{directive_u32, get_directive_arg, ChunkDirective, IndexConfig};
use super::parser::{ParsedField, ParsedType, ParsedTypeDef, ParsedTypeDirectives, SdlParser};

const DEFAULT_CHUNK_MAX_CHARS: u32 = 1600;
/// Smaller chunks carry too little text to embed or rank well.
const MIN_CHUNK_MAX_CHARS: u32 = 64;

/// One `@chunk` field found while parsing.
#[derive(Debug, Clone)]
pub(super) struct ChunkSpec {
    pub(super) parent: String,
    pub(super) field: String,
    pub(super) config: ChunkDirective,
}

pub(super) fn parse_chunk_directive(directive: &Directive<'_, String>) -> Result<ChunkDirective> {
    let read = |name: &str, default: u32| -> Result<u32> {
        get_directive_arg(directive, name)
            .map(|value| {
                directive_u32(name, value).map_err(|e| QueryError::parse(format!("@chunk {e}")))
            })
            .unwrap_or(Ok(default))
    };
    let max_chars = read("maxChars", DEFAULT_CHUNK_MAX_CHARS)?;
    if max_chars < MIN_CHUNK_MAX_CHARS {
        return Err(QueryError::parse(format!(
            "@chunk maxChars must be at least {MIN_CHUNK_MAX_CHARS}, got {max_chars}"
        )));
    }
    Ok(ChunkDirective {
        max_chars,
        overlap: read("overlap", 0)?,
    })
}

fn scalar(base: &str, is_list: bool) -> ParsedType {
    ParsedType {
        base_type: base.to_string(),
        is_list,
        is_non_null: false,
        element_non_null: false,
    }
}

fn field(name: &str, field_type: ParsedType) -> ParsedField {
    ParsedField {
        name: name.to_string(),
        field_type,
        directives: Default::default(),
    }
}

impl SdlParser<'_> {
    /// Adds the generated chunk types and list fields for every `@chunk` field.
    pub(super) fn synthesize_chunk_types(&mut self) -> Result<Vec<ChunkSpec>> {
        let mut specs = Vec::new();
        for parent in self.definition_order.clone() {
            let Some(type_def) = self.type_defs.get(&parent) else {
                continue;
            };
            for f in &type_def.fields {
                let Some(config) = f.directives.chunk else {
                    continue;
                };
                let lww = matches!(f.directives.crdt_type, None | Some(CType::LwwRegister));
                if f.field_type.base_type != "String" || f.field_type.is_list || !lww {
                    return Err(QueryError::parse(format!(
                        "@chunk is only allowed on a last-writer-wins String field. Field: {}.{}",
                        parent, f.name
                    )));
                }
                if type_def.is_interface {
                    return Err(QueryError::parse(format!(
                        "@chunk is not allowed on an interface. Field: {}.{}",
                        parent, f.name
                    )));
                }
                specs.push(ChunkSpec {
                    parent: parent.clone(),
                    field: f.name.clone(),
                    config,
                });
            }
        }

        for spec in &specs {
            let chunk_type = chunk_collection_name(&spec.parent, &spec.field);
            let list = chunk_list_field(&spec.field);
            if self.type_defs.contains_key(&chunk_type)
                || self.known_external_types.contains(&chunk_type)
            {
                return Err(QueryError::parse(format!(
                    "@chunk needs the type name {chunk_type}, which is already taken"
                )));
            }
            let Some(parent) = self.type_defs.get_mut(&spec.parent) else {
                continue;
            };
            if parent.fields.iter().any(|f| f.name == list) {
                return Err(QueryError::parse(format!(
                    "@chunk needs the field name {}.{list}, which is already taken",
                    spec.parent
                )));
            }
            let mut hashes = scalar("String", true);
            hashes.element_non_null = true;
            parent.fields.push(field(&list, hashes));

            // One index on `_sourceID` and `hash` serves both lookups writes need,
            // one chunk of a parent and all of them.
            let mut source = field(CHUNK_SOURCE_FIELD, scalar(&spec.parent, false));
            source.directives.is_primary = true;
            source.directives.index.push(IndexConfig {
                includes: vec![(CHUNK_HASH_FIELD.to_string(), false)],
                ..IndexConfig::default()
            });
            let position = self
                .definition_order
                .iter()
                .position(|name| *name == spec.parent)
                .map_or(self.definition_order.len(), |i| i + 1);
            self.definition_order.insert(position, chunk_type.clone());
            self.type_defs.insert(
                chunk_type.clone(),
                ParsedTypeDef {
                    name: chunk_type,
                    fields: vec![
                        source,
                        field(CHUNK_TEXT_FIELD, scalar("String", false)),
                        field(CHUNK_HASH_FIELD, scalar("String", false)),
                    ],
                    directives: ParsedTypeDirectives::default(),
                    is_interface: false,
                },
            );
        }
        Ok(specs)
    }
}

/// Records each `@chunk` field on its parent and on its generated collection.
pub(super) fn attach_chunk_settings(collections: &mut [CollectionVersion], specs: &[ChunkSpec]) {
    for spec in specs {
        let chunk_type = chunk_collection_name(&spec.parent, &spec.field);
        for collection in collections.iter_mut() {
            if collection.name == spec.parent {
                collection.chunked_fields.push(ChunkedField {
                    field_name: spec.field.clone(),
                    max_chars: spec.config.max_chars,
                    overlap: spec.config.overlap,
                });
            } else if collection.name == chunk_type {
                collection.chunk_of = Some(ChunkSource {
                    collection: spec.parent.clone(),
                    field_name: spec.field.clone(),
                });
            }
        }
    }
}

#[cfg(test)]
#[path = "chunk_tests.rs"]
mod tests;
