//! PermissionFilterNode for ACP-based document filtering
//!
//! This node wraps a source node and filters out documents that the
//! identity context doesn't have permission to read.

use std::sync::Arc;

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use defra_core::thread_bounds::MaybeBoxFuture;
use futures::{stream::FuturesOrdered, FutureExt, StreamExt};
use identity::Did;
use sync_wrapper::SyncWrapper;

use crate::access_hooks::read::app_allows;
use crate::access_hooks::AppReadCheck;
use crate::document::DocumentMapping;
use crate::error::Result;
use crate::planner::{index_selection::CursorSeek, Doc, PlanNode};
use crate::txn::AcpRegistrationProjection;

// Keep enough remote checks moving to hide latency without letting one query
// exhaust the ACP provider's connection pool.
const MAX_IN_FLIGHT_PERMISSION_CHECKS: usize = 16;

type PermissionCheck = MaybeBoxFuture<'static, Result<(Doc, bool)>>;

/// Who reads, through which ACP, under which policy.
pub struct AcpRead {
    pub acp: Arc<dyn DocumentACP>,
    pub identity: Identity,
    /// The request's DAC bypass (`RequestContext::dac_bypass`).
    pub dac_bypass: bool,
    pub policy_id: String,
    pub resource_name: String,
}

#[derive(Clone)]
struct AcpReadCheck {
    acp: Arc<dyn DocumentACP>,
    identity: Arc<Identity>,
    dac_bypass: bool,
    policy_id: Arc<str>,
    resource_name: Arc<str>,
    retry_inputs: Option<crate::prepared::PreparedInputMemo>,
    projection: Option<Arc<AcpRegistrationProjection>>,
}

/// PermissionFilterNode filters documents based on ACP permissions.
///
/// This node wraps a source node and only yields documents that the
/// identity has read permission for. Documents created without ACP
/// (public/unregistered) pass through.
pub struct PermissionFilterNode {
    /// Source node to filter
    source: Box<dyn PlanNode>,

    /// ACP read check, when the collection has a policy.
    acp: Option<AcpReadCheck>,

    /// App read check, when an app read validator governs the collection.
    app: Option<AppReadCheck>,

    /// Current document
    current_doc: Doc,

    /// Document mapping from source
    document_mapping: DocumentMapping,

    /// Ordered checks retain source order while allowing bounded concurrency.
    /// The wrapper only supplies the `Sync` bound required by `PlanNode`;
    /// access is exclusive through `&mut self`.
    pending: SyncWrapper<FuturesOrdered<PermissionCheck>>,

    /// Whether the wrapped source has no more documents to enqueue.
    source_exhausted: bool,
}

impl PermissionFilterNode {
    /// Create a new permission filter node, with the DAC bypass of
    /// [`defra_core::RequestContext::ambient`] until
    /// [`PermissionFilterNode::with_dac_bypass`] sets the request's.
    ///
    /// # Arguments
    /// * `source` - The source node to filter
    /// * `acp` - Document ACP for permission checks
    /// * `identity` - The identity requesting access
    /// * `policy_id` - Policy ID from the collection
    /// * `resource_name` - Resource name from the policy
    pub fn new(
        source: Box<dyn PlanNode>,
        acp: Arc<dyn DocumentACP>,
        identity: Identity,
        policy_id: impl Into<String>,
        resource_name: impl Into<String>,
    ) -> Self {
        let document_mapping = source.document_map().clone();
        Self {
            source,
            acp: Some(AcpReadCheck {
                acp,
                identity: Arc::new(identity),
                dac_bypass: defra_core::RequestContext::ambient().dac_bypass(),
                policy_id: Arc::from(policy_id.into()),
                resource_name: Arc::from(resource_name.into()),
                retry_inputs: None,
                projection: None,
            }),
            app: None,
            current_doc: Doc::default(),
            document_mapping,
            pending: SyncWrapper::new(FuturesOrdered::new()),
            source_exhausted: false,
        }
    }

