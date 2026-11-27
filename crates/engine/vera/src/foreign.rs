//! Which deployment owns each policy (`proofs/tla/ForeignPolicy.tla`,
//! `proofs/lean/ForeignRoutes.lean`).
//!
//! A node decides document access against its own Vera deployment. A policy
//! another deployment owns, such as the one an account shares a space under,
//! is decided against that deployment instead: its owner's home. The
//! registry maps such a policy to a document ACP over a light client of the
//! home ([`ForeignProvider`]), verified under the home's consensus key and
//! genesis identity, and every other policy to nothing, so the node's own
//! deployment decides it as before. Policies are added and removed while
//! the node runs; policies of one home share its light client, one per
//! home and address, built once however many adds ask for it at a time.
//!
//! A policy is named by one spelling everywhere: its 32 bytes in lowercase
//! hex, however it was added. The node's own deployment never holds a
//! routed policy: [`VeraDocumentACP::add_foreign_policy`] refuses to route
//! one it holds or is creating, and [`VeraDocumentACP::add_policy`] refuses,
//! before anything is sent, to create or adopt one that is routed or being
//! routed ([`claims`]).
//!
//! A home's new verified state is told to the listener the database gave
//! ([`acp::ObjectChangeListener::policy_changed`]) for each policy routed
//! to it, so what waits on the home's grants is asked again; a removed
//! route is told as [`acp::ObjectChangeListener::policy_removed`].

mod claims;

use std::future::Future;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Weak};

use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

use self::claims::Claims;
pub(crate) use self::claims::Held;
use crate::dac::VeraDocumentACP;
use crate::provider::ProviderError;
#[cfg(not(target_arch = "wasm32"))]
use crate::tuning::AcpTuning;
use crate::vera_rs::HomeStateListener;
#[cfg(not(target_arch = "wasm32"))]
use crate::vera_rs::{AcpTransport, ForeignProvider};

/// The deployment that owns a policy: what a node is given to decide it
/// there. None of it is taken from the home itself.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignHome {
    /// Hex consensus key every proof and header of the home is verified
    /// against.
    pub consensus_key: String,
    /// The home deployment's genesis identity, which a switch of its active
    /// device is checked against.
    pub genesis_id: [u8; 32],
    /// The home's JSON-RPC address; finalized headers are read from the
    /// WebSocket at the same address. A later add of the same home with
    /// another address moves every policy of that home to it.
    pub rpc_address: String,
}

impl ForeignHome {
    /// The deployment's name: one per consensus key and genesis.
    pub(crate) fn deployment(&self) -> String {
        let key = self.consensus_key.trim_start_matches("0x").to_lowercase();
        format!("{key}/{}", hex::encode(self.genesis_id))
    }
}

/// One policy's route: its home, the address it was reached at, and the
/// document ACP that reads it there.
#[derive(Clone)]
struct Route {
    deployment: String,
    address: String,
    acp: Arc<VeraDocumentACP>,
}

/// The light client of one home at one address, built by the first add
/// that asks for it while the others wait for it.
#[derive(Default)]
struct HomeClient {
    acp: regolith::sync::OnceCell<Arc<VeraDocumentACP>>,
    /// How many adds are building or waiting for it now.
    adds: AtomicUsize,
}

/// The policies other deployments own, and where each is decided.
#[derive(Default)]
pub struct ForeignPolicies {
    routes: HopscotchMap<String, Route, RandomState>,
    clients: HopscotchMap<String, Arc<HomeClient>, RandomState>,
    /// Policies an add is routing now and creates on the node's own
    /// deployment in flight.
    claims: Claims,
    listener: regolith::sync::OnceCell<Arc<dyn acp::ObjectChangeListener>>,
}

/// The bounded policy routing and creation work in flight on this node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForeignPolicyClaims {
    /// Adds that are deciding whether and where to route a policy.
    pub adds: usize,
    /// Creates on the node's own deployment that may still land.
    pub creates: usize,
}

/// The policy identifier as every route is keyed and told: its 32 bytes in
/// lowercase hex, however it was written.
pub(crate) fn route_key(policy_id: &str) -> Result<String, ProviderError> {
    vera_client::parse_policy_id(policy_id)
        .map(hex::encode)
        .map_err(|_| ProviderError::Config(format!("invalid policy ID {policy_id}")))
}

/// Tells the registry that a home's verified state moved.
struct HomeChanged {
    registry: Weak<ForeignPolicies>,
    deployment: String,
}

impl HomeStateListener for HomeChanged {
    fn state_changed(&self) {
        if let Some(registry) = self.registry.upgrade() {
            registry.home_changed(&self.deployment);
        }
    }
}

impl ForeignPolicies {
    /// The policy routing and creation work in flight now.
    pub fn claims(&self) -> ForeignPolicyClaims {
        let (adds, creates) = self.claims.sizes();
        ForeignPolicyClaims { adds, creates }
    }

