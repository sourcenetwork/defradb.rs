//! Collection topic subscriptions.
//!
//! The desired set of subscribed collections is durable: it lives in the
//! [`P2PCollectionStorage`], where each change is one engine transaction. The
//! live set holds the collection topics whose transport installation
//! succeeded. A subscribe or an unsubscribe commits its durable change first,
//! then installs or removes the topics at the network edge, then checks each
//! topic against the desired state again. A topic it leaves apart (the
//! transport failed, or a change the other way committed meanwhile) goes to
//! the topic's one background reconciler ([`reconcile`]), which converges it.
//! Nothing waits on a lock: changes that race leave the topic to the
//! reconciler.
//!
//! For example, an unsubscribe of `users` commits, a subscribe of `users`
//! commits and finds the topic still live, so it installs nothing; the
//! unsubscribe then removes the topic and finds `users` desired again, so the
//! reconciler installs it. What must never happen: once the changes stop and
//! the transport answers, a topic that is live and not desired, or desired
//! and not live.

mod reconcile;
#[cfg(test)]
mod tests;

use std::sync::Arc;
use std::time::Duration;

use blockstore::Blockstore;
use futures::{stream, StreamExt};
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};

use super::{SyncCoordinator, SyncShutdownHandle};
use crate::error::{Error, Result};
use crate::sync::broadcaster::Broadcaster;
use crate::sync::collection_store::P2PCollectionStorage;
use crate::transport::P2PTransport;

const MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS: usize = 16;
const COLLECTION_SUBSCRIPTION_TIMEOUT: Duration = Duration::from_secs(5);

/// A set of collection IDs.
type CollectionSet = HopscotchMap<String, (), RandomState>;

/// Collection topic subscriptions: the durable desired set and the live
/// topics.
pub(super) struct CollectionSubscriptions<T>
where
    T: P2PTransport,
{
    shared: Arc<Shared<T>>,
}

/// What the subscription calls and the reconcilers share.
struct Shared<T>
where
    T: P2PTransport,
{
    /// The desired set.
    store: Arc<dyn P2PCollectionStorage>,
    /// Topics whose transport installation succeeded and that were not
    /// removed since.
    live: CollectionSet,
    /// Collections whose reconciler runs.
    reconciling: CollectionSet,
    broadcaster: Broadcaster<T>,
    shutdown: SyncShutdownHandle,
}

/// What a transport call does to a collection topic.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TopicChange {
    Install,
    Remove,
}

impl TopicChange {
    /// The change that moves a topic to `desired`.
    fn towards(desired: bool) -> Self {
        if desired {
            Self::Install
        } else {
            Self::Remove
        }
    }

    fn doing(self) -> &'static str {
        match self {
            Self::Install => "subscribing to",
            Self::Remove => "unsubscribing from",
        }
    }
}