    /// Create from an optional DID for backward compatibility.
    pub fn from_optional_did(
        source: Box<dyn PlanNode>,
        acp: Arc<dyn DocumentACP>,
        did: Option<Did>,
        policy_id: impl Into<String>,
        resource_name: impl Into<String>,
    ) -> Self {
        Self::new(source, acp, Identity::from(did), policy_id, resource_name)
    }

    /// Let an authenticated reader see every document when the request's node
    /// access grants it `bypass-dac` (`RequestContext::dac_bypass`).
    pub fn with_dac_bypass(mut self, bypass: bool) -> Self {
        if let Some(acp) = &mut self.acp {
            acp.dac_bypass = bypass;
        }
        self
    }

    /// A filter that applies only an app read check.
    pub fn app_only(source: Box<dyn PlanNode>, app: AppReadCheck) -> Self {
        let document_mapping = source.document_map().clone();
        Self {
            source,
            acp: None,
            app: Some(app),
            current_doc: Doc::default(),
            document_mapping,
            pending: SyncWrapper::new(FuturesOrdered::new()),
            source_exhausted: false,
        }
    }

    /// Also require the app read check, when there is one.
    pub fn with_app_check(mut self, app: Option<AppReadCheck>) -> Self {
        self.app = app;
        self
    }

    pub(crate) fn with_projection(
        mut self,
        projection: Option<Arc<AcpRegistrationProjection>>,
    ) -> Self {
        if let Some(acp) = &mut self.acp {
            acp.projection = projection;
        }
        self
    }

    pub(crate) fn with_retry_inputs(
        mut self,
        inputs: Option<crate::prepared::PreparedInputMemo>,
    ) -> Self {
        if let Some(acp) = &mut self.acp {
            acp.retry_inputs = inputs;
        }
        self
    }

    /// Wrap `source` in whichever of the ACP and app checks apply, or return
    /// it unchanged when neither does.
    pub fn wrap(
        source: Box<dyn PlanNode>,
        acp: Option<AcpRead>,
        app: Option<AppReadCheck>,
        retry_inputs: Option<crate::prepared::PreparedInputMemo>,
    ) -> Box<dyn PlanNode> {
        Self::wrap_with_projection_and_inputs(source, acp, app, None, retry_inputs)
    }

    pub fn wrap_with_projection(
        source: Box<dyn PlanNode>,
        acp: Option<AcpRead>,
        app: Option<AppReadCheck>,
        projection: Option<Arc<AcpRegistrationProjection>>,
    ) -> Box<dyn PlanNode> {
        Self::wrap_with_projection_and_inputs(source, acp, app, projection, None)
    }

    pub fn wrap_with_projection_and_inputs(
        source: Box<dyn PlanNode>,
        acp: Option<AcpRead>,
        app: Option<AppReadCheck>,
        projection: Option<Arc<AcpRegistrationProjection>>,
        retry_inputs: Option<crate::prepared::PreparedInputMemo>,
    ) -> Box<dyn PlanNode> {
        match (acp, app) {
            (Some(read), app) => Box::new(
                Self::new(
                    source,
                    read.acp,
                    read.identity,
                    read.policy_id,
                    read.resource_name,
                )
                .with_dac_bypass(read.dac_bypass)
                .with_app_check(app)
                .with_projection(projection)
                .with_retry_inputs(retry_inputs),
            ),
            (None, Some(app)) => Box::new(Self::app_only(source, app)),
            (None, None) => source,
        }
    }