    /// The document ACP that decides `policy_id`, when another deployment
    /// owns it.
    pub fn route(&self, policy_id: &str) -> Option<Arc<VeraDocumentACP>> {
        if self.routes.is_empty() {
            return None;
        }
        let key = route_key(policy_id).ok()?;
        self.routes.get(&key).map(|route| route.acp)
    }

    /// Whether another deployment owns `policy_id`.
    pub fn owns_elsewhere(&self, policy_id: &str) -> bool {
        self.route(policy_id).is_some()
    }

    /// Whether `policy_id` is routed elsewhere or an add is routing it now:
    /// the node's own deployment must not hold it. The claims are read
    /// before the routes: an add inserts its route before it drops its
    /// claim, so a claim seen gone leaves its route seen.
    pub(crate) fn claimed(&self, policy_id: &str) -> bool {
        let Ok(key) = route_key(policy_id) else {
            return false;
        };
        self.claims.claimed(&key) || self.routes.contains_key(&key)
    }

    /// Whether any policy is routed elsewhere or being routed.
    fn any_claimed(&self) -> bool {
        self.claims.any_claimed() || !self.routes.is_empty()
    }

    /// Mark `policy_id` as being routed until the claim is dropped. Refused
    /// while a create on the node's own deployment may be making it
    /// ([`ProviderError::PolicyBeingCreated`]).
    pub(crate) fn claim(&self, policy_id: &str) -> Result<Held, ProviderError> {
        let policy = route_key(policy_id)?;
        let claim = self.claims.claim(policy.clone()).map_err(claims_full)?;
        if self.claims.creating(&policy) {
            return Err(ProviderError::PolicyBeingCreated { policy });
        }
        Ok(claim)
    }

    /// Mark a create on the node's own deployment of `policy_id`, or of a
    /// policy whose ID is known only once it is made, as in flight until
    /// the mark is dropped. Refused, before anything is sent, while the
    /// policy is routed or being routed
    /// ([`ProviderError::PolicyRoutedElsewhere`]), and, for an unknown ID,
    /// while any policy is ([`ProviderError::UnnamedPolicyCreate`]).
    pub(crate) fn begin_create(&self, policy_id: Option<&str>) -> Result<Held, ProviderError> {
        let Some(policy_id) = policy_id else {
            let create = self.claims.create(None).map_err(claims_full)?;
            if self.any_claimed() {
                return Err(ProviderError::UnnamedPolicyCreate);
            }
            return Ok(create);
        };
        let policy = route_key(policy_id)?;
        let create = self
            .claims
            .create(Some(policy.clone()))
            .map_err(claims_full)?;
        if self.claimed(&policy) {
            return Err(ProviderError::PolicyRoutedElsewhere { policy });
        }
        Ok(create)
    }

    /// The policies routed to another deployment, each in its one spelling.
    pub fn policies(&self) -> Vec<String> {
        self.routes.iter().map(|(policy, _)| policy).collect()
    }

    /// How many objects of `policy_id` the database has changes parked
    /// under, waiting for its home's state to let them in.
    pub fn waiting(&self, policy_id: &str) -> usize {
        match (self.listener.get(), route_key(policy_id)) {
            (Some(listener), Ok(policy)) => listener.waiting_under(&policy),
            _ => 0,
        }
    }

    /// How many parked objects of `policy_id` the database has released
    /// because its home reached a new state.
    pub fn released(&self, policy_id: &str) -> usize {
        match (self.listener.get(), route_key(policy_id)) {
            (Some(listener), Ok(policy)) => listener.released_under(&policy),
            _ => 0,
        }
    }

