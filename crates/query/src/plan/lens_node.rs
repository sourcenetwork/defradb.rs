//! LensNode applies a WASM lens transform to view query results.
//!
//! In the view pipeline: SourceQuery → LensNode (transform) → ViewNode (field mapping)
//! The LensNode streams its source through the view's lens: it reads a source
//! document only when the lens asks for one, so it never holds the whole
//! source, and the lens may still combine documents across all of it.

use std::sync::Arc;

use async_trait::async_trait;

use crate::document::DocumentMapping;
use crate::error::{QueryError, Result};
use crate::planner::{index_selection::CursorSeek, Doc, PlanNode};

use lens::{FeedStep, LensDoc, LensFeed, TransformId, TransformStore};

/// LensNode wraps a source plan and applies a lens transform to each document.
///
/// Documents from the source plan are converted to JSON objects, passed through
/// the WASM lens transform, and converted back to Doc format.
///
/// When the lens renames fields (e.g. `name` → `fullName`), the output JSON
/// has the target schema's field names. The `output_mapping` is used to convert
/// the transformed JSON back to Doc format, and should reflect the post-transform
/// field names (typically the view's target mapping).
pub struct LensNode {
    source: Box<dyn PlanNode>,
    source_mapping: DocumentMapping,
    output_mapping: DocumentMapping,
    lens_store: Arc<dyn TransformStore>,
    /// The view's one transform: a lens and its modules, under one ID.
    transform_id: TransformId,
    /// The running lens, from `start`.
    feed: Option<LensFeed>,
    /// The last document the lens yielded.
    current: Doc,
}

impl LensNode {
    pub fn new(
        source: Box<dyn PlanNode>,
        source_mapping: DocumentMapping,
        output_mapping: DocumentMapping,
        lens_store: Arc<dyn TransformStore>,
        transform_id: TransformId,
    ) -> Self {
        Self {
            source,
            source_mapping,
            output_mapping,
            lens_store,
            transform_id,
            feed: None,
            current: Doc::new(0),
        }
    }

    /// Convert a Doc to a LensDoc (JSON object) using the source mapping.
    fn doc_to_lens_doc(doc: &Doc, mapping: &DocumentMapping) -> LensDoc {
        let mut json_doc = serde_json::Map::new();
        for rk in &mapping.render_keys {
            if rk.index < doc.fields().len() {
                if let Some(ref value) = doc.fields()[rk.index] {
                    json_doc.insert(rk.key.clone(), value.clone());
                }
            }
        }
        json_doc
    }

    /// Convert a LensDoc back to a Doc using the source mapping.
    ///
    /// After transform, the lens may have added/removed/renamed fields.
    /// We create a Doc large enough to hold all fields and populate it
    /// by matching JSON keys to mapping render keys.
    fn lens_doc_to_doc(lens_doc: &LensDoc, mapping: &DocumentMapping) -> Doc {
        let mut doc = Doc::new(mapping.next_index());
        for rk in &mapping.render_keys {
            if let Some(value) = lens_doc.get(&rk.key) {
                doc.set(rk.index, value.clone());
            }
        }
        // Also set fields by underlying name for fields the transform added
        for (key, value) in lens_doc {
            if let Some(indexes) = mapping.indexes_of_name(key) {
                for &idx in indexes {
                    if doc.fields().get(idx).map(|f| f.is_none()).unwrap_or(true) {
                        doc.set(idx, value.clone());
                    }
                }
            }
        }
        doc
    }
}