    fn permission_check(&self, doc_id: String, doc: Doc) -> PermissionCheck {
        let acp = self.acp.clone();
        let app = self.app.clone();

        Box::pin(async move {
            let Some(AcpReadCheck {
                acp,
                identity,
                dac_bypass,
                policy_id,
                resource_name,
                retry_inputs,
                projection,
            }) = acp
            else {
                let allowed = app_allows(app.as_ref(), &doc_id).await?;
                return Ok((doc, allowed));
            };
            let allowed = if identity.is_authenticated() && dac_bypass {
                true
            } else {
                match crate::txn::check_doc_access_with_prepared_inputs(
                    acp.as_ref(),
                    projection.as_deref(),
                    identity.as_ref(),
                    DocumentPermission::Read,
                    policy_id.as_ref(),
                    resource_name.as_ref(),
                    &doc_id,
                    retry_inputs.as_ref(),
                )
                .await
                {
                    Ok(allowed) => allowed,
                    Err(error @ crate::error::QueryError::NeedsInput(_)) => return Err(error),
                    Err(error) => {
                        tracing::warn!(
                            %doc_id,
                            policy_id = %policy_id,
                            resource_name = %resource_name,
                            identity = %identity,
                            %error,
                            "Permission check failed, denying access to document"
                        );
                        false
                    }
                }
            };
            let allowed = allowed && app_allows(app.as_ref(), &doc_id).await?;

            Ok((doc, allowed))
        })
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl PlanNode for PermissionFilterNode {
    async fn init(&mut self) -> Result<()> {
        *self.pending.get_mut() = FuturesOrdered::new();
        self.source_exhausted = false;
        self.current_doc = Doc::default();
        self.source.init().await
    }

    async fn start(&mut self) -> Result<()> {
        self.source.start().await
    }

    async fn next(&mut self) -> Result<bool> {
        loop {
            while !self.source_exhausted
                && self.pending.get_mut().len() < MAX_IN_FLIGHT_PERMISSION_CHECKS
            {
                if !self.source.next().await? {
                    self.source_exhausted = true;
                    break;
                }

                let doc = self.source.value().deep_clone();

                // Child join mappings may place `_docID` at a non-zero index.
                let Some(doc_id) = self
                    .document_mapping
                    .first_index_of_name("_docID")
                    .and_then(|index| doc.get(index))
                    .and_then(|value| value.as_str())
                    .map(str::to_owned)
                else {
                    continue;
                };

                let check = self.permission_check(doc_id, doc);
                self.pending.get_mut().push_back(check);

                // Start queued I/O before pulling another source row. A
                // non-blocking poll avoids racing (and potentially cancelling)
                // `source.next()` while still letting immediately available
                // decisions preserve single-row streaming latency.
                if let Some(Some(result)) = self.pending.get_mut().next().now_or_never() {
                    let (doc, allowed) = result?;
                    if allowed {
                        self.current_doc = doc;
                        return Ok(true);
                    }
                }
            }

            match self.pending.get_mut().next().await {
                Some(Ok((doc, true))) => {
                    self.current_doc = doc;
                    return Ok(true);
                }
                Some(Ok((_, false))) => continue,
                Some(Err(error)) => return Err(error),
                None => return Ok(false),
            }
        }
    }

    fn value(&self) -> &Doc {
        &self.current_doc
    }

    async fn close(&mut self) -> Result<()> {
        *self.pending.get_mut() = FuturesOrdered::new();
        self.source_exhausted = true;
        self.source.close().await
    }

    fn source(&self) -> Option<&dyn PlanNode> {
        Some(self.source.as_ref())
    }

    fn document_map(&self) -> &DocumentMapping {
        &self.document_mapping
    }

    // Rows this node drops would come out of a bounded scan's page, so the
    // scan may seek but must read until the cursor has its page.
    fn set_cursor_seek(&mut self, seek: CursorSeek) -> bool {
        self.source.set_cursor_seek(CursorSeek {
            fetch_limit: None,
            ..seek
        })
    }

    fn set_cursor_fetch_limit(&mut self, _limit: u64) -> bool {
        false
    }

    fn page_info(&self) -> Option<crate::plan::CursorPageInfo> {
        self.source.page_info()
    }

    fn kind(&self) -> &'static str {
        "permissionFilterNode"
    }
}

#[cfg(test)]
mod tests {
    use rapidhash::{HashMapExt, RapidHashMap};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use regolith::sync::Barrier;

    use super::*;

    struct MockSource {
        docs: Vec<Doc>,
        position: usize,
        current: Doc,
        mapping: DocumentMapping,
        yielded: Arc<AtomicUsize>,
    }

