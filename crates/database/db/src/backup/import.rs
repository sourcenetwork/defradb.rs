use rapidhash::{RapidHashMap, RapidHashSet};
use std::sync::Arc;

use acp::nac::NodePermission;
use document::Document;
use query::documents::{self, AddOptions};
use query::QueryRequest;
use serde_json::{Map, Value as JsonValue};

use storage::corekv::Store;

use super::alias::{self, Saved, ALIAS_TAKEN};
use super::batch::{Batch, Gathered};
use super::classify_schema_fields;
use super::host::{ImportHost, RunnerHost};
use crate::DB;

/// Statistics from a backup import operation.
#[derive(Debug, Clone, Default)]
pub struct ImportStats {
    pub documents_imported: u64,
    pub collections_affected: Vec<String>,
}

/// A relation field whose target could not be resolved when its document
/// was imported (forward reference); applied after all imports.
struct PendingRelation {
    collection_name: String,
    doc_id: String,
    fk_name: String,
    imported_doc_id: String,
}

/// What the import has saved so far.
#[derive(Default)]
struct Imported {
    documents: u64,
    collections: RapidHashSet<String>,
    /// The file's IDs, each mapped to the new ID of the document it names.
    doc_ids: RapidHashMap<String, String>,
    relations: Vec<PendingRelation>,
}

/// The collection the documents being gathered belong to.
struct Target {
    name: String,
    short_id: u32,
}

/// [`import_database_as`] in [`defra_core::RequestContext::ambient`], with
/// every write sent to `runner`.
pub async fn import_database<S>(
    database: &Arc<DB<S>>,
    runner: &Arc<dyn query::QueryExecutor>,
    data: &str,
) -> Result<ImportStats, String>
where
    S: Store + 'static,
{
    import_database_as(
        &defra_core::RequestContext::ambient(),
        database,
        &RunnerHost::new(Arc::clone(runner)),
        data,
    )
    .await
}