fn lens_failure(error: lens::Error) -> QueryError {
    QueryError::execution(format!("lens transform failed: {}", error))
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for LensNode {
    async fn init(&mut self) -> Result<()> {
        self.source.init().await
    }

    async fn start(&mut self) -> Result<()> {
        self.source.start().await?;
        let feed = LensFeed::transform(self.lens_store.as_ref(), &self.transform_id)
            .map_err(lens_failure)?;
        self.feed = Some(feed);
        Ok(())
    }

    async fn next(&mut self) -> Result<bool> {
        let Some(feed) = self.feed.as_mut() else {
            return Ok(false);
        };
        loop {
            match feed.step().await {
                FeedStep::Output(Some(Ok(lens_doc))) => {
                    self.current = Self::lens_doc_to_doc(&lens_doc, &self.output_mapping);
                    return Ok(true);
                }
                FeedStep::Output(Some(Err(e))) => return Err(lens_failure(e)),
                FeedStep::Output(None) => return Ok(false),
                FeedStep::Input => {
                    let document = if self.source.next().await? {
                        Some(Self::doc_to_lens_doc(
                            self.source.value(),
                            &self.source_mapping,
                        ))
                    } else {
                        None
                    };
                    feed.push(document).map_err(lens_failure)?;
                }
            }
        }
    }

    fn value(&self) -> &Doc {
        &self.current
    }

    async fn close(&mut self) -> Result<()> {
        self.feed = None;
        self.source.close().await
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        Some(self.source.as_ref())
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.output_mapping
    }

    fn set_cursor_seek(&mut self, seek: CursorSeek) -> bool {
        self.source.set_cursor_seek(seek)
    }

    fn set_cursor_fetch_limit(&mut self, limit: u64) -> bool {
        self.source.set_cursor_fetch_limit(limit)
    }

    fn page_info(&self) -> Option<crate::plan::CursorPageInfo> {
        self.source.page_info()
    }

    fn kind(&self) -> &'static str {
        "lensNode"
    }

    fn explain_inner(&self) -> serde_json::Value {
        let child_explain = self.source.explain();
        // Wrap the source plan in selectTopNode (Go's view pipeline convention).
        // LensNode is the innermost view-related node when present.
        serde_json::json!({ "selectTopNode": child_explain })
    }

    fn explain_debug_inner(&self) -> serde_json::Value {
        let child_explain = self.source.explain_debug();
        serde_json::json!({ "selectTopNode": child_explain })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};

    use futures::executor::block_on;
    use futures::StreamExt;
    use lens::{LensConfig, LensDocResultStream, LensDocStream, LensModule, MemoryTransformStore};
    use rapidhash::RapidHashMap;
    use serde_json::json;

    use super::*;

    fn mapping() -> DocumentMapping {
        let mut mapping = DocumentMapping::new();
        mapping.add(0, "n");
        mapping.add_render_key(0, "n");
        mapping
    }

    /// A source of documents `{n: 0..count}` that counts its reads.
    struct CountingSource {
        count: usize,
        reads: Arc<AtomicUsize>,
        mapping: DocumentMapping,
        current: Doc,
    }

    #[cfg_attr(not(target_arch = "wasm32"), async_trait)]
    #[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
    impl PlanNode for CountingSource {
        async fn init(&mut self) -> Result<()> {
            Ok(())
        }

        async fn start(&mut self) -> Result<()> {
            Ok(())
        }

        async fn next(&mut self) -> Result<bool> {
            let n = self.reads.fetch_add(1, Ordering::SeqCst);
            if n >= self.count {
                return Ok(false);
            }
            self.current = Doc::new(1);
            self.current.set(0, json!(n));
            Ok(true)
        }

        fn value(&self) -> &Doc {
            &self.current
        }

        async fn close(&mut self) -> Result<()> {
            Ok(())
        }

        fn source(&self) -> Option<&dyn PlanNode> {
            None
        }

        fn document_map(&self) -> &DocumentMapping {
            &self.mapping
        }

        fn kind(&self) -> &'static str {
            "countingSource"
        }
    }

    fn lens_node(
        count: usize,
        store: Arc<dyn TransformStore>,
        id: &str,
    ) -> (LensNode, Arc<AtomicUsize>) {
        let reads = Arc::new(AtomicUsize::new(0));
        let source = CountingSource {
            count,
            reads: Arc::clone(&reads),
            mapping: mapping(),
            current: Doc::default(),
        };
        let node = LensNode::new(
            Box::new(source),
            mapping(),
            mapping(),
            store,
            TransformId::new(id),
        );
        (node, reads)
    }

    fn passthrough() -> Arc<dyn TransformStore> {
        let store = MemoryTransformStore::new();
        block_on(store.add_with_id(
            TransformId::new("passthrough"),
            LensConfig::new("v1", "v2", LensModule::default()),
        ))
        .unwrap();
        Arc::new(store)
    }

    #[test]
    fn a_lens_node_reads_its_source_as_the_lens_asks() {
        let (mut node, reads) = lens_node(50, passthrough(), "passthrough");
        block_on(node.start()).unwrap();
        assert_eq!(reads.load(Ordering::SeqCst), 0, "start reads nothing");

        for n in 0..50 {
            assert!(block_on(node.next()).unwrap());
            assert_eq!(node.value().fields()[0], Some(json!(n)));
            assert_eq!(
                reads.load(Ordering::SeqCst),
                n + 1,
                "one source document per output, never the whole source"
            );
        }
        assert!(!block_on(node.next()).unwrap());
    }

    /// Fails the document `{n: 1}` and passes every other.
    struct FailsOne;

    #[async_trait]
    impl TransformStore for FailsOne {
        fn new_isolated_store(&self) -> Arc<dyn TransformStore> {
            Arc::new(FailsOne)
        }

        async fn add(&self, _config: LensConfig) -> lens::Result<TransformId> {
            Err(lens::Error::RuntimeUnavailable)
        }

        async fn add_with_id(&self, _id: TransformId, _config: LensConfig) -> lens::Result<()> {
            Err(lens::Error::RuntimeUnavailable)
        }

        async fn list(&self) -> lens::Result<RapidHashMap<String, Vec<LensModule>>> {
            Ok(RapidHashMap::default())
        }

        fn transform(
            &self,
            _id: &TransformId,
            docs: LensDocStream,
        ) -> lens::Result<LensDocResultStream> {
            Ok(Box::pin(docs.map(|doc| {
                if doc.get("n") == Some(&json!(1)) {
                    Err(lens::Error::WasmExecution(
                        "WASM error: rejected".to_string(),
                    ))
                } else {
                    Ok(doc)
                }
            })))
        }

        fn inverse(
            &self,
            id: &TransformId,
            docs: LensDocStream,
        ) -> lens::Result<LensDocResultStream> {
            self.transform(id, docs)
        }

        fn has_transform(&self, _id: &TransformId) -> bool {
            true
        }

        async fn remove(&self, _id: &TransformId) -> lens::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn a_lens_failure_fails_the_read() {
        let (mut node, _) = lens_node(3, Arc::new(FailsOne), "fails-one");
        block_on(node.start()).unwrap();
        assert!(block_on(node.next()).unwrap());
        let failure = block_on(node.next()).unwrap_err();
        assert!(failure.to_string().contains("rejected"), "{failure}");
    }

    #[test]
    fn a_missing_lens_fails_the_start() {
        let (mut node, _) = lens_node(3, passthrough(), "missing");
        assert!(block_on(node.start()).is_err());
    }
}
