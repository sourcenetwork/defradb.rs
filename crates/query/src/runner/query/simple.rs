//! Non-planner execution path for simple queries.

use acp::Identity;
use identity::Did;
use rapidhash::{HashSetExt, RapidHashSet};
use schema::CollectionVersion;
use serde_json::Value as JsonValue;
use std::sync::Arc;
use tracing::{debug, warn};

use crate::document::documents_to_plan_docs;
use crate::error::Result;
use crate::mapper::Select;
use crate::planner::index_selection::{
    estimate_select, filter_to_index_scan, select_best_index_with_estimates, IndexEstimates,
};
use crate::txn::TransactionRegistry;

use super::super::fetcher::FetcherWrapper;
use super::super::plan::{self, ScanSource};
use super::super::plan_drive;
use super::super::{DocFetcher, QueryRunner};

impl<F: DocFetcher + 'static, R: TransactionRegistry> QueryRunner<F, R> {
    /// Execute a simple query without nested selections.
    ///
    /// This is the optimized path that supports aggregations and grouping.
    pub(crate) async fn execute_simple_select(
        &self,
        select: &Select,
        fetcher: &dyn DocFetcher,
        collection: &Arc<CollectionVersion>,
        identity: Option<Did>,
    ) -> Result<JsonValue> {
        // Build document mapping first (needed for both paths)
        let mapping = plan::build_mapping(select, collection)?;

        // A fetcher-backed scan streams documents one at a time, so it honours
        // show_deleted, the scan filter, and a downstream limit without ever
        // materializing the collection. doc_ids and index-scan lookups fetch
        // specific documents rather than scanning, so they stay materialized.
        let source = if select.show_deleted {
            ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
        } else if let Some(ref doc_ids) = select.doc_ids {
            // Deduplicate doc_ids while preserving order (Go compatibility)
            let mut seen = RapidHashSet::new();
            let unique_ids: Vec<String> = doc_ids
                .iter()
                .filter(|id| seen.insert((*id).clone()))
                .cloned()
                .collect();
            let result = fetcher
                .get_by_ids(&select.collection_name, &unique_ids)
                .await?;
            let missing = result.missing_ids();
            if !missing.is_empty() {
                warn!(
                    collection = %select.collection_name,
                    missing_ids = ?missing,
                    requested_count = unique_ids.len(),
                    found_count = result.docs().len(),
                    "Some requested documents were not found"
                );
            }
            ScanSource::Docs(documents_to_plan_docs(&result.into_docs(), &mapping)?)
        } else if crate::plan::ScanNode::point_id_for(None, select.filter.as_ref()).is_some() {
            ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
        } else if let Some(ref filter) = select.filter {
            // Try to use an index if available
            if fetcher.supports_index_queries() && !collection.indexes.is_empty() {
                let estimates = estimate_select(fetcher, collection, select)
                    .await?
                    .map(|estimates| estimates.estimates)
                    .unwrap_or_else(IndexEstimates::default);
                if let Some(best_index) =
                    select_best_index_with_estimates(filter, &collection.indexes, &estimates)
                {
                    // Extract limit/offset for index optimization
                    let limit = select.limit.as_ref().and_then(|l| l.limit);
                    let offset = select.limit.as_ref().map(|l| l.offset).unwrap_or(0);
                    if let Some(params) = filter_to_index_scan(
                        filter,
                        best_index,
                        select.order_by.as_ref(),
                        &collection.fields,
                        limit,
                        offset,
                    ) {
                        debug!(
                            collection = %select.collection_name,
                            index = %params.index_name,
                            "Using index for query"
                        );
                        // Get doc IDs from index
                        let scan_result = fetcher
                            .get_by_index_scan(&select.collection_name, &params)
                            .await?;
                        // Fetch the actual documents by ID
                        let result = fetcher
                            .get_by_ids(&select.collection_name, scan_result.doc_ids())
                            .await?;
                        ScanSource::Docs(documents_to_plan_docs(&result.into_docs(), &mapping)?)
                    } else {
                        // Filter doesn't translate to index scan, stream the collection
                        ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
                    }
                } else {
                    // No suitable index found, stream the collection
                    ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
                }
            } else {
                // Fetcher doesn't support index queries or no indexes, stream the collection
                ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
            }
        } else {
            ScanSource::Fetcher(Arc::new(FetcherWrapper::new(fetcher)))
        };

        // Build ACP filter config when the collection is policy-backed.
        let app_read = self.app_read_check(identity.clone(), collection);
        let acp_filter = collection.policy.as_ref().map(|policy| plan::AcpFilter {
            acp: self.acp.clone(),
            identity: Identity::from(identity),
            policy_id: policy.id.clone(),
            resource_name: policy.resource_name.clone(),
        });

        // Build and execute the plan (ACP filter is inserted inside, after Select but before aggregates)
        let mut plan = plan::build_plan(
            select,
            source,
            mapping.clone(),
            collection,
            acp_filter,
            app_read,
            self.query_limits,
        )?;

        let outcome = async {
            plan.init().await?;
            plan.start().await?;

            let mut results = Vec::new();

            while plan.next().await? {
                let doc = plan.value();
                let json = self.doc_to_json(doc, &mapping)?;
                results.push(json);
            }

            Ok(results)
        }
        .await;

        let results = plan_drive::close_after(plan.as_mut(), outcome).await?;

        Ok(JsonValue::Array(results))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::doc_stream::DocStream;
    use crate::fetcher::{FetchByIdsResult, IndexScanResult};
    use crate::planner::index_selection::IndexScanParams;
    use crate::test_utils::MockFetcher;
    use async_trait::async_trait;
    use document::Document;
    use schema::{FieldDescription, FieldKind, IndexDescription, IndexedFieldDescription};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct PointOnlyFetcher {
        inner: MockFetcher,
        reads: Arc<AtomicUsize>,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl DocFetcher for PointOnlyFetcher {
        async fn get_all(&self, _: &str) -> Result<Vec<Document>> {
            panic!("unexpected full scan")
        }
        async fn stream_all_with_deleted(&self, _: &str, _: bool) -> Result<Box<dyn DocStream>> {
            panic!("unexpected stream")
        }
        async fn stream_by_doc_short_ids(
            &self,
            _: &str,
            _: &[u64],
            _: bool,
        ) -> Result<Box<dyn DocStream>> {
            panic!("unexpected short-ID scan")
        }
        async fn get_by_field_value(&self, _: &str, _: &str, _: &str) -> Result<Vec<Document>> {
            panic!("unexpected field lookup")
        }
        async fn get_by_ids(&self, collection: &str, ids: &[String]) -> Result<FetchByIdsResult> {
            assert_eq!(ids.len(), 1);
            self.reads.fetch_add(1, Ordering::Relaxed);
            self.inner.get_by_ids(collection, ids).await
        }
        fn supports_index_queries(&self) -> bool {
            true
        }
        async fn get_by_index_scan(&self, _: &str, _: &IndexScanParams) -> Result<IndexScanResult> {
            panic!("exact ID used a scope index")
        }
        async fn estimate_index_scan(
            &self,
            _: &str,
            _: &IndexScanParams,
            _: u64,
        ) -> Result<Option<u64>> {
            panic!("exact ID estimated indexes")
        }
    }

    #[tokio::test]
    async fn ordinary_scoped_id_queries_seek_without_index_prefetch() {
        let id = "bae-47bd7c29-69cc-5b8a-856f-caaa93d9ace0";
        let inner = MockFetcher::new();
        inner.add_doc(
            "Note",
            Document::from_json_str(&format!(r#"{{"_docID":"{id}","owner":"shared"}}"#)).unwrap(),
        );
        let reads = Arc::new(AtomicUsize::new(0));
        let fetcher = PointOnlyFetcher {
            inner,
            reads: reads.clone(),
        };
        let mut collection = CollectionVersion::new(
            "Note",
            "v1",
            "notes",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "owner", FieldKind::string()),
            ],
        );
        collection.indexes.push(IndexDescription {
            id: 1,
            name: "owner_idx".into(),
            unique: false,
            kind: None,
            auto_generated: false,
            fields: vec![IndexedFieldDescription {
                name: "owner".into(),
                descending: false,
            }],
        });
        let runner = QueryRunner::new(fetcher, vec![collection]);
        let data = runner.execute_query(&format!(r#"{{
            matching: Note(filter: {{_docID: {{_eq: "{id}"}}, owner: {{_eq: "shared"}}}}) {{ _docID owner }}
            rejected: Note(filter: {{_docID: {{_eq: "{id}"}}, owner: {{_eq: "other"}}}}) {{ _docID }}
            missing: Note(filter: {{_docID: {{_eq: "missing"}}, owner: {{_eq: "shared"}}}}) {{ _docID }}
        }}"#)).await.unwrap();
        assert_eq!(data["matching"][0]["_docID"], id);
        assert_eq!(data["rejected"], serde_json::json!([]));
        assert_eq!(data["missing"], serde_json::json!([]));
        assert_eq!(reads.load(Ordering::Relaxed), 3);
    }
}
