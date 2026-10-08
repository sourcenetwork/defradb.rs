use super::*;

use crate::event::arrivals::sequence_on_commit;
use crate::write::create::{create_documents, TxnStores};

impl<S: Store + 'static> AutoCommitMutator<S> {
    pub(super) async fn create_many_impl(
        &self,
        collection_name: &str,
        docs: Vec<Document>,
    ) -> query::error::Result<Vec<CreateResult>> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }

        self.db
            .check_node_access(None, acp::nac::NodePermission::DocumentUpdate)
            .await
            .map_err(|e| query::error::QueryError::permission_denied(e.to_string()))?;

        let (_collection_guard, collection) = self.guarded_collection(collection_name).await?;
        let index_manager = IndexManager::from_indexes(
            collection.resolved_root_id(),
            collection.schema(),
            collection.write_indexes(),
        )
        .map_err(|e| {
            query::error::QueryError::execution(format!(
                "failed to create index manager for collection '{}': {}",
                collection_name, e
            ))
        })?;

        // No per-doc write guard for creates: the DocID is derived from the
        // genesis block inside the txn, so no identity exists to guard yet.
        // The DocID-mapping duplicate check is the gate.
        let mut txn = self.new_mutation_txn().await?;
        sequence_on_commit(&mut txn, &self.db, collection.resolved_root_id())
            .map_err(|e| query::error::QueryError::execution(e.to_string()))?;
        let result = match TxnStores::of(&txn) {
            Ok(stores) => {
                create_documents(
                    &self.db,
                    &stores,
                    collection_name,
                    &collection,
                    &index_manager,
                    docs,
                )
                .await
            }
            Err(error) => Err(error),
        };
        let created = self
            .finish_mutation(txn, result, collection_name, "create")
            .await?;

        let mut results = Vec::with_capacity(created.len());
        for doc in created {
            let doc_id = doc.doc_id.to_string();
            self.register_created_doc_with_acp(&collection, &doc_id)
                .await?;
            self.emit_update_events(
                &collection,
                &doc_id,
                doc.cid,
                doc.block.clone(),
                doc.collection_block.clone(),
            );

            let mut result = CreateResult::with_commit(doc.doc_id, doc.doc, doc.cid, doc.block);
            if let Some((col_cid, col_bytes)) = doc.collection_block {
                result.broadcast_cid = Some(col_cid);
                result.broadcast_block = Some(col_bytes);
            }
            results.push(result);
        }
        Ok(results)
    }
}
