use std::ffi::c_char;

use acp::nac::NodePermission;
use storage::corekv::Key;

use crate::request::Request;
use crate::target::go_string;
use crate::types::DefraResult;
use crate::{ffi_async, ffi_entry, try_ffi};

/// Go's encrypted index type names; empty is the default, equality.
fn index_type(name: &str) -> Result<schema::EncryptedIndexType, String> {
    match name {
        "" | "equality" => Ok(schema::EncryptedIndexType::Equality),
        other => Err(format!("unsupported encrypted index type: {}", other)),
    }
}

/// Creates an encrypted index of type `index_type` ("equality", the default
/// for null or empty) on field `field_name` of collection `collection_name`,
/// and answers with its description as JSON.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_new(
    node_ptr: usize,
    collection_name: *const c_char,
    field_name: *const c_char,
    index_type_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::EncryptedIndexAdd));
        let node_ptr = try_ffi!(request.require_node("encrypted index create"));
        let index_type = try_ffi!(index_type(&go_string(index_type_name)).map_err(DefraResult::error));
        let collection_name_str = go_string(collection_name);
        let field_name_str = go_string(field_name);
        let database = try_ffi!(request.database());

        ffi_async!(node_ptr, {
            // Get the collection
            let collection = database
                .get_collection(&collection_name_str)
                .map_err(|e| format!("failed to get collection: {}", e))?
                .ok_or_else(|| format!("collection '{}' not found", collection_name_str))?;

            let schema = collection.schema();

            // Check if field exists in the collection
            let field_exists = schema.fields.iter().any(|f| f.name == field_name_str);
            if !field_exists {
                return Err(format!(
                    "encrypted index on non-existent field. Field: {}",
                    field_name_str
                ));
            }

            // Check if encrypted index already exists for this field
            let index_exists = schema
                .encrypted_indexes
                .iter()
                .any(|idx| idx.field_name == field_name_str);
            if index_exists {
                return Err(format!(
                    "encrypted index already exists on this field. Field: {}",
                    field_name_str
                ));
            }

            // Create the encrypted index description
            let mut enc_idx = schema::EncryptedIndexDescription::new(&field_name_str);
            enc_idx.index_type = index_type;

            // Create a transaction
            let txn = database
                .new_txn(false)
                .await
                .map_err(|e| format!("failed to create transaction: {}", e))?;

            // Update the collection schema with the new encrypted index
            {
                let mut updated_schema = schema.clone();
                updated_schema.encrypted_indexes.push(enc_idx.clone());

                // Save the updated schema at /collection/id/{version_id}
                let collection_key =
                    storage::keys::systemstore::CollectionKey::new(&updated_schema.version_id);
                let schema_data = serde_json::to_vec(&updated_schema)
                    .map_err(|e| format!("failed to serialize schema: {}", e))?;

                let systemstore = txn
                    .systemstore()
                    .map_err(|e| format!("failed to get systemstore: {}", e))?;

                systemstore
                    .set(&collection_key.bytes(), &schema_data)
                    .await
                    .map_err(|e| format!("failed to save schema: {}", e))?;

                // Update the name → version_id mapping at /collection/name/{name}
                let name_key = storage::keys::systemstore::CollectionNameKey::new(&collection_name_str);
                systemstore
                    .set(&name_key.bytes(), updated_schema.version_id.as_bytes())
                    .await
                    .map_err(|e| format!("failed to save name mapping: {}", e))?;
            }

            // Commit the transaction
            txn.commit()
                .await
                .map_err(|e| format!("failed to commit: {}", e))?;

            // Reload the collection cache
            database
                .reload_cache()
                .await
                .map_err(|e| format!("failed to reload cache: {}", e))?;

            // Return the created encrypted index description
            let json = serde_json::to_string(&enc_idx)
                .map_err(|e| format!("failed to serialize result: {}", e))?;

            Ok(json)
        })
    }
}

/// Deletes the encrypted index on field `field_name` of collection
/// `collection_name`.
///
/// # Safety
///
/// Every string must be null or a valid null-terminated string.
#[no_mangle]
pub unsafe extern "C" fn defra_encrypted_index_delete(
    node_ptr: usize,
    collection_name: *const c_char,
    field_name: *const c_char,
    identity_ptr: usize,
) -> DefraResult {
    ffi_entry! {
        let request = try_ffi!(Request::begin(node_ptr, identity_ptr, NodePermission::EncryptedIndexDelete));
        let node_ptr = try_ffi!(request.require_node("encrypted index delete"));
        let collection_name_str = go_string(collection_name);
        let field_name_str = go_string(field_name);
        let database = try_ffi!(request.database());

        ffi_async!(node_ptr, {
            // Get the collection
            let collection = database
                .get_collection(&collection_name_str)
                .map_err(|e| format!("failed to get collection: {}", e))?
                .ok_or_else(|| format!("collection '{}' not found", collection_name_str))?;

            let schema = collection.schema();

            // Check if encrypted index exists for this field
            let index_exists = schema
                .encrypted_indexes
                .iter()
                .any(|idx| idx.field_name == field_name_str);
            if !index_exists {
                return Err(format!(
                    "encrypted index does not exist on this field. Field: {}",
                    field_name_str
                ));
            }

            // Create a transaction
            let txn = database
                .new_txn(false)
                .await
                .map_err(|e| format!("failed to create transaction: {}", e))?;

            // Update the collection schema to remove the encrypted index
            {
                let mut updated_schema = schema.clone();
                updated_schema
                    .encrypted_indexes
                    .retain(|idx| idx.field_name != field_name_str);

                // Save the updated schema at /collection/id/{version_id}
                let collection_key =
                    storage::keys::systemstore::CollectionKey::new(&updated_schema.version_id);
                let schema_data = serde_json::to_vec(&updated_schema)
                    .map_err(|e| format!("failed to serialize schema: {}", e))?;

                let systemstore = txn
                    .systemstore()
                    .map_err(|e| format!("failed to get systemstore: {}", e))?;

                systemstore
                    .set(&collection_key.bytes(), &schema_data)
                    .await
                    .map_err(|e| format!("failed to save schema: {}", e))?;

                // Update the name → version_id mapping at /collection/name/{name}
                let name_key = storage::keys::systemstore::CollectionNameKey::new(&collection_name_str);
                systemstore
                    .set(&name_key.bytes(), updated_schema.version_id.as_bytes())
                    .await
                    .map_err(|e| format!("failed to save name mapping: {}", e))?;
            }

            // Commit the transaction
            txn.commit()
                .await
                .map_err(|e| format!("failed to commit: {}", e))?;

            // Reload the collection cache
            database
                .reload_cache()
                .await
                .map_err(|e| format!("failed to reload cache: {}", e))?;

            Ok("{}".to_string())
        })
    }
}
