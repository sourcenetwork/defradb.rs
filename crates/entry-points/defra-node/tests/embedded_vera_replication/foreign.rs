//! Two accounts on deployments of their own share a collection under the
//! owner's policy: the guest's node decides it against the owner's home,
//! registers nothing on its own deployment, parks what the owner's home
//! refuses and lets it in once the owner's home has a new state that grants
//! it, answers for at most the light client's freshness bound once the
//! owner's home is away and then fails retryably, never with a denial, and
//! never routes a policy its own deployment holds
//! (`proofs/tla/ForeignPolicy.tla`).
//!
//! Each deployment's `Users` policy is created by its own account, so the
//! two deployments' policies have different IDs, as they do between real
//! accounts. The guest's node routes the owner's policy in another spelling
//! than the one its collection names it by.

use std::time::{Duration, Instant};

use acp::DocumentACP as _;
use defra_node::{DocumentPermission, ForeignHome};

use super::actor::Actor;
use super::assembly::{seed, POLICY};
use super::documents::transient;
use super::peer::{Blocks, ForeignRoute, Peer, PeerSpec, Provider};
use super::wait::{eventually, eventually_within, pause, throughout, PATIENCE};

/// How long the light client keeps answering from the newest revision it
/// verified: its freshness bound, the product's stated staleness.
const FRESHNESS: Duration = Duration::from_secs(30);

/// What the bound may be overrun by on a loaded machine: a poll, and the
/// revision time's rounding to whole seconds.
const SLACK: Duration = Duration::from_secs(5);

/// Whether `error` is the retryable failure of a deployment that could not
/// answer: never a denial.
fn retryable(error: &acp::Error) -> bool {
    matches!(error, acp::Error::ProviderUnavailable(_)) || transient(&error.to_string())
}

impl Peer {
    /// Where this peer's deployment is decided from by another account's
    /// node: its consensus key, its genesis identity and its JSON-RPC
    /// address.
    pub fn home(&self) -> ForeignHome {
        let trust = self
            .node
            .embedded_vera_trust()
            .expect("the peer runs its deployment");
        let status = acp::RemotePolicies::light_client_status(
            self.node.vera_acp().expect("vera.rs document ACP").as_ref(),
        )
        .expect("light client status");
        ForeignHome {
            consensus_key: trust.consensus_key.clone(),
            genesis_id: status
                .delegation
                .and_then(|audience| audience.genesis_id)
                .expect("a hosted deployment knows its genesis identity"),
            rpc_address: format!("http://{}", self.rpc.expect("the peer serves JSON-RPC")),
        }
    }

    /// The route another account's node takes to this peer's `Users`
    /// policy: spelled with `0x` and in capitals, as the collection does
    /// not name it.
    fn route(&self) -> ForeignRoute {
        ForeignRoute {
            spelled: format!("0x{}", self.policy.to_uppercase()),
            home: self.home(),
        }
    }

    /// A peer on a deployment of its own, which it serves, whose `Users`
    /// policy `actor` created: the owner's node.
    async fn start_owner(home: &std::path::Path, chain_id: u64, actor: &Actor) -> Self {
        Self::from_spec(Self::spec(
            home,
            Provider::Own {
                chain_id,
                serves: true,
            },
            actor,
            None,
            None,
        ))
        .await
    }

    /// A peer on a deployment of its own, which it serves, whose `Users`
    /// collection is under `owner`'s policy, decided against `owner`'s
    /// home: the guest's node.
    async fn start_guest(
        home: &std::path::Path,
        chain_id: u64,
        actor: &Actor,
        owner: &Peer,
        sweep: Option<Duration>,
    ) -> Self {
        let guest = Self::from_spec(Self::spec(
            home,
            Provider::Own {
                chain_id,
                serves: true,
            },
            actor,
            Some(owner.route()),
            sweep,
        ))
        .await;
        assert_eq!(guest.policy, owner.policy);
        guest
    }

    /// A peer on `host`'s deployment whose `Users` collection is under
    /// `owner`'s policy, decided against `owner`'s home.
    async fn start_sharing_guest(
        home: &std::path::Path,
        host: &Peer,
        actor: &Actor,
        owner: &Peer,
    ) -> Self {
        Self::from_spec(Self::spec(
            home,
            Provider::Shared {
                rpc: host.rpc.expect("the host serves its deployment"),
                trust: host
                    .node
                    .embedded_vera_trust()
                    .expect("the host runs its deployment")
                    .clone(),
            },
            actor,
            Some(owner.route()),
            None,
        ))
        .await
    }

