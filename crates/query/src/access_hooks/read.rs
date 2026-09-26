use std::sync::Arc;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use schema::CollectionVersion;

/// One document a client of this node asks to read.
pub struct ReadRequest<'a> {
    pub identity: Option<&'a Did>,
    pub collection: &'a CollectionVersion,
    pub doc_id: &'a str,
}

/// Decides which documents clients of this node may read.
///
/// Consulted for user queries, nested relations, aggregates, time-travel
/// queries, encrypted-index results and `_commits`, after any ACP check, so it
/// can only hide what ACP would show. It is node-local and may read anything
/// the node knows. An `Err` hides the document.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ReadValidator: MaybeSendSync {
    /// Whether reads of this collection go through [`Self::may_read`].
    fn governs(&self, collection: &CollectionVersion) -> bool;

    async fn may_read(&self, request: &ReadRequest<'_>) -> Result<bool, String>;
}

/// A read validator bound to one query's identity and collection.
#[derive(Clone)]
pub struct AppReadCheck {
    validator: Arc<dyn ReadValidator>,
    identity: Option<Did>,
    collection: Arc<CollectionVersion>,
}

impl AppReadCheck {
    /// `None` when no validator is installed or it does not govern the
    /// collection, so ungoverned reads pay nothing.
    pub fn bind(
        validator: Option<&Arc<dyn ReadValidator>>,
        identity: Option<Did>,
        collection: &CollectionVersion,
    ) -> Option<Self> {
        let validator = validator.filter(|validator| validator.governs(collection))?;
        Some(Self {
            validator: validator.clone(),
            identity,
            collection: Arc::new(collection.clone()),
        })
    }

    pub async fn allows(&self, doc_id: &str) -> bool {
        let request = ReadRequest {
            identity: self.identity.as_ref(),
            collection: &self.collection,
            doc_id,
        };
        self.validator
            .may_read(&request)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%doc_id, collection = %self.collection.name, %error, "Read validator failed, hiding document");
                false
            })
    }
}

/// Whether `check` lets `doc_id` through; no check lets everything through.
pub(crate) async fn app_allows(check: Option<&AppReadCheck>, doc_id: &str) -> bool {
    match check {
        Some(check) => check.allows(doc_id).await,
        None => true,
    }
}