impl<T> CollectionSubscriptions<T>
where
    T: P2PTransport,
{
    pub(super) fn new(
        store: Arc<dyn P2PCollectionStorage>,
        broadcaster: Broadcaster<T>,
        shutdown: SyncShutdownHandle,
    ) -> Self {
        Self {
            shared: Arc::new(Shared {
                store,
                live: HopscotchMap::with_hasher(RandomState::default()),
                reconciling: HopscotchMap::with_hasher(RandomState::default()),
                broadcaster,
                shutdown,
            }),
        }
    }

    /// Whether this node has joined the collection's topic.
    pub(super) fn is_live(&self, collection_id: &str) -> bool {
        self.shared.live.contains_key(collection_id)
    }

    /// Subscribes the collections: the durable change, then the transport
    /// installation of each topic not live yet. Returns how many topics the
    /// transport installed anew; the first transport failure is returned
    /// after every topic was tried, and its topic converges in the
    /// background.
    pub(super) async fn subscribe(&self, collection_ids: &[String]) -> Result<usize> {
        let requested = distinct(collection_ids);
        if requested.is_empty() {
            return Ok(0);
        }
        let desired_added = self.shared.store.add_collections(&requested).await?;
        let missing = requested
            .into_iter()
            .filter(|collection_id| !self.is_live(collection_id))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            return Ok(0);
        }
        let (installed, failure) = self.shared.change_all(missing, TopicChange::Install).await;
        tracing::debug!(
            requested = collection_ids.len(),
            desired_added,
            transport_installed = installed,
            "Applied durable collection subscription batch"
        );
        failure.map_or(Ok(installed), Err)
    }

    /// Unsubscribes the collections: the durable change, then the transport
    /// removal of every topic, live or not, since a subscribe that timed out
    /// may have joined a topic the live set does not hold. Returns how many
    /// topics the transport removed.
    pub(super) async fn unsubscribe(&self, collection_ids: &[String]) -> Result<usize> {
        let requested = distinct(collection_ids);
        if requested.is_empty() {
            return Ok(0);
        }
        self.shared.store.remove_collections(&requested).await?;
        let count = requested.len();
        let (removed, failure) = self.shared.change_all(requested, TopicChange::Remove).await;
        tracing::debug!(
            requested = count,
            transport_removed = removed,
            "Removed durable collection subscription batch"
        );
        failure.map_or(Ok(removed), Err)
    }

    /// Joins the topic of every durably subscribed collection, at startup.
    /// Returns how many the transport installed in this call; the others
    /// converge in the background.
    pub(super) async fn restore(&self) -> Result<usize> {
        let collections = self.shared.store.get_all_collections().await?;
        if collections.is_empty() {
            tracing::debug!("No persisted P2P collections to load");
            return Ok(0);
        }
        tracing::info!(
            count = collections.len(),
            "Loading persisted P2P collections"
        );
        let (_, failure) = self
            .shared
            .change_all(collections.clone(), TopicChange::Install)
            .await;
        if let Some(error) = failure {
            tracing::warn!(
                error = %error,
                "Failed to subscribe to a persisted P2P collection; retrying in the background"
            );
        }
        let loaded = collections
            .iter()
            .filter(|collection_id| self.is_live(collection_id))
            .count();
        tracing::info!(loaded, "Finished loading P2P collections");
        Ok(loaded)
    }

    /// The durably subscribed collections, sorted.
    pub(super) async fn desired(&self) -> Result<Vec<String>> {
        let mut collections = self.shared.store.get_all_collections().await?;
        collections.sort();
        Ok(collections)
    }

    /// Marks a topic live without a transport call, for tests that start
    /// from a joined topic.
    #[cfg(test)]
    pub(super) fn insert_live(&self, collection_id: &str) {
        self.shared.live.insert(collection_id.to_owned(), ());
    }
}

impl<T> Shared<T>
where
    T: P2PTransport,
{
    /// Applies `change` to every topic, a bounded number at a time, then
    /// checks each against the desired state. Returns how many the transport
    /// changed and the first failure.
    async fn change_all(
        self: &Arc<Self>,
        collection_ids: Vec<String>,
        change: TopicChange,
    ) -> (usize, Option<Error>) {
        let results = stream::iter(collection_ids)
            .map(|collection_id| async move {
                let result = self.change_topic(&collection_id, change).await;
                (collection_id, result)
            })
            .buffer_unordered(MAX_CONCURRENT_COLLECTION_SUBSCRIPTIONS)
            .collect::<Vec<_>>()
            .await;
        let mut changed = 0;
        let mut failure = None;
        for (collection_id, result) in results {
            match result {
                Ok(did_change) => changed += usize::from(did_change),
                Err(error) => {
                    tracing::warn!(
                        collection_id = %collection_id,
                        error = %error,
                        "Collection topic change failed; retrying in the background"
                    );
                    failure.get_or_insert(error);
                }
            }
            self.settle(collection_id).await;
        }
        (changed, failure)
    }

    /// Installs or removes one collection topic at the network edge, bounded
    /// by the timeout, and records the outcome in the live set. Returns
    /// whether the transport changed anything.
    async fn change_topic(&self, collection_id: &str, change: TopicChange) -> Result<bool> {
        let call = async {
            match change {
                TopicChange::Install => self.broadcaster.subscribe_collection(collection_id).await,
                TopicChange::Remove => self.broadcaster.unsubscribe_collection(collection_id).await,
            }
        };
        let changed = crate::rt::time::timeout(COLLECTION_SUBSCRIPTION_TIMEOUT, call)
            .await
            .unwrap_or_else(|_| {
                Err(Error::Transport(format!(
                    "timed out {} collection {collection_id}",
                    change.doing()
                )))
            })?;
        match change {
            TopicChange::Install => {
                self.live.insert(collection_id.to_owned(), ());
            }
            TopicChange::Remove => {
                self.live.remove(collection_id);
            }
        }
        Ok(changed)
    }

    /// The desired state of a collection, or `None`, reported, when the
    /// durable read fails.
    async fn desired(&self, collection_id: &str) -> Option<bool> {
        match self.store.is_subscribed(collection_id).await {
            Ok(desired) => Some(desired),
            Err(error) => {
                tracing::warn!(
                    collection_id = %collection_id,
                    error = %error,
                    "Failed to read durable subscription state; retrying in the background"
                );
                None
            }
        }
    }

    /// Whether the live set agrees with the desired state of a collection.
    /// A desired state that cannot be read does not agree.
    async fn converged(&self, collection_id: &str) -> bool {
        self.desired(collection_id)
            .await
            .is_some_and(|desired| desired == self.live.contains_key(collection_id))
    }

    /// Hands a collection whose live topic and desired state are apart to
    /// its reconciler.
    async fn settle(self: &Arc<Self>, collection_id: String) {
        if !self.converged(&collection_id).await {
            self.reconcile(collection_id);
        }
    }
}