    fn spec(
        home: &std::path::Path,
        provider: Provider,
        actor: &Actor,
        foreign: Option<ForeignRoute>,
        governance_sweep: Option<Duration>,
    ) -> PeerSpec {
        PeerSpec {
            home: home.to_path_buf(),
            port: None,
            provider,
            owner: actor.clone(),
            blocks: Blocks::Signed,
            seed_creator: Some(actor.did.to_string()),
            foreign,
            governance_sweep,
        }
    }

    /// Whether this peer's own deployment holds a registration of `doc`,
    /// asked of it alone: the route of the `Users` policy is removed first.
    pub async fn registers_on_own_deployment(&self, doc: &str) -> bool {
        self.node.remove_foreign_policy(&self.policy);
        let deadline = Instant::now() + PATIENCE;
        loop {
            match self
                .node
                .vera_acp()
                .expect("vera.rs document ACP")
                .is_doc_registered(&self.policy, "users", doc)
                .await
            {
                Ok(registered) => return registered,
                Err(error) if transient(&error.to_string()) && Instant::now() < deadline => {
                    pause().await
                }
                Err(error) => panic!("registration of {doc}: {error}"),
            }
        }
    }

    /// Stop replicating to `other` and drop the connection both ways, while
    /// `other` keeps its subscriptions.
    async fn stop_pushing_to(&self, other: &Peer) {
        let ops = self.node.p2p_system().expect("p2p").ops();
        let other_ops = other.node.p2p_system().expect("p2p").ops();
        let (addr, other_addr) = (self.p2p_addr().await, other.p2p_addr().await);
        ops.remove_replicator(vec!["Users".into()], Some(&other_addr))
            .await
            .expect("replicator removed");
        ops.disconnect_peer(&other_addr)
            .await
            .expect("disconnected");
        other_ops
            .disconnect_peer(&addr)
            .await
            .expect("disconnected");
    }

    /// How many parked objects of the `Users` policy this peer released
    /// because the owner's home reached a new state.
    fn released_under_policy(&self) -> usize {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .foreign_policies()
            .released(&self.policy)
    }

    /// How many objects of the `Users` policy this peer has changes parked
    /// under, as the listener its foreign routes tell reports them.
    fn parked_under_policy(&self) -> usize {
        self.node
            .vera_acp()
            .expect("vera.rs document ACP")
            .foreign_policies()
            .waiting(&self.policy)
    }
}

/// The owner's node, which serves its deployment, and the guest's node on a
/// deployment of its own that decides the owner's policy against the
/// owner's home; the owner's document, on which the guest holds `reader`
/// and `writer`.
pub(super) struct Shared {
    pub(super) a: Peer,
    pub(super) b: Peer,
    pub(super) owner: Actor,
    pub(super) guest: Actor,
    pub(super) doc: String,
}

impl Shared {
    /// The two nodes on `chains`, the guest's sweeping what it parked every
    /// `sweep` when given.
    pub(super) async fn new(
        root: &std::path::Path,
        chains: (u64, u64),
        sweep: Option<Duration>,
    ) -> Self {
        let owner = Actor::new(61);
        let guest = Actor::new(62);
        let a = Peer::start_owner(&root.join("a"), chains.0, &owner).await;
        let b = Peer::start_guest(&root.join("b"), chains.1, &guest, &a, sweep).await;
        let doc = a.create(&owner, "Alice").await;
        // `reader` keeps the guest's node reading the document when its
        // `writer` is revoked, so what it refuses is the guest's write.
        a.grant(&doc, "reader", &guest.did).await;
        a.grant(&doc, "writer", &guest.did).await;
        a.replicate_to(&b).await;
        b.replicate_to(&a).await;
        eventually("the owner's document reaches the guest's node", || async {
            b.points(&guest.did, &doc).await == Some(0)
        })
        .await;
        Self {
            a,
            b,
            owner,
            guest,
            doc,
        }
    }

    pub(super) async fn stop(self) {
        self.b.stop().await;
        self.a.stop().await;
    }
}