/// Import documents from a JSON string into the database.
///
/// The data must be a JSON object mapping collection names to arrays of documents:
/// ```json
/// {
///     "User": [{"_docID": "...", "_docIDNew": "...", "name": "John", "age": 30}],
///     "Address": [{"_docID": "...", "_docIDNew": "...", "street": "...", "city": "..."}]
/// }
/// ```
///
/// Imported documents receive fresh genesis-CID-derived DocIDs. The file's
/// `_docID`/`_docIDNew` values are registered as aliases of the new
/// identity and relation fields are remapped through them, matching Go
/// v1.0.0's import behavior.
///
/// `context` is the caller, checked for `DocumentUpdate` node access. Every
/// create and relation patch is a request sent through `host`, the request
/// path of the edge the import came in by, so it is written exactly as a
/// create through that edge would be. Consecutive documents of a collection
/// are saved together, each save all or nothing; a refused save is redone
/// one document at a time, so a failure stops the import with every
/// document before the one refused kept.
pub async fn import_database_as<S>(
    context: &defra_core::RequestContext,
    database: &Arc<DB<S>>,
    host: &dyn ImportHost,
    data: &str,
) -> Result<ImportStats, String>
where
    S: Store + 'static,
{
    let parsed: JsonValue =
        serde_json::from_str(data).map_err(|e| format!("failed to parse JSON: {}", e))?;

    let root = match parsed.as_object() {
        Some(obj) => obj,
        None => {
            return Err(
                "invalid JSON: expected JSON object at root, got array or primitive".to_string(),
            )
        }
    };

    let (checked, caller) = (Arc::clone(database), context.clone());
    host.on_db(Box::new(move || {
        Box::pin(async move {
            checked
                .check_node_access_as(&caller, NodePermission::DocumentUpdate)
                .await
                .map_err(|e| e.to_string())
        })
    }))
    .await?;

    let mut imported = Imported::default();
    let mut batch = Batch::default();

    for (collection_name, docs_value) in root {
        let collection = database
            .get_collection(collection_name)
            .map_err(|e| format!("failed to get collection: {}", e))?
            .ok_or_else(|| {
                format!(
                    "failed to get collection: collection not found. Name: {}",
                    collection_name
                )
            })?;
        let target = Target {
            name: collection_name.clone(),
            short_id: collection.resolved_root_id(),
        };

        let schema = collection.schema();
        let fields = classify_schema_fields(schema);

        let relation_to_fk: Vec<(String, String)> = fields
            .iter()
            .filter(|f| f.is_relation && !f.is_array)
            .map(|f| (f.name.clone(), format!("_{}ID", f.name)))
            .collect();

        let docs = match docs_value.as_array() {
            Some(arr) => arr,
            None => {
                return Err(format!(
                    "invalid JSON: expected JSON array for collection '{}', got object",
                    collection_name
                ))
            }
        };

        // Pre-validate all documents' field names before creating any
        let valid_field_names: Vec<&str> = schema.fields.iter().map(|f| f.name.as_str()).collect();
        for doc in docs {
            if let Some(doc_obj) = doc.as_object() {
                for key in doc_obj.keys() {
                    if key == "_docID" || key == "_docIDNew" {
                        continue;
                    }
                    if !valid_field_names.contains(&key.as_str()) {
                        return Err(format!(
                            "failed to create document in '{}': the given field does not exist. Name: {}",
                            collection_name, key
                        ));
                    }
                }
            }
        }

        for doc in docs {
            let mut doc_map = match doc.as_object() {
                Some(m) => m.clone(),
                None => continue,
            };

            // The file's identities become aliases of the fresh
            // genesis-derived identity.
            let mut aliases: Vec<String> = Vec::new();
            for key in ["_docID", "_docIDNew"] {
                if let Some(JsonValue::String(alias)) = doc_map.remove(key) {
                    if !alias.is_empty() && !aliases.contains(&alias) {
                        aliases.push(alias);
                    }
                }
            }

            for (rel_name, fk_name) in &relation_to_fk {
                if let Some(value) = doc_map.remove(rel_name) {
                    if !value.is_null() {
                        doc_map.insert(fk_name.clone(), value);
                    }
                }
            }

            let targets: Vec<&str> = relation_to_fk
                .iter()
                .filter_map(|(_, fk_name)| doc_map.get(fk_name).and_then(JsonValue::as_str))
                .collect();
            if batch.must_save_before(&aliases, targets) {
                save(host, database, &target, &mut batch, &mut imported).await?;
            }

            // Resolve relation targets through already-imported identities;
            // unresolved targets are stripped and applied after the import
            // completes (forward references).
            let mut deferred: Vec<(String, String)> = Vec::new();
            for (_, fk_name) in &relation_to_fk {
                let Some(target_id) = doc_map
                    .get(fk_name)
                    .and_then(JsonValue::as_str)
                    .map(str::to_string)
                else {
                    continue;
                };
                match imported.doc_ids.get(&target_id) {
                    Some(new_id) => {
                        doc_map.insert(fk_name.clone(), JsonValue::String(new_id.clone()));
                    }
                    None => {
                        doc_map.remove(fk_name);
                        deferred.push((fk_name.clone(), target_id));
                    }
                }
            }

            // Checked here so a value the schema refuses fails with the
            // import's own words. The documents gathered before it are saved
            // first: the import keeps every document before the one refused.
            if let Err(e) =
                Document::from_map(doc_map.clone().into_iter().collect(), schema.clone())
            {
                save(host, database, &target, &mut batch, &mut imported).await?;
                return Err(format!(
                    "failed to create document in '{}': {}",
                    collection_name, e
                ));
            }

            batch.push(Gathered {
                fields: doc_map,
                aliases,
                deferred,
            });
            if batch.is_full() {
                save(host, database, &target, &mut batch, &mut imported).await?;
            }
        }
        save(host, database, &target, &mut batch, &mut imported).await?;
    }

    for pending in std::mem::take(&mut imported.relations) {
        let target = imported
            .doc_ids
            .get(&pending.imported_doc_id)
            .cloned()
            .unwrap_or(pending.imported_doc_id);
        let mut patch = Map::new();
        patch.insert(pending.fk_name, JsonValue::String(target));
        documents::update(
            |mutation| host.run(QueryRequest::new(mutation)),
            &pending.collection_name,
            &pending.doc_id,
            &JsonValue::Object(patch),
        )
        .await
        .map_err(|e| {
            format!(
                "failed to update relation fields in '{}': {}",
                pending.collection_name, e
            )
        })?;
    }

    Ok(ImportStats {
        documents_imported: imported.documents,
        collections_affected: imported.collections.into_iter().collect(),
    })
}

