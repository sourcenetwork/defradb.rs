use defra_core::RequestContext;
use identity::Did;
use rapidhash::RapidHashSet;

use super::turn::Turned;
use super::{DocFetcher, QueryRunner};
use crate::error::{QueryError, Result};
use crate::executor::QueryRequest;
use crate::owners::{DocumentOwnerObservation, MAX_OWNER_OBSERVATIONS};
use crate::prepared::PreparedInputMemo;
use crate::txn::{txn_job, TransactionHandle, TransactionRegistry};

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    pub(super) async fn observe_owners(
        &self,
        context: RequestContext,
        handle: &TransactionHandle,
        collection: &str,
        doc_ids: &[String],
        identity: Option<identity::Did>,
        node_authorized: bool,
    ) -> Result<Vec<DocumentOwnerObservation>> {
        let mut name = collection.chars();
        if !name
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
            || !name.all(|c| c.is_ascii_alphanumeric() || c == '_')
            || collection.len() > 1024
            || doc_ids.is_empty()
            || doc_ids.len() > MAX_OWNER_OBSERVATIONS
            || doc_ids.iter().any(|id| id.is_empty() || id.len() > 1024)
        {
            return Err(QueryError::parse(format!(
                "choose a collection and between 1 and {MAX_OWNER_OBSERVATIONS} known document IDs",
            )));
        }
        let context = if node_authorized {
            context.with_dac_bypass(false)
        } else {
            context.or_ambient()
        };
        let nac_identity =
            if node_authorized {
                Some(
                    Did::new(context.identity().ok_or_else(|| {
                        QueryError::execution("transaction principal is required")
                    })?)
                    .map_err(|error| QueryError::execution(error.to_string()))?,
                )
            } else {
                None
            };
        let identity = match identity {
            Some(identity) => Some(identity),
            None => context
                .identity()
                .map(Did::new)
                .transpose()
                .map_err(|error| QueryError::execution(error.to_string()))?,
        };
        let ids = serde_json::to_string(doc_ids)
            .map_err(|error| QueryError::execution(error.to_string()))?;
        let request = QueryRequest::new(format!("{{ {collection}(docID: {ids}) {{ _docID }} }}"))
            .with_identity(identity)
            .with_context(context.clone());
        let runner = self.for_query(&request);
        let collection = collection.to_owned();
        let doc_ids = doc_ids.to_vec();
        let job = txn_job(move |turn| {
            Box::pin(async move {
                let ctx = turn
                    .context()
                    .map_err(|error| QueryError::execution(error.to_string()))?;
                let acp = ctx
                    .doc_fetcher()
                    .transaction_acp(runner.acp.clone())
                    .await?
                    .ok_or_else(|| {
                        QueryError::execution(
                            "transaction-bound authorization is unavailable for owner observations",
                        )
                    })?;
                let provider = ctx.collection_provider().ok_or_else(|| {
                    QueryError::execution("transaction schema observation is unavailable")
                })?;
                let definition = provider
                    .get_collection(&collection)
                    .await?
                    .ok_or_else(|| QueryError::collection_not_found(&collection))?;
                drop(ctx);
                // The ordinary read path owns node, application, document and live
                // membership gates. Both reads stay on this one transaction turn.
                let response = match runner.run_turn_with_nac_identity(turn, request, PreparedInputMemo::default(), None, nac_identity).await {
                Turned::Done(response) => response,
                Turned::NeedsInput(_, _) => return Err(QueryError::execution("owner observation requires an external input; use a local authorization snapshot")),
            };
                if let Some(error) = response.errors.first() {
                    return Err(QueryError::execution(error.message.clone()));
                }
                let rows = response
                    .data
                    .as_ref()
                    .and_then(|data| data.get(&collection))
                    .and_then(|rows| rows.as_array());
                // Node denial uses an empty data object on the normal read path.
                let visible: RapidHashSet<&str> = rows
                    .into_iter()
                    .flatten()
                    .filter_map(|row| row.get("_docID").and_then(|id| id.as_str()))
                    .collect();
                let mut result = Vec::with_capacity(doc_ids.len());
                for doc_id in doc_ids {
                    let readable = visible.contains(doc_id.as_str());
                    let owner = if readable {
                        if let Some(policy) = &definition.policy {
                            if acp
                                .is_doc_registered(&policy.id, &policy.resource_name, &doc_id)
                                .await
                                .map_err(|error| QueryError::execution(error.to_string()))?
                            {
                                acp.get_doc_owner(&policy.id, &policy.resource_name, &doc_id)
                                    .await
                                    .map_err(|error| QueryError::execution(error.to_string()))?
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    result.push(DocumentOwnerObservation {
                        doc_id,
                        visible: readable,
                        owner,
                    });
                }
                Ok(result)
            })
        });
        self.registry
            .run_as(&context, handle, self.deadline(), job)
            .await
            .map_err(|error| QueryError::execution(error.to_string()))?
    }
}