/// The guest's change merges on both nodes while the owner's home grants
/// it; the guest's node names the owner's home's owner, and its own
/// deployment holds no registration of the document.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn a_guests_change_merges_on_both_while_the_owners_home_grants_it_and_registers_nothing() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let shared = Shared::new(directory.path(), (9461, 9462), None).await;
        let (a, b, doc) = (&shared.a, &shared.b, &shared.doc);
        assert!(
            b.answer(&shared.guest.did, DocumentPermission::Update, doc)
                .await
        );
        assert_eq!(b.owner_of(doc).await, Some(shared.owner.did.clone()));

        b.write_while_unavailable(
            shared.guest.signed(Peer::add_points(doc, 1)),
            "update_Users",
        )
        .await;
        assert_eq!(b.visible_points(&shared.guest.did, doc).await, 1);
        eventually("the guest's change merges on the owner's node", || async {
            a.points(&shared.owner.did, doc).await == Some(1)
        })
        .await;
        assert_eq!(
            a.commits(&shared.owner.did, doc).await,
            b.commits(&shared.guest.did, doc).await,
            "both nodes hold the same history"
        );

        assert!(
            !b.registers_on_own_deployment(doc).await,
            "the guest's deployment holds no registration of the owner's document"
        );
        shared.stop().await;
    })
    .unwrap();
}

/// A change the guest made on a third node, of the guest's deployment,
/// while it was granted arrives after the owner revoked the guest: both the
/// owner's node and the guest's park it, the guest's under the owner's
/// policy, and the owner's new grant lets it in on both within seconds. The
/// guest's node sweeps nothing in that time: what releases the change there
/// is the owner's home's new state.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn a_revoked_guests_change_parks_on_both_and_a_new_grant_releases_it() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let no_sweep = Duration::from_secs(3_600);
        let shared = Shared::new(directory.path(), (9463, 9464), Some(no_sweep)).await;
        let (a, b, doc) = (&shared.a, &shared.b, &shared.doc);
        let (owner, guest) = (&shared.owner, &shared.guest);
        let third = Actor::new(63);
        let c = Peer::start_sharing_guest(&directory.path().join("c"), b, &third, a).await;
        a.grant(doc, "reader", &third.did).await;
        a.replicate_to(&c).await;
        eventually("the owner's document reaches the third node", || async {
            c.points(&guest.did, doc).await == Some(0)
        })
        .await;

        a.part_from(&c).await;
        let before = c.commits(&guest.did, doc).await;
        c.write_while_unavailable(guest.signed(Peer::add_points(doc, 1)), "update_Users")
            .await;
        let mut change = c.commits_since(&guest.did, doc, &before).await;
        assert_eq!(change.len(), 1, "one change: {change:?}");
        let change = change.remove(0);

        a.revoke(doc, "writer", &guest.did).await;
        for peer in [a, b, &c] {
            eventually("the revocation reaches the node", || async {
                matches!(
                    peer.may(&guest.did, DocumentPermission::Update, doc).await,
                    Ok(false)
                )
            })
            .await;
        }
        c.replicate_to(a).await;
        c.replicate_to(b).await;
        for (peer, reader) in [(a, owner), (b, owner)] {
            eventually("the node holds the revoked guest's change", || async {
                peer.holds_unapplied(&reader.did, doc, &change).await
            })
            .await;
        }
        eventually(
            "the guest's node parks the change under the owner's policy",
            || async { b.parked_under_policy() == 1 },
        )
        .await;
        // Nothing reaches the guest's node again by replication: neither the
        // owner's node, which merges the change once it grants, nor the
        // third node may hand it over a second time.
        a.part_from(b).await;
        b.stop_pushing_to(a).await;
        c.stop_pushing_to(b).await;
        throughout(
            "the revoked guest's change is applied on neither node",
            Duration::from_secs(5),
            || async {
                a.visible_points(&owner.did, doc).await == 0
                    && b.visible_points(&owner.did, doc).await == 0
                    && b.parked_under_policy() == 1
            },
        )
        .await;

        let released = b.released_under_policy();
        a.grant(doc, "writer", &guest.did).await;
        eventually_within(
            "the new grant lets the change in on both nodes",
            Duration::from_secs(15),
            || async {
                a.points(&owner.did, doc).await == Some(1)
                    && b.points(&owner.did, doc).await == Some(1)
            },
        )
        .await;
        assert_eq!(b.parked_under_policy(), 0);
        assert!(
            b.released_under_policy() > released,
            "the owner's home's new state released the change on the guest's node"
        );
        c.stop().await;
        shared.stop().await;
    })
    .unwrap();
}

