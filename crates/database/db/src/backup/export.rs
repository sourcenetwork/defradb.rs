use std::sync::Arc;

use serde_json::{Map, Value as JsonValue};

use storage::corekv::Store;

use super::classify_schema_fields;
use crate::DB;

/// [`export_database_as`] in [`defra_core::RequestContext::ambient`].
pub async fn export_database<S: Store>(
    database: &Arc<DB<S>>,
    runner: &Arc<dyn query::QueryExecutor>,
    collections: &[String],
    pretty: bool,
) -> Result<String, String> {
    export_database_as(
        &defra_core::RequestContext::ambient(),
        database,
        runner,
        collections,
        pretty,
    )
    .await
}

/// Export database contents to a JSON string.
///
/// Three-phase export:
/// - Phase 1: Query all docs, compute initial _docIDNew (including FK fields)
/// - Phase 2: Remap FK values to _docIDNew and recompute _docIDNew
/// - Phase 3: Build export output
///
/// If `collections` is empty, all collections are exported.
/// Returns the JSON string (not written to file: the caller handles I/O). The
/// documents are read as the identity acting in `context`.
pub async fn export_database_as<S: Store>(
    context: &defra_core::RequestContext,
    database: &Arc<DB<S>>,
    runner: &Arc<dyn query::QueryExecutor>,
    collections: &[String],
    pretty: bool,
) -> Result<String, String> {
    let identity = context
        .identity()
        .map(identity::Did::new)
        .transpose()
        .map_err(|e| format!("invalid identity: {e}"))?;
    let all_names = database
        .list_collections()
        .map_err(|e| format!("failed to list collections: {}", e))?;

    let filtered_names: Vec<String> = if collections.is_empty() {
        all_names
    } else {
        for name in collections {
            if !all_names.contains(name) {
                return Err(format!(
                    "failed to get collection: collection not found. Name: {}",
                    name
                ));
            }
        }
        collections.to_vec()
    };
    let filtered_names = with_chunk_collections(database, filtered_names)?;

    // Sort collections by collection_id (CID) to match Go's ordering.
    let mut name_cid_pairs: Vec<(String, String)> = Vec::new();
    for name in &filtered_names {
        let col = database
            .get_collection(name)
            .map_err(|e| format!("failed to get collection '{}': {}", name, e))?
            .ok_or_else(|| {
                format!(
                    "failed to get collection: collection not found. Name: {}",
                    name
                )
            })?;
        name_cid_pairs.push((name.clone(), col.schema().collection_id.clone()));
    }
    name_cid_pairs.sort_by(|a, b| a.1.cmp(&b.1));
    let collection_names: Vec<String> = name_cid_pairs.into_iter().map(|(n, _)| n).collect();

    struct DocEntry {
        doc_map: Map<String, JsonValue>,
    }

    struct CollectionData {
        name: String,
        docs: Vec<DocEntry>,
    }

    let mut all_collections: Vec<CollectionData> = Vec::new();

    // Phase 1: Query all docs
    for name in &collection_names {
        let collection = database
            .get_collection(name)
            .map_err(|e| format!("failed to get collection '{}': {}", name, e))?
            .ok_or_else(|| {
                format!(
                    "failed to get collection: collection not found. Name: {}",
                    name
                )
            })?;

        let schema = collection.schema().clone();
        // Import rebuilds `@chunk` collections from the parents' text, so
        // only their vectors are kept: a restore needs no model for them.
        if schema.is_generated() {
            if !schema.vector_embeddings.is_empty() {
                let docs = chunk_vectors(runner, context, &identity, name, &schema).await?;
                all_collections.push(CollectionData {
                    name: name.clone(),
                    docs: docs
                        .into_iter()
                        .map(|doc_map| DocEntry { doc_map })
                        .collect(),
                });
            }
            continue;
        }
        let fields = classify_schema_fields(&schema);

        let mut query_parts = vec!["_docID".to_string()];
        let mut relation_field_names: Vec<String> = Vec::new();
        let mut fk_field_names: Vec<String> = Vec::new();
        let mut self_ref_candidate_fks: Vec<String> = Vec::new();

        for field in &fields {
            if field.is_relation {
                if !field.is_array && field.is_primary {
                    query_parts.push(format!("{} {{ _docID }}", field.name));
                    relation_field_names.push(field.name.clone());
                    let fk_name = format!("_{}ID", field.name);
                    fk_field_names.push(fk_name.clone());
                    if field.is_self_ref {
                        self_ref_candidate_fks.push(fk_name);
                    }
                }
            } else {
                query_parts.push(field.name.clone());
            }
        }

        let query = format!("{{ {} {{ {} }} }}", name, query_parts.join(" "));

        let request = query::QueryRequest::new(query)
            .with_context(context.clone())
            .with_identity(identity.clone());
        let response = runner.execute(request).await;

        if !response.errors.is_empty() {
            let errs: Vec<String> = response.errors.iter().map(|e| e.message.clone()).collect();
            return Err(format!("query errors for '{}': {}", name, errs.join("; ")));
        }

        let response_json = serde_json::to_value(&response.data)
            .map_err(|e| format!("failed to serialize response: {}", e))?;

        let docs = response_json
            .get(name.as_str())
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut doc_entries = Vec::new();
        for doc in docs {
            let mut doc_map = match doc.as_object() {
                Some(m) => m.clone(),
                None => continue,
            };

            // Transform relation fields: {author: {_docID: "..."}} → {_authorID: "..."}
            for rel_name in &relation_field_names {
                if let Some(related) = doc_map.remove(rel_name) {
                    if related.is_null() {
                        continue;
                    }
                    if let Some(related_id) = related.get("_docID") {
                        let fk_name = format!("_{}ID", rel_name);
                        doc_map.insert(fk_name, related_id.clone());
                    }
                }
            }

            let own_doc_id = doc_map
                .get("_docID")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            // Detect document-level self-references
            let mut self_ref_excludes: Vec<String> = Vec::new();
            for fk_name in &self_ref_candidate_fks {
                if let Some(fk_value) = doc_map.get(fk_name).and_then(|v| v.as_str()) {
                    if fk_value == own_doc_id {
                        self_ref_excludes.push(fk_name.clone());
                    }
                }
            }

            // Identity is genesis-CID-derived and cannot be recomputed from
            // field values: `_docIDNew` is the document's current identity
            // (Go v1.0.0 backup format), and the import registers it as an
            // alias of the freshly derived identity.
            doc_map.insert(
                "_docIDNew".to_string(),
                JsonValue::String(own_doc_id.clone()),
            );

            doc_map.retain(|_, v| !v.is_null());

            doc_entries.push(DocEntry { doc_map });
        }

        all_collections.push(CollectionData {
            name: name.clone(),
            docs: doc_entries,
        });
    }

    // Phase 2: Build export output
    let mut collection_json_parts: Vec<String> = Vec::new();
    for col_data in all_collections {
        let export_docs: Vec<JsonValue> = col_data
            .docs
            .into_iter()
            .map(|entry| JsonValue::Object(entry.doc_map))
            .collect();
        let docs_json = serde_json::to_string(&export_docs)
            .map_err(|e| format!("failed to serialize docs: {}", e))?;
        collection_json_parts.push(format!("\"{}\":{}", col_data.name, docs_json));
    }

    let json_output = if pretty {
        let mut pretty_parts: Vec<String> = Vec::new();
        for part in &collection_json_parts {
            let val: JsonValue = serde_json::from_str(&format!("{{{}}}", part))
                .map_err(|e| format!("failed to parse for pretty print: {}", e))?;
            let pretty_str = serde_json::to_string_pretty(&val)
                .map_err(|e| format!("failed to pretty print: {}", e))?;
            let inner = pretty_str.trim().strip_prefix('{').unwrap_or(&pretty_str);
            let inner = inner.strip_suffix('}').unwrap_or(inner);
            pretty_parts.push(inner.trim_end().to_string());
        }
        format!("{{\n{}\n}}", pretty_parts.join(",\n"))
    } else {
        format!("{{{}}}", collection_json_parts.join(","))
    };

    Ok(json_output)
}