    impl MockSource {
        fn new(doc_ids: &[String], yielded: Arc<AtomicUsize>) -> Self {
            let docs = doc_ids
                .iter()
                .map(|doc_id| {
                    let mut doc = Doc::new(1);
                    doc.set_doc_id(doc_id);
                    doc
                })
                .collect();
            let mut mapping = DocumentMapping::new();
            mapping.add(0, "_docID");

            Self {
                docs,
                position: 0,
                current: Doc::default(),
                mapping,
                yielded,
            }
        }
    }

    #[async_trait]
    impl PlanNode for MockSource {
        async fn init(&mut self) -> Result<()> {
            self.position = 0;
            Ok(())
        }

        async fn start(&mut self) -> Result<()> {
            Ok(())
        }

        async fn next(&mut self) -> Result<bool> {
            let Some(doc) = self.docs.get(self.position) else {
                return Ok(false);
            };
            self.position += 1;
            self.yielded.fetch_add(1, Ordering::SeqCst);
            self.current = doc.deep_clone();
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
            "mockSource"
        }
    }

    struct MockAcp {
        barrier: Option<Arc<Barrier>>,
        delays: RapidHashMap<String, Duration>,
        active: AtomicUsize,
        max_active: AtomicUsize,
        calls: AtomicUsize,
    }

    impl MockAcp {
        fn new(barrier: Option<Arc<Barrier>>, delays: RapidHashMap<String, Duration>) -> Self {
            Self {
                barrier,
                delays,
                active: AtomicUsize::new(0),
                max_active: AtomicUsize::new(0),
                calls: AtomicUsize::new(0),
            }
        }
    }