/// With the owner's node stopped, the guest's node answers from the newest
/// revision it verified for at most the light client's freshness bound,
/// and from then on fails as unavailable: never a denial, and the guest's
/// edit of the owner's document is refused retryably. Its reads report the
/// unanswered check as every read does
/// ([`Peer::reads_with_the_home_away`]).
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn with_the_owners_home_away_the_guests_node_fails_retryably_and_never_denies() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let Shared {
            a, b, guest, doc, ..
        } = Shared::new(directory.path(), (9465, 9466), None).await;
        assert!(b.answer(&guest.did, DocumentPermission::Update, &doc).await);
        a.stop().await;
        let stopped = Instant::now();

        let stale = stopped + FRESHNESS + SLACK;
        loop {
            match b.may(&guest.did, DocumentPermission::Update, &doc).await {
                Err(error) if retryable(&error) => break,
                Ok(false) => panic!("an away home was read as a denial"),
                Ok(true) if Instant::now() < stale => pause().await,
                Ok(true) => panic!("answered past the freshness bound of an away home"),
                Err(error) => panic!("not the retryable failure: {error}"),
            }
        }
        throughout(
            "past the freshness bound an away home is unavailable",
            Duration::from_secs(5),
            || async {
                matches!(
                    b.may(&guest.did, DocumentPermission::Update, &doc).await,
                    Err(error) if retryable(&error)
                )
            },
        )
        .await;
        let response = b
            .node
            .execute_request(guest.signed(Peer::add_points(&doc, 1)))
            .await;
        assert!(
            response.has_errors()
                && response
                    .errors
                    .iter()
                    .all(|error| transient(&error.message)),
            "the guest's edit is refused retryably: {:?}",
            response.errors
        );
        b.reads_with_the_home_away(&guest.did, &doc).await;
        b.stop().await;
    })
    .unwrap();
}

/// A policy one deployment's node holds is never decided by another home:
/// a route to it is refused, and once a route is taken, the node's own
/// deployment neither creates nor adopts a policy of that ID.
#[test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
fn a_policy_of_the_same_id_on_both_deployments_is_refused_in_either_order() {
    workers::block_on(async {
        let directory = tempfile::tempdir().unwrap();
        let owner = Actor::new(64);
        let guest = Actor::new(65);
        let a = Peer::start_serving(&directory.path().join("a"), 9467, &owner).await;
        let b = Peer::start_serving(&directory.path().join("b"), 9468, &guest).await;
        let common = seed().id().expect("policy ID");
        assert_eq!(
            (a.policy.as_str(), b.policy.as_str()),
            (common.as_str(), common.as_str())
        );

        let refused = b
            .node
            .add_foreign_policy(&format!("0x{}", common.to_uppercase()), &a.home())
            .await
            .expect_err("a route to a policy the node's own deployment holds");
        assert!(
            refused.to_string().contains(&format!(
                "policy {common} is held by this node's own deployment"
            )),
            "{refused}"
        );
        assert!(!b.node.remove_foreign_policy(&common), "nothing was routed");

        let nonce = defra_node::PolicyNonce([9; 32]);
        let mine =
            vera::vera_rs::predict_policy_id(POLICY, nonce, guest.did.as_str()).expect("policy ID");
        b.node
            .add_foreign_policy(&mine, &a.home())
            .await
            .expect("a route to a policy the node's own deployment does not hold");
        let as_guest = b.node.with_identity(guest.did.clone());
        let refused = as_guest
            .add_dac_policy(POLICY, Some(nonce))
            .await
            .expect_err("a policy of a routed ID on the node's own deployment");
        assert!(
            refused.to_string().contains(&format!(
                "policy {mine} is decided by another deployment's home"
            )),
            "{refused}"
        );
        assert!(b.node.remove_foreign_policy(&mine));
        assert_eq!(
            as_guest
                .add_dac_policy(POLICY, Some(nonce))
                .await
                .expect("the policy, once no longer routed"),
            mine
        );
        b.stop().await;
        a.stop().await;
    })
    .unwrap();
}
