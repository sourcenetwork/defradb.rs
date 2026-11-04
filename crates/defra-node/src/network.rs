//! The node's share of the network runtime.
//!
//! A node runs its network work (signed queries, the HTTP server, P2P) on
//! the runtime the application passes to
//! [`NodeBuilder::with_edge`](crate::NodeBuilder::with_edge). Given none,
//! it starts one itself, and only when it has network work to run: a node
//! with no identity, no HTTP server and no P2P starts no runtime at all.
//! Either way the node's work goes through a scope of its own, so shutting
//! the node down stops accepting and drains the node's work alone, then
//! stops a runtime the node started.

use crate::Result;
use std::num::NonZeroUsize;
use std::time::Duration;

use edge::{Edge, EdgeConfig, EdgeRuntime, Scope};
use kovan_queue::seg_queue::SegQueue;

/// How long shutdown waits for the node's in-flight network work.
pub(crate) const NETWORK_DRAIN_TIMEOUT: Duration = Duration::from_secs(30);

/// Threads of a runtime the node starts itself. Network work is I/O bound;
/// an application with heavier network work passes its own runtime.
const OWNED_RUNTIME_THREADS: NonZeroUsize = NonZeroUsize::MIN.saturating_add(1);

pub(crate) struct NodeEdge {
    #[cfg(any(feature = "otel", feature = "p2p"))]
    runtime: Edge,
    scope: Scope,
    owned: SegQueue<EdgeRuntime>,
}

impl NodeEdge {
    /// The node's network handle, a scope of `given` or of a runtime the
    /// node starts, or `None` when the node has no network work and was
    /// given no runtime.
    pub(crate) fn start(given: Option<Edge>, needed: bool) -> Result<Option<Self>> {
        let owned = SegQueue::new();
        let edge = match given {
            Some(edge) => edge,
            None if needed => {
                let runtime = EdgeRuntime::start(EdgeConfig {
                    threads: Some(OWNED_RUNTIME_THREADS),
                    thread_name: "defra-node-edge".to_string(),
                    ..EdgeConfig::default()
                })?;
                let edge = runtime.edge().clone();
                owned.push(runtime);
                edge
            }
            None => return Ok(None),
        };
        Ok(Some(Self {
            scope: edge.scope(),
            #[cfg(any(feature = "otel", feature = "p2p"))]
            runtime: edge,
            owned,
        }))
    }

    /// The handle the node's network work is submitted through.
    pub(crate) fn edge(&self) -> &Edge {
        self.scope.edge()
    }

    /// The runtime the node's scope is part of, which still accepts work
    /// after [`drain`](Self::drain) and until [`stop_owned`](Self::stop_owned).
    #[cfg(feature = "otel")]
    pub(crate) fn runtime(&self) -> &Edge {
        &self.runtime
    }

    /// A scope of the runtime beside the node's own, so its work goes on
    /// after [`drain`](Self::drain) and is drained apart.
    #[cfg(feature = "p2p")]
    pub(crate) fn sibling(&self) -> Scope {
        self.runtime.scope()
    }

    /// Stops accepting the node's network work, which stops its services,
    /// then waits up to [`NETWORK_DRAIN_TIMEOUT`] for the rest.
    pub(crate) async fn drain(&self) {
        drain(&self.scope).await;
    }

    /// Shuts down the runtime the node started, if it started one.
    pub(crate) async fn stop_owned(&self) {
        let Some(runtime) = self.owned.pop() else {
            return;
        };
        if let Err(error) = runtime.shutdown_async().await {
            tracing::warn!(%error, "the node's network runtime did not shut down cleanly");
        }
    }
}

/// Stops accepting work through `scope`, which stops its services, then
/// waits up to [`NETWORK_DRAIN_TIMEOUT`] for the rest.
pub(crate) async fn drain(scope: &Scope) {
    let drained = scope.drain(NETWORK_DRAIN_TIMEOUT).await;
    if !drained.is_complete() {
        tracing::warn!(
            remaining = drained.remaining,
            timeout_secs = NETWORK_DRAIN_TIMEOUT.as_secs(),
            "timed out draining the node's network work during shutdown"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::NodeEdge;

    #[test]
    fn a_node_without_network_work_starts_no_runtime() {
        assert!(NodeEdge::start(None, false).unwrap().is_none());
    }

    #[test]
    fn a_node_given_a_runtime_drains_its_own_scope_of_it() {
        let given = edge::testing::edge();
        let network = NodeEdge::start(Some(given.clone()), false)
            .unwrap()
            .expect("a given runtime is used even without network work");

        assert!(network.owned.is_empty());
        assert_ne!(network.edge(), &given);
        given.block_on(network.drain()).unwrap();
        assert!(network.edge().is_closed());
        assert!(!given.is_closed());
    }

    #[cfg(feature = "p2p")]
    #[test]
    fn a_sibling_scope_accepts_work_after_the_node_drains_and_drains_apart() {
        let given = edge::testing::edge();
        let network = NodeEdge::start(Some(given.clone()), true)
            .unwrap()
            .expect("a given runtime is used");
        let sibling = network.sibling();

        given
            .block_on(async {
                network.drain().await;
                assert!(network.edge().is_closed());
                assert_eq!(sibling.edge().spawn(async { 1 }).await, Ok(1));
                super::drain(&sibling).await;
            })
            .unwrap();
        assert!(sibling.edge().is_closed());
        assert!(!given.is_closed());
    }

    #[test]
    fn a_node_given_no_runtime_starts_one_and_stops_it() {
        workers::block_on(async {
            let network = NodeEdge::start(None, true)
                .unwrap()
                .expect("network work needs a runtime");
            let edge = network.edge().clone();

            assert_eq!(edge.spawn(async { 1 }).await, Ok(1));
            network.drain().await;
            network.stop_owned().await;

            assert!(edge.is_closed());
            assert!(network.owned.is_empty());
        })
        .unwrap()
    }
}