/// Saves the gathered documents of `target` in one request through `host`,
/// then registers their aliases in one transaction.
///
/// A batch that is refused saved nothing, so its documents are then saved
/// one at a time, in file order, up to the one refused: the import keeps
/// every document before the first it cannot import, as one create per
/// document would.
async fn save<S>(
    host: &dyn ImportHost,
    database: &Arc<DB<S>>,
    target: &Target,
    batch: &mut Batch,
    imported: &mut Imported,
) -> Result<(), String>
where
    S: Store + 'static,
{
    if batch.is_empty() {
        return Ok(());
    }
    let mut gathered = batch.take();
    let docs: Vec<JsonValue> = gathered
        .iter_mut()
        .map(|doc| JsonValue::Object(std::mem::take(&mut doc.fields)))
        .collect();
    match create(host, database, target, &gathered, &docs).await {
        Ok(ids) => record(host, database, target, gathered, ids, imported).await,
        Err(error) if gathered.len() == 1 => Err(error),
        Err(_) => {
            for (doc, fields) in gathered.into_iter().zip(&docs) {
                let one = vec![doc];
                let ids =
                    create(host, database, target, &one, std::slice::from_ref(fields)).await?;
                record(host, database, target, one, ids, imported).await?;
            }
            Ok(())
        }
    }
}

/// Checks that no file ID of `gathered` names a stored document, then
/// creates them, whose fields are `docs`, in one request: all or none.
/// Their new IDs, in order.
async fn create<S>(
    host: &dyn ImportHost,
    database: &Arc<DB<S>>,
    target: &Target,
    gathered: &[Gathered],
    docs: &[JsonValue],
) -> Result<Vec<String>, String>
where
    S: Store + 'static,
{
    // The batch never holds one ID twice (`Batch::must_save_before`).
    let aliases: Vec<String> = gathered
        .iter()
        .flat_map(|doc| doc.aliases.iter().cloned())
        .collect();
    if !aliases.is_empty() {
        host.on_db(alias::check_free(database, aliases)).await?;
    }
    documents::save_many(
        |request| host.run(request),
        &target.name,
        docs,
        &AddOptions::default(),
    )
    .await
    .map_err(|e| create_error(&target.name, e.to_string()))
}

/// Registers the aliases of the created documents `gathered`, whose new IDs
/// are `ids`, in one transaction, and notes them as imported.
///
/// An alias another writer took since the check refuses the whole
/// registration, and the documents stay saved. Each document's aliases are
/// then registered on their own, so every other one still answers to its
/// file ID, and the import fails with the first refusal.
async fn record<S>(
    host: &dyn ImportHost,
    database: &Arc<DB<S>>,
    target: &Target,
    gathered: Vec<Gathered>,
    ids: Vec<String>,
    imported: &mut Imported,
) -> Result<(), String>
where
    S: Store + 'static,
{
    let saved: Vec<Saved> = ids
        .iter()
        .zip(&gathered)
        .map(|(doc_id, doc)| Saved {
            doc_id: doc_id.clone(),
            aliases: doc.aliases.clone(),
        })
        .collect();
    let together = host
        .on_db(alias::register(database, target.short_id, saved.clone()))
        .await;
    if let Err(error) = together {
        if saved.len() == 1 {
            return Err(error);
        }
        let mut refused = None;
        for doc in saved {
            let alone = host
                .on_db(alias::register(database, target.short_id, vec![doc]))
                .await;
            if let Err(error) = alone {
                refused.get_or_insert(error);
            }
        }
        if let Some(error) = refused {
            return Err(error);
        }
    }

    for (doc_id, doc) in ids.into_iter().zip(gathered) {
        for alias in doc.aliases {
            imported.doc_ids.insert(alias, doc_id.clone());
        }
        for (fk_name, imported_doc_id) in doc.deferred {
            imported.relations.push(PendingRelation {
                collection_name: target.name.clone(),
                doc_id: doc_id.clone(),
                fk_name,
                imported_doc_id,
            });
        }
        imported.documents += 1;
    }
    imported.collections.insert(target.name.clone());
    Ok(())
}

/// A refused create in the import's words: an ID that names a stored
/// document is Go's alias error.
fn create_error(collection: &str, message: String) -> String {
    match message.contains("already exists") {
        true => ALIAS_TAKEN.to_string(),
        false => format!("failed to create document in '{collection}': {message}"),
    }
}
