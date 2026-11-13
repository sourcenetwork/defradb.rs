use std::ffi::{c_char, c_int};

use acp::nac::NodePermission;

use crate::request::Request;
use crate::target::{go_string, non_empty, split_comma_separated, CollectionArgs};
use crate::types::{CollectionOptions, DefraResult};
use crate::{ffi_async, ffi_entry, try_ffi};

/// Creates the index `index_name` on the collection `opts.name` names, over
/// `fields`: comma-separated field names, each optionally `:ASC` (the
/// default) or `:DESC`. `vector_json`, a vector index description, makes it a
/// vector index (null or empty for none). Answers with the index
/// description as JSON.
///
/// # Safety
///
/// `index_name`, `fields` and `vector_json` must be null or valid
/// null-terminated strings; every non-null string in `opts` must be one too.
#[no_mangle]
pub unsafe extern "C" fn defra_index_new(
    node_ptr: usize,
    index_name: *const c_char,
    fields: *const c_char,
    is_unique: c_int,
    vector_json: *const c_char,
    opts: CollectionOptions,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::IndexCreate));
        let node = try_ffi!(request.require_node("index create"));
        let fields = try_ffi!(parse_fields(&go_string(fields)).map_err(DefraResult::error));
        let vector = match non_empty(vector_json) {
            Some(json) => match serde_json::from_str::<schema::VectorIndexDescription>(&json) {
                Ok(vector) => Some(vector),
                Err(error) => return DefraResult::error(error.to_string()),
            },
            None => None,
        };
        let index_name = non_empty(index_name);
        let collection_name = CollectionArgs::read(&opts).name.unwrap_or_default();
        let database = try_ffi!(request.database());

        ffi_async!(node, {
            let kind = db::IndexManager::requested_kind(&fields, is_unique != 0, vector)
                .map_err(|e| e.to_string())?;
            let index = database
                .create_index(&collection_name, index_name.as_deref(), fields, kind)
                .await
                .map_err(|e| e.to_string())?;
            serde_json::to_string(&index).map_err(|e| format!("failed to serialize result: {}", e))
        })
    }
}

/// Go's index field list: `name`, `name:ASC` or `name:DESC`, comma-separated.
fn parse_fields(fields: &str) -> Result<Vec<schema::IndexedFieldDescription>, String> {
    split_comma_separated(fields)
        .into_iter()
        .map(|field| {
            let parts: Vec<&str> = field.split(':').collect();
            let descending = match parts.as_slice() {
                [_] => false,
                [_, order] => match order.to_uppercase().as_str() {
                    "ASC" => false,
                    "DESC" => true,
                    _ => return Err("invalid ascension order: expected ASC or DESC".to_string()),
                },
                _ => {
                    return Err(format!(
                        "invalid or malformed field description. Field: {}",
                        field
                    ))
                }
            };
            Ok(schema::IndexedFieldDescription {
                name: parts[0].to_string(),
                descending,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_fields;

    #[test]
    fn parses_go_index_fields() {
        let fields = parse_fields("name,age:desc,email:ASC").unwrap();
        let parsed: Vec<_> = fields
            .iter()
            .map(|f| (f.name.as_str(), f.descending))
            .collect();
        assert_eq!(parsed, [("name", false), ("age", true), ("email", false)]);
        assert!(parse_fields("name:sideways").is_err());
        assert!(parse_fields("a:b:c").is_err());
    }
}
