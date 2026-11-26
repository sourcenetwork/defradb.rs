//! GraphQL SDL parsing to create CollectionVersion schemas
//!
//! This module parses GraphQL Schema Definition Language (SDL) and converts
//! type definitions into DefraDB CollectionVersion schemas.
//!
//! # Module Organization
//!
//! - `parser`: Main SdlParser struct and parse logic
//! - `directives`: Directive parsing
//! - `warnings`: Warning types
//! - `fields`: Field and directive parsing
//! - `validation`: Schema validation
//! - `builder`: Collection building
//! - `builder_cycles`: Circular relation detection
//! - `builder_field_kinds`: Field kind resolution
//! - `field_type_refusal`: Field type declarations that are refused
//! - `chunk`: Generated `@chunk` collections and list fields
//! - `helpers`: Shared parsing utilities

mod builder;
mod builder_cycles;
mod builder_field_kinds;
mod chunk;
mod directives;
mod field_type_refusal;
mod fields;
mod helpers;
mod parser;
mod validation;
mod warnings;

pub use directives::ParsedDirectives;
pub use field_type_refusal::{
    FieldTypeRefusal, FieldTypeRefusals, UNSUPPORTED_FIELD_TYPE_ERROR_CODE,
};
pub use parser::{parse_sdl, parse_sdl_with_known_types, parse_sdl_with_warnings, SdlParser};
pub use warnings::{DirectiveLocation, ParseOutput, ParseWarning};