/// `names` with the chunk collection of every chunked field they hold, so a
/// parent's export keeps its chunk vectors.
fn with_chunk_collections<S: Store>(
    database: &Arc<DB<S>>,
    mut names: Vec<String>,
) -> Result<Vec<String>, String> {
    let mut chunk_collections = Vec::new();
    for name in &names {
        let Some(collection) = database
            .get_collection(name)
            .map_err(|e| format!("failed to get collection '{}': {}", name, e))?
        else {
            continue;
        };
        let schema = collection.schema();
        for chunked in &schema.chunked_fields {
            chunk_collections.push(chunked.collection(&schema.name));
        }
    }
    for name in chunk_collections {
        if !names.contains(&name) {
            names.push(name);
        }
    }
    Ok(names)
}

/// Each current chunk's text with its vectors, once per text.
async fn chunk_vectors(
    runner: &Arc<dyn query::QueryExecutor>,
    context: &defra_core::RequestContext,
    identity: &Option<identity::Did>,
    name: &str,
    schema: &schema::CollectionVersion,
) -> Result<Vec<Map<String, JsonValue>>, String> {
    let vectors: Vec<&str> = schema
        .vector_embeddings
        .iter()
        .map(|embedding| embedding.field_name.as_str())
        .collect();
    let query = format!(
        "{{ {} {{ {} {} }} }}",
        name,
        schema::CHUNK_TEXT_FIELD,
        vectors.join(" ")
    );
    let request = query::QueryRequest::new(query)
        .with_context(context.clone())
        .with_identity(identity.clone());
    let response = runner.execute(request).await;
    if !response.errors.is_empty() {
        let errs: Vec<String> = response.errors.iter().map(|e| e.message.clone()).collect();
        return Err(format!("query errors for '{}': {}", name, errs.join("; ")));
    }
    let response_json = serde_json::to_value(&response.data)
        .map_err(|e| format!("failed to serialize response: {}", e))?;
    let mut seen = rapidhash::RapidHashSet::default();
    let mut docs = Vec::new();
    for doc in response_json
        .get(name)
        .and_then(JsonValue::as_array)
        .into_iter()
        .flatten()
    {
        let Some(mut doc_map) = doc.as_object().cloned() else {
            continue;
        };
        doc_map.retain(|_, v| !v.is_null());
        let text = doc_map
            .get(schema::CHUNK_TEXT_FIELD)
            .and_then(JsonValue::as_str);
        if doc_map.len() > 1 && text.is_some_and(|text| seen.insert(text.to_string())) {
            docs.push(doc_map);
        }
    }
    // A stable order keeps two exports of the same data byte for byte equal.
    docs.sort_by(|a, b| {
        let text = |doc: &Map<String, JsonValue>| {
            doc.get(schema::CHUNK_TEXT_FIELD)
                .and_then(JsonValue::as_str)
                .unwrap_or("")
                .to_string()
        };
        text(a).cmp(&text(b))
    });
    Ok(docs)
}