    /// Decide `policy_id` against `home` from now on, reached at its
    /// JSON-RPC address over the network runtime `edge`. The caller has
    /// checked that the node's own deployment does not hold it.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn add(
        self: &Arc<Self>,
        policy_id: &str,
        home: &ForeignHome,
        tuning: &AcpTuning,
        edge: &edge::Edge,
    ) -> Result<(), ProviderError> {
        let ws = home
            .rpc_address
            .replacen("http://", "ws://", 1)
            .replacen("https://", "wss://", 1);
        let transport = acp_light_client::HttpWsTransport::new(&home.rpc_address, &ws)
            .map_err(|e| ProviderError::Config(format!("foreign light client: {e}")))?;
        self.add_with_transport(policy_id, home, Arc::new(transport), tuning, edge)
            .await
    }

    /// [`Self::add`], reaching the home through `transport`, which is not
    /// trusted: everything it carries is verified under `home`'s key.
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) async fn add_with_transport(
        self: &Arc<Self>,
        policy_id: &str,
        home: &ForeignHome,
        transport: Arc<dyn AcpTransport>,
        tuning: &AcpTuning,
        edge: &edge::Edge,
    ) -> Result<(), ProviderError> {
        let deployment = home.deployment();
        let listener = Arc::new(HomeChanged {
            registry: Arc::downgrade(self),
            deployment: deployment.clone(),
        });
        let (key, genesis_id) = (home.consensus_key.clone(), home.genesis_id);
        let (edge, timeout) = (edge.clone(), tuning.request_timeout);
        self.route_to(policy_id, &deployment, &home.rpc_address, async move {
            let provider =
                ForeignProvider::connect(transport, &key, genesis_id, listener, &edge).await?;
            Ok(Arc::new(
                VeraDocumentACP::new(Arc::new(provider)).on_network(&edge, timeout),
            ))
        })
        .await
    }

    /// Route `policy_id` to `deployment` at `address`, through the client
    /// `connect` builds when no add built one for that home and address
    /// before; adds that ask at once wait for one build. Every other policy
    /// of the deployment moves to this address too, and a client no route
    /// uses any more is dropped. Whatever waits on the policy is asked
    /// again under its new home.
    pub(crate) async fn route_to<F>(
        &self,
        policy_id: &str,
        deployment: &str,
        address: &str,
        connect: F,
    ) -> Result<(), ProviderError>
    where
        F: Future<Output = Result<Arc<VeraDocumentACP>, ProviderError>>,
    {
        let key = route_key(policy_id)?;
        let acp = self
            .client(client_key(deployment, address), connect)
            .await?;
        let route = Route {
            deployment: deployment.to_owned(),
            address: address.to_owned(),
            acp,
        };
        for (other, moved) in self.routes.iter() {
            if moved.deployment == route.deployment && moved.address != route.address {
                let _ = self.routes.replace_if(other, route.clone(), |current| {
                    current.deployment == moved.deployment
                        && current.address == moved.address
                        && Arc::ptr_eq(&current.acp, &moved.acp)
                });
            }
        }
        self.routes.insert(key.clone(), route);
        self.drop_unused_clients();
        if let Some(listener) = self.listener.get() {
            listener.policy_changed(&key);
        }
        Ok(())
    }

    /// The client of one home at one address, built by `connect` when no
    /// add built it before. A build that fails leaves no entry behind once
    /// no other add waits on it, so a home that was never reached is not
    /// kept; an add that took the entry as it went builds it again and puts
    /// it back.
    async fn client<F>(
        &self,
        name: String,
        connect: F,
    ) -> Result<Arc<VeraDocumentACP>, ProviderError>
    where
        F: Future<Output = Result<Arc<VeraDocumentACP>, ProviderError>>,
    {
        let client = self.clients.get_or_insert(name.clone(), Arc::default());
        client.adds.fetch_add(1, Ordering::SeqCst);
        let built = client.acp.get_or_try_init(connect).await.map(Arc::clone);
        let last = client.adds.fetch_sub(1, Ordering::SeqCst) == 1;
        match built {
            Ok(acp) => {
                self.clients.insert_if_absent(name, client);
                Ok(acp)
            }
            Err(error) => {
                let unbuilt =
                    |held: &Arc<HomeClient>| Arc::ptr_eq(held, &client) && held.acp.get().is_none();
                if last && self.clients.get(&name).is_some_and(|held| unbuilt(&held)) {
                    self.clients.remove(&name);
                }
                Err(error)
            }
        }
    }

    /// Stop deciding `policy_id` against another deployment: the node's own
    /// decides it again. Whether it was routed elsewhere. What waits on the
    /// policy's home leaves the database's index, and a home no policy is
    /// routed to any more is no longer followed.
    pub fn remove(&self, policy_id: &str) -> bool {
        let Ok(key) = route_key(policy_id) else {
            return false;
        };
        if self.routes.remove(&key).is_none() {
            return false;
        }
        self.drop_unused_clients();
        if let Some(listener) = self.listener.get() {
            listener.policy_removed(&key);
        }
        true
    }

    /// Drop the built clients no route uses: a home that moved, or that no
    /// policy is routed to any more. One still being built is kept.
    fn drop_unused_clients(&self) {
        let used: Vec<String> = self
            .routes
            .iter()
            .map(|(_, route)| client_key(&route.deployment, &route.address))
            .collect();
        for (key, client) in self.clients.iter() {
            if client.acp.get().is_some() && !used.contains(&key) {
                self.clients.remove(&key);
            }
        }
    }

    /// Tell `listener` whenever a home's state moves. The first listener
    /// stays.
    pub fn watch(&self, listener: Arc<dyn acp::ObjectChangeListener>) {
        let _ = self.listener.set(listener);
    }

    fn home_changed(&self, deployment: &str) {
        let Some(listener) = self.listener.get() else {
            return;
        };
        for (policy, route) in self.routes.iter() {
            if route.deployment == deployment {
                listener.policy_changed(&policy);
            }
        }
    }
}

fn claims_full(error: claims::ClaimsFull) -> ProviderError {
    ProviderError::PolicyChangesBusy {
        operation: error.operation,
        limit: error.limit,
    }
}

/// The name a home's client is shared under: its deployment and address.
fn client_key(deployment: &str, address: &str) -> String {
    format!("{deployment}@{address}")
}

#[cfg(test)]
#[path = "foreign_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "foreign_create_tests.rs"]
mod create_tests;