/// The IDs in order of first appearance, each once.
fn distinct(collection_ids: &[String]) -> Vec<String> {
    let mut seen = RapidHashSet::with_capacity(collection_ids.len());
    collection_ids
        .iter()
        .filter(|collection_id| seen.insert(collection_id.as_str()))
        .cloned()
        .collect()
}

impl<B: Blockstore + 'static, T: P2PTransport> SyncCoordinator<B, T> {
    /// Subscribe to a collection for sync.
    pub async fn subscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let subscribed = self
            .subscribe_collections(&[collection_id.to_string()])
            .await?;
        Ok(subscribed == 1)
    }

    /// Subscribe to collection topics as one durable, bounded batch.
    ///
    /// The durable set is desired state, matching Go DefraDB: it commits
    /// atomically before transport work begins and heals in the background
    /// or at restart. The live set holds only topics whose transport
    /// installation succeeded.
    pub async fn subscribe_collections(&self, collection_ids: &[String]) -> Result<usize> {
        self.subscriptions
            .collections
            .subscribe(collection_ids)
            .await
    }

    /// Subscribe to a specific document for sync.
    pub async fn subscribe_document(&self, doc_id: &str) -> Result<bool> {
        self.runtime.broadcaster.subscribe_document(doc_id).await
    }

    /// Unsubscribe from a collection.
    pub async fn unsubscribe_collection(&self, collection_id: &str) -> Result<bool> {
        let removed = self
            .unsubscribe_collections(&[collection_id.to_string()])
            .await?;
        Ok(removed == 1)
    }

    /// Unsubscribe from collection topics as one durable, bounded batch.
    ///
    /// Matches Go DefraDB's commit-first semantics: the durable removal
    /// commits before any transport topic is touched.
    pub async fn unsubscribe_collections(&self, collection_ids: &[String]) -> Result<usize> {
        self.subscriptions
            .collections
            .unsubscribe(collection_ids)
            .await
    }

    /// Unsubscribe from a document.
    pub async fn unsubscribe_document(&self, doc_id: &str) -> Result<bool> {
        self.runtime.broadcaster.unsubscribe_document(doc_id).await
    }

    /// Get the list of subscribed collection IDs.
    pub async fn get_subscribed_collections(&self) -> Result<Vec<String>> {
        self.subscriptions.collections.desired().await
    }

    /// Load and subscribe to all persisted P2P collections.
    ///
    /// This can run before pubsub_rpc services start: collection topic
    /// subscription is a transport-level operation independent of the base
    /// doc-sync / sync-branchable service topics.
    pub async fn load_p2p_collections(&self) -> Result<usize> {
        self.subscriptions.collections.restore().await
    }
}