    #[async_trait]
    impl DocumentACP for MockAcp {
        async fn register_doc_object(
            &self,
            _identity: &Did,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> acp::Result<()> {
            Ok(())
        }

        async fn is_doc_registered(
            &self,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> acp::Result<bool> {
            Ok(true)
        }

        async fn check_doc_access(
            &self,
            _identity: &Identity,
            _permission: DocumentPermission,
            _policy_id: &str,
            _resource_name: &str,
            doc_id: &str,
        ) -> acp::Result<bool> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let active = self.active.fetch_add(1, Ordering::SeqCst) + 1;
            self.max_active.fetch_max(active, Ordering::SeqCst);

            if let Some(barrier) = &self.barrier {
                barrier.wait().await;
            }
            if let Some(delay) = self.delays.get(doc_id) {
                workers::delay(*delay).unwrap().await.unwrap();
            }

            self.active.fetch_sub(1, Ordering::SeqCst);
            if doc_id == "error" {
                Err(acp::Error::Storage("injected failure".to_string()))
            } else {
                Ok(true)
            }
        }

        async fn add_actor_relationship(
            &self,
            _requestor: &Did,
            _target: &Did,
            _policy_id: &str,
            _collection_id: &str,
            _doc_id: &str,
            _relation: &str,
            _managing_relations: &[String],
        ) -> acp::Result<bool> {
            Ok(true)
        }

        async fn delete_actor_relationship(
            &self,
            _requestor: &Did,
            _target: &Did,
            _policy_id: &str,
            _collection_id: &str,
            _doc_id: &str,
            _relation: &str,
            _managing_relations: &[String],
        ) -> acp::Result<bool> {
            Ok(true)
        }

        async fn unregister_doc_object(
            &self,
            _policy_id: &str,
            _resource_name: &str,
            _doc_id: &str,
        ) -> acp::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn prepared_permission_check_exits_for_external_input_without_calling_provider() {
        workers::block_on(async {
            let yielded = Arc::new(AtomicUsize::new(0));
            let acp = Arc::new(MockAcp::new(None, RapidHashMap::new()));
            let memo = crate::prepared::PreparedInputMemo::default();
            let node = PermissionFilterNode::new(
                Box::new(MockSource::new(&["doc-1".to_string()], yielded)),
                acp.clone(),
                Identity::Anonymous,
                "policy",
                "User",
            )
            .with_retry_inputs(Some(memo.clone()));
            let doc = Doc::default();
            let error = node
                .permission_check("doc-1".to_string(), doc.deep_clone())
                .await
                .unwrap_err();
            let crate::error::QueryError::NeedsInput(request) = error else {
                panic!("expected typed ACP input request");
            };
            assert_eq!(acp.calls.load(Ordering::SeqCst), 0);

            memo.insert(request, b"true".as_slice());
            let (_, allowed) = node
                .permission_check("doc-1".to_string(), doc)
                .await
                .expect("memoized decision succeeds");
            assert!(allowed);
            assert_eq!(acp.calls.load(Ordering::SeqCst), 0);
        })
        .unwrap()
    }

    fn permission_filter(
        doc_ids: &[String],
        yielded: Arc<AtomicUsize>,
        acp: Arc<MockAcp>,
    ) -> PermissionFilterNode {
        PermissionFilterNode::new(
            Box::new(MockSource::new(doc_ids, yielded)),
            acp,
            Identity::Anonymous,
            "policy",
            "User",
        )
    }

    #[test]
    fn dispatches_a_bounded_window_concurrently() {
        workers::block_on(async {
            let doc_ids: Vec<_> = (0..=MAX_IN_FLIGHT_PERMISSION_CHECKS)
                .map(|index| format!("doc-{index}"))
                .collect();
            let yielded = Arc::new(AtomicUsize::new(0));
            let acp = Arc::new(MockAcp::new(
                Some(Arc::new(Barrier::new(MAX_IN_FLIGHT_PERMISSION_CHECKS))),
                RapidHashMap::new(),
            ));
            let mut node = permission_filter(&doc_ids, Arc::clone(&yielded), Arc::clone(&acp));

            node.init().await.unwrap();
            node.start().await.unwrap();
            let has_row = workers::timeout(Duration::from_secs(1), node.next())
                .await
                .expect("permission checks did not run concurrently")
                .unwrap();

            assert!(has_row);
            assert_eq!(node.value().doc_id(), Some("doc-0"));
            assert_eq!(
                yielded.load(Ordering::SeqCst),
                MAX_IN_FLIGHT_PERMISSION_CHECKS
            );
            assert_eq!(
                acp.max_active.load(Ordering::SeqCst),
                MAX_IN_FLIGHT_PERMISSION_CHECKS
            );
        })
        .unwrap()
    }

    #[test]
    fn preserves_source_order_when_checks_finish_out_of_order() {
        workers::block_on(async {
            let doc_ids = vec!["slow".to_string(), "fast".to_string(), "medium".to_string()];
            let delays = RapidHashMap::from_iter([
                ("slow".to_string(), Duration::from_millis(30)),
                ("fast".to_string(), Duration::from_millis(1)),
                ("medium".to_string(), Duration::from_millis(10)),
            ]);
            let yielded = Arc::new(AtomicUsize::new(0));
            let acp = Arc::new(MockAcp::new(None, delays));
            let mut node = permission_filter(&doc_ids, yielded, Arc::clone(&acp));

            node.init().await.unwrap();
            node.start().await.unwrap();

            let mut actual = Vec::new();
            while node.next().await.unwrap() {
                actual.push(node.value().doc_id().unwrap().to_string());
            }

            assert_eq!(actual, doc_ids);
            assert_eq!(acp.max_active.load(Ordering::SeqCst), 3);
        })
        .unwrap()
    }

    #[test]
    fn permission_errors_fail_closed_without_stopping_the_scan() {
        workers::block_on(async {
            let doc_ids = vec!["error".to_string(), "allowed".to_string()];
            let acp = Arc::new(MockAcp::new(None, RapidHashMap::new()));
            let mut node = permission_filter(&doc_ids, Arc::new(AtomicUsize::new(0)), acp);

            node.init().await.unwrap();
            node.start().await.unwrap();

            assert!(node.next().await.unwrap());
            assert_eq!(node.value().doc_id(), Some("allowed"));
            assert!(!node.next().await.unwrap());
        })
        .unwrap()
    }
}
