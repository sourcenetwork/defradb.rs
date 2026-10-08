use std::sync::Arc;

use document::{Document, WritePreparation};
use rapidhash::RapidHashSet;
use storage::corekv::Store;

use crate::{block::builder::DocStorageIdentity, database::DB};
use defra_core::encryption::EncryptionConfig;

pub(crate) async fn prepare_keys<S: Store + 'static>(
    db: &DB<S>,
    doc: &mut Document,
    identity: DocStorageIdentity,
    modified_fields: Option<&RapidHashSet<String>>,
    config: Option<&EncryptionConfig>,
) -> query::error::Result<()> {
    prepare_keys_with_stores(db, doc, identity, modified_fields, config, None, None).await
}

#[allow(clippy::too_many_arguments)]
pub(crate) async fn prepare_keys_with_stores<S: Store + 'static>(
    db: &DB<S>,
    doc: &mut Document,
    identity: DocStorageIdentity,
    modified_fields: Option<&RapidHashSet<String>>,
    config: Option<&EncryptionConfig>,
    stores: Option<(&datastore::NamespaceView, &datastore::NamespaceView)>,
    prior: Option<&WritePreparation>,
) -> query::error::Result<()> {
    if let Some(prepared) = doc.write_preparation() {
        if prepared.collection_short_id != identity.collection_short_id
            || prepared.doc_short_id != identity.doc_short_id
        {
            return Err(query::error::QueryError::execution(
                "prepared write identity changed",
            ));
        }
        return Ok(());
    }
    let Some(kms) = db.kms() else { return Ok(()) };
    let mut prepared = WritePreparation::new(identity.collection_short_id, identity.doc_short_id);
    let fields: Vec<_> = doc
        .values()
        .keys()
        .filter(|field| {
            field.as_str() != "_docID"
                && modified_fields.is_none_or(|fields| fields.contains(*field))
        })
        .cloned()
        .collect();
    let mut scopes: Vec<_> = fields
        .iter()
        .filter_map(|field| {
            config
                .filter(|config| config.should_encrypt_field(field))
                .map(|config| {
                    (
                        field.clone(),
                        config
                            .should_encrypt_individual_field(field)
                            .then(|| field.clone()),
                    )
                })
        })
        .collect();
    if modified_fields.is_some() {
        let all_fields: Vec<_> = doc.values().keys().cloned().collect();
        let policy = if let Some(prior) = prior {
            (prior.encrypt_doc, prior.encrypted_fields.clone())
        } else if let Some((blockstore, headstore)) = stores {
            crate::block::builder::inherited_encryption_policy(
                blockstore,
                headstore,
                identity,
                &all_fields,
            )
            .await
            .map_err(query::error::QueryError::execution)?
        } else {
            let txn = db
                .new_txn(true)
                .await
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            let policy = {
                let blockstore = txn
                    .blockstore()
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                let headstore = txn
                    .headstore()
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                crate::block::builder::inherited_encryption_policy(
                    &blockstore,
                    &headstore,
                    identity,
                    &all_fields,
                )
                .await
                .map_err(query::error::QueryError::execution)?
            };
            txn.discard()
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            policy
        };
        prepared.encrypt_doc = policy.0;
        prepared.encrypted_fields = policy.1;
    }
    prepared.encrypt_doc |= config.is_some_and(|config| config.encrypt_doc);
    for field in fields {
        if prepared.encrypt_doc
            && !prepared.encrypted_fields.contains(&field)
            && !scopes.iter().any(|(explicit, _)| explicit == &field)
        {
            scopes.push((field, None));
        }
    }
    for (field, key_field) in scopes {
        let scope = kms::KeyScope::Document {
            doc_id: hex::encode(identity.doc_ref_bytes()),
            field: key_field,
        };
        let (cid, key) = kms
            .generate_key(&kms::RequestContext::anonymous(), scope)
            .await
            .map_err(|e| query::error::QueryError::execution(format!("kms generate_key: {e}")))?;
        prepared.insert_key(field, cid, key);
    }
    doc.set_write_preparation(Arc::new(prepared));
    Ok(())
}

pub(crate) fn prepared_identity(doc: &Document) -> Option<DocStorageIdentity> {
    doc.write_preparation().map(|prepared| {
        DocStorageIdentity::new(prepared.collection_short_id, prepared.doc_short_id)
    })
}

impl<S: Store + 'static> crate::AutoCommitMutator<S> {
    pub(super) async fn prepare_request_write(
        &self,
        collection_name: &str,
        mut doc: Document,
        mut modified_fields: Option<RapidHashSet<String>>,
        config: Option<EncryptionConfig>,
    ) -> query::error::Result<Document> {
        let collection = self.get_collection_or_err(collection_name)?;
        if let Some(fields) = modified_fields.as_mut() {
            for embedding in &collection.schema().vector_embeddings {
                if !fields.contains(&embedding.field_name)
                    && embedding
                        .fields
                        .iter()
                        .any(|source| fields.contains(source))
                {
                    fields.insert(embedding.field_name.clone());
                    if doc.get(&embedding.field_name).is_none() {
                        doc.set(&embedding.field_name, document::NormalValue::Null);
                    }
                }
            }
        }
        let prior = doc.write_preparation().cloned();
        let identity = if let Some(prior) = &prior {
            doc.clear_write_preparation();
            DocStorageIdentity::new(prior.collection_short_id, prior.doc_short_id)
        } else if modified_fields.is_some() {
            let txn = self
                .db
                .new_txn(true)
                .await
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            let identity = {
                let systemstore = txn
                    .systemstore()
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                let doc_id = doc.id().ok_or_else(|| {
                    query::error::QueryError::execution("update requires a document ID")
                })?;
                let (short_id, canonical) = collection
                    .require_doc_identity(&systemstore, doc_id)
                    .await
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
                doc.set_id(canonical);
                DocStorageIdentity::new(collection.resolved_root_id(), short_id)
            };
            txn.discard()
                .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
            identity
        } else {
            crate::search::set_embedding(
                &collection.schema().vector_embeddings,
                &mut doc,
                true,
                None,
                &self.db.options().embedding_config(),
            )
            .await
            .map_err(|e| query::error::QueryError::execution(format!("embedding error: {e}")))?;
            DocStorageIdentity::new(
                collection.resolved_root_id(),
                self.db
                    .next_doc_short_id()
                    .await
                    .map_err(|e| query::error::QueryError::execution(e.to_string()))?,
            )
        };
        prepare_keys_with_stores(
            &self.db,
            &mut doc,
            identity,
            modified_fields.as_ref(),
            config.as_ref(),
            None,
            prior.as_deref(),
        )
        .await?;
        Ok(doc)
    }
}
