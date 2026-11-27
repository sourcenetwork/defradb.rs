use std::sync::atomic::{AtomicUsize, Ordering};

use acp::{DocumentACP, DocumentPermission, Identity};
use async_trait::async_trait;
use identity::Did;

use super::*;
use crate::provider::{SubjectRef, VeraProvider};

pub(super) const OWN: &str = "1111111111111111111111111111111111111111111111111111111111111111";
pub(super) const SHARED: &str = "2222222222222222222222222222222222222222222222222222222222222222";
pub(super) const ACTOR: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
pub(super) const YAML: &str = "name: shared\nresources:\n  - name: users\n";

/// A provider that answers every permission with `allows`, or is away, and
/// names `owner` as every object's owner; it counts what it is asked.
pub(super) struct Answers {
    allows: Option<bool>,
    owner: &'static str,
    asked: AtomicUsize,
    commands: AtomicUsize,
    /// The policies this provider holds, when it is the node's own: what
    /// it was made with and what it created.
    held: std::sync::Mutex<Vec<String>>,
    /// The actor it creates policies as, under the ID the nonce names; a
    /// provider with no actor creates under `creates`.
    actor: Option<&'static str>,
    creates: Option<String>,
    created: AtomicUsize,
    /// Whether a create lets the other task in a join run once before it
    /// lands.
    yields: bool,
}

impl Answers {
    pub(super) fn new(allows: Option<bool>, owner: &'static str) -> Arc<Self> {
        Arc::new(Self::answering(allows, owner))
    }

    fn answering(allows: Option<bool>, owner: &'static str) -> Self {
        Self {
            allows,
            owner,
            asked: AtomicUsize::new(0),
            commands: AtomicUsize::new(0),
            held: std::sync::Mutex::new(Vec::new()),
            actor: None,
            creates: None,
            created: AtomicUsize::new(0),
            yields: false,
        }
    }

    /// The node's own provider, holding `policy`.
    fn holding(policy: &'static str) -> Arc<Self> {
        let holding = Self::answering(Some(true), "did:key:z6Mkowner");
        holding.held.lock().expect("held").push(policy.to_owned());
        Arc::new(holding)
    }

    /// The node's own provider, whose actor `actor` creates policies under
    /// the ID their nonce names, as vera.rs does.
    pub(super) fn creating(actor: &'static str) -> Arc<Self> {
        Arc::new(Self {
            actor: Some(actor),
            ..Self::answering(Some(true), "did:key:z6Mkowner")
        })
    }

    /// [`Self::creating`], letting the other task in a join run once while
    /// each create is in flight.
    pub(super) fn creating_slowly(actor: &'static str) -> Arc<Self> {
        Arc::new(Self {
            actor: Some(actor),
            yields: true,
            ..Self::answering(Some(true), "did:key:z6Mkowner")
        })
    }

    /// The node's own provider with no actor, whose ACP names a policy only
    /// once it is created: every create makes `creates`.
    pub(super) fn naming_on_create(creates: &'static str) -> Arc<Self> {
        Arc::new(Self {
            creates: Some(creates.to_owned()),
            ..Self::answering(Some(true), "did:key:z6Mkowner")
        })
    }

    pub(super) fn created(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }

    fn asked(&self) -> usize {
        self.asked.load(Ordering::Relaxed)
    }

    fn commands(&self) -> usize {
        self.commands.load(Ordering::Relaxed)
    }

    fn command(&self) -> Result<(), ProviderError> {
        self.commands.fetch_add(1, Ordering::Relaxed);
        Ok(())
    }
}

#[async_trait]
impl VeraProvider for Answers {
    fn authorized_account(&self) -> String {
        String::new()
    }

    async fn create_bearer_token(&self, did: &str) -> Result<String, ProviderError> {
        Ok(format!("token.{did}"))
    }

    fn self_did(&self) -> Option<String> {
        self.actor.map(str::to_owned)
    }

    async fn create_policy(
        &self,
        yaml: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError> {
        self.created.fetch_add(1, Ordering::Relaxed);
        if self.yields {
            yield_once().await;
        }
        let id = match (self.actor, nonce) {
            (Some(actor), Some(nonce)) => crate::vera_rs::predict_policy_id(yaml, nonce, actor)?,
            (Some(_), None) => panic!("a create the node sent with no nonce"),
            (None, _) => self.creates.clone().expect("a provider that creates"),
        };
        self.held.lock().expect("held").push(id.clone());
        Ok(id)
    }

    async fn register_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &[acp::DerivedRelationship],
    ) -> Result<(), ProviderError> {
        self.command()
    }

    async fn archive_object(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
    ) -> Result<(), ProviderError> {
        self.command()
    }

    async fn set_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.command().map(|()| true)
    }

    async fn delete_relationship(
        &self,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &str,
        _: &SubjectRef,
    ) -> Result<bool, ProviderError> {
        self.command().map(|()| true)
    }

    async fn query_policy(
        &self,
        policy_id: &str,
    ) -> Result<Option<crate::ProviderPolicyInfo>, ProviderError> {
        Ok(self
            .held
            .lock()
            .expect("held")
            .iter()
            .find(|held| *held == policy_id)
            .map(|held| crate::ProviderPolicyInfo {
                id: held.clone(),
                name: "held".into(),
                raw_policy: None,
            }))
    }

    async fn query_object_owner(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
    ) -> Result<(bool, String), ProviderError> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        match self.allows {
            Some(_) => Ok((true, self.owner.to_owned())),
            None => Err(ProviderError::Unavailable("the home is away".into())),
        }
    }

    async fn verify_access(
        &self,
        _policy_id: &str,
        _resource: &str,
        _object_id: &str,
        _permission: &str,
        _actor_did: &str,
    ) -> Result<bool, ProviderError> {
        self.asked.fetch_add(1, Ordering::Relaxed);
        self.allows
            .ok_or_else(|| ProviderError::Unavailable("the home is away".into()))
    }
}

/// Records which policies it was told changed and which were removed.
#[derive(Default)]
struct Told {
    policies: kovan_queue::seg_queue::SegQueue<String>,
    removed: kovan_queue::seg_queue::SegQueue<String>,
}

impl acp::ObjectChangeListener for Told {
    fn changed(&self, _policy_id: &str, _resource_name: &str, _object_id: &str) {}

    fn policy_changed(&self, policy_id: &str) {
        self.policies.push(policy_id.to_owned());
    }

    fn policy_removed(&self, policy_id: &str) {
        self.removed.push(policy_id.to_owned());
    }

    fn waiting_under(&self, policy_id: &str) -> usize {
        usize::from(policy_id == SHARED)
    }
}

impl Told {
    fn take(&self) -> Vec<String> {
        std::iter::from_fn(|| self.policies.pop()).collect()
    }

    fn take_removed(&self) -> Vec<String> {
        std::iter::from_fn(|| self.removed.pop()).collect()
    }
}

/// Route `policy` of `node` to `home`, a deployment of its own at one
/// address.
pub(super) async fn route(node: &VeraDocumentACP, policy: &str, home: Arc<VeraDocumentACP>) {
    route_at(node, policy, "home", "http://home", home)
        .await
        .expect("route");
}

pub(super) async fn route_at(
    node: &VeraDocumentACP,
    policy: &str,
    deployment: &str,
    address: &str,
    home: Arc<VeraDocumentACP>,
) -> Result<(), ProviderError> {
    node.route_for_test(policy, deployment, address, async move { Ok(home) })
        .await
}

fn did(name: &str) -> Did {
    Did::new(format!("did:key:z6Mk{name}")).expect("DID")
}

pub(super) fn acp_over(provider: Arc<Answers>) -> Arc<VeraDocumentACP> {
    Arc::new(VeraDocumentACP::new(provider))
}

async fn may(acp: &VeraDocumentACP, policy: &str) -> acp::Result<bool> {
    acp.check_doc_access(
        &Identity::Authenticated(did("guest")),
        DocumentPermission::Update,
        policy,
        "users",
        "doc",
    )
    .await
}

/// A policy the registry routes is decided by its home and every other by
/// the node's own provider; once removed, by the node's own again.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_routed_policy_is_decided_by_its_home_and_every_other_by_the_node() {
    let own = Answers::new(Some(false), "did:key:z6Mkowner");
    let home = Answers::new(Some(true), "did:key:z6Mkhome");
    let node = acp_over(own.clone());
    route(&node, SHARED, acp_over(home.clone())).await;

    assert!(node.is_foreign_policy(SHARED));
    assert!(node.is_foreign_policy(&format!("0x{}", SHARED.to_uppercase())));
    assert!(!node.is_foreign_policy(OWN));
    assert!(may(&node, SHARED).await.unwrap());
    assert!(!may(&node, OWN).await.unwrap());
    assert_eq!(
        node.get_doc_owner(SHARED, "users", "doc").await.unwrap(),
        Some(Did::new("did:key:z6Mkhome").unwrap())
    );
    assert!(node
        .is_doc_registered(SHARED, "users", "doc")
        .await
        .unwrap());
    assert_eq!(home.asked(), 3);
    assert_eq!(own.asked(), 1);

    assert!(node.foreign_policies().remove(SHARED));
    assert!(!node.is_foreign_policy(SHARED));
    assert!(!may(&node, SHARED).await.unwrap());
    assert_eq!(home.asked(), 3, "a removed route asks its home nothing");
}

/// Nothing under a foreign policy is sent to either provider: the node's
/// own does not decide it, and its home takes no command from this node.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn no_command_under_a_foreign_policy_is_sent_anywhere() {
    let own = Answers::new(Some(true), "did:key:z6Mkowner");
    let home = Answers::new(Some(true), "did:key:z6Mkhome");
    let node = acp_over(own.clone());
    route(&node, SHARED, acp_over(home.clone())).await;
    let refused = |result: acp::Result<()>| {
        assert!(
            matches!(result, Err(acp::Error::ForeignPolicy { ref policy, .. }) if policy == SHARED),
            "{result:?}"
        );
    };
    refused(
        node.register_doc_object(&did("guest"), SHARED, "users", "doc", &[])
            .await,
    );
    refused(
        node.add_actor_relationship(
            &did("guest"),
            &did("other"),
            SHARED,
            "users",
            "doc",
            "writer",
            &[],
        )
        .await
        .map(|_| ()),
    );
    refused(
        node.delete_actor_relationship(
            &did("guest"),
            &did("other"),
            SHARED,
            "users",
            "doc",
            "writer",
            &[],
        )
        .await
        .map(|_| ()),
    );
    refused(
        node.archive_doc_object(&did("guest"), SHARED, "users", "doc")
            .await,
    );
    node.unregister_doc_object(SHARED, "users", "doc")
        .await
        .expect("a delete archives nothing of a foreign policy");
    assert_eq!(own.commands() + home.commands(), 0);

    node.register_doc_object(&did("guest"), OWN, "users", "doc", &[])
        .await
        .expect("the node's own policy registers as before");
    assert_eq!(own.commands(), 1);
}

/// A home that cannot be reached fails retryably and is never a denial.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn an_unreachable_home_is_unavailable_and_never_a_denial() {
    let node = acp_over(Answers::new(Some(true), "did:key:z6Mkowner"));
    route(&node, SHARED, acp_over(Answers::new(None, ""))).await;
    assert!(matches!(
        may(&node, SHARED).await,
        Err(acp::Error::ProviderUnavailable(_))
    ));
    assert!(matches!(
        node.get_doc_owner(SHARED, "users", "doc").await,
        Err(acp::Error::ProviderUnavailable(_))
    ));
}

/// Routing a policy, removing it, and a new state of its home each tell
/// the listener of that policy only, in its one spelling however it was
/// added: a route and a new state as a change, a removal as a removal.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn the_listener_hears_of_each_route_and_each_new_home_state() {
    let node = acp_over(Answers::new(Some(true), "did:key:z6Mkowner"));
    let told = Arc::new(Told::default());
    node.watch_object_changes(told.clone());
    let registry = node.foreign_policies();
    let spelled = format!("0x{}", SHARED.to_uppercase());
    route(&node, &spelled, acp_over(Answers::new(Some(true), ""))).await;
    route_at(
        &node,
        OWN,
        "other",
        "http://other",
        acp_over(Answers::new(Some(true), "")),
    )
    .await
    .expect("route");
    assert_eq!(told.take(), vec![SHARED.to_owned(), OWN.to_owned()]);
    assert_eq!(registry.waiting(&spelled), 1, "asked in its one spelling");

    registry.home_changed("home");
    assert_eq!(told.take(), vec![SHARED.to_owned()]);

    assert!(registry.remove(&spelled));
    assert!(!registry.remove(SHARED));
    assert!(told.take().is_empty(), "a removal asks nothing again");
    assert_eq!(told.take_removed(), vec![SHARED.to_owned()]);
    assert_eq!(registry.policies(), vec![OWN.to_owned()]);
}

/// A policy the node's own deployment holds is never routed to another
/// home: that home would decide the node's own documents under it.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_policy_the_node_holds_is_never_routed_elsewhere() {
    let node = acp_over(Answers::holding(SHARED));
    let spelled = format!("0x{}", SHARED.to_uppercase());
    let refused = route_at(
        &node,
        &spelled,
        "home",
        "http://home",
        acp_over(Answers::new(Some(true), "")),
    )
    .await;
    assert!(
        matches!(&refused, Err(ProviderError::OwnPolicyRouted { policy }) if policy == SHARED),
        "{refused:?}"
    );
    assert!(refused.unwrap_err().to_string().contains(&format!(
        "policy {SHARED} is held by this node's own deployment"
    )));
    assert!(!node.is_foreign_policy(SHARED));
    assert!(
        !node.foreign_policies().claimed(SHARED),
        "the claim is released"
    );
}

/// Let the other task in a join run once before going on.
pub(super) async fn yield_once() {
    let mut yielded = false;
    std::future::poll_fn(|cx| {
        if yielded {
            return std::task::Poll::Ready(());
        }
        yielded = true;
        cx.waker().wake_by_ref();
        std::task::Poll::Pending
    })
    .await;
}

/// Adds of one home at one address build one client, however many ask at
/// once; an add at a new address builds another and moves every policy of
/// that home to it.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn one_client_per_home_address_and_a_new_address_moves_the_home() {
    let node = acp_over(Answers::new(Some(true), "did:key:z6Mkowner"));
    let built = Arc::new(AtomicUsize::new(0));
    let first = acp_over(Answers::new(Some(true), ""));
    let connect = |acp: Arc<VeraDocumentACP>| {
        let built = Arc::clone(&built);
        async move {
            built.fetch_add(1, Ordering::Relaxed);
            yield_once().await;
            Ok(acp)
        }
    };
    let (a, b) = futures::join!(
        node.route_for_test(SHARED, "home", "http://one", connect(first.clone())),
        node.route_for_test(OWN, "home", "http://one", connect(first.clone())),
    );
    a.expect("route");
    b.expect("route");
    assert_eq!(built.load(Ordering::Relaxed), 1, "one build for both adds");
    let registry = node.foreign_policies();
    assert!(Arc::ptr_eq(&registry.route(OWN).unwrap(), &first));

    let moved = acp_over(Answers::new(Some(true), ""));
    node.route_for_test(SHARED, "home", "http://two", connect(moved.clone()))
        .await
        .expect("route");
    assert_eq!(built.load(Ordering::Relaxed), 2);
    for policy in [SHARED, OWN] {
        assert!(
            Arc::ptr_eq(&registry.route(policy).unwrap(), &moved),
            "every policy of the home moves to its new address"
        );
    }
    node.route_for_test(OWN, "home", "http://two", connect(moved.clone()))
        .await
        .expect("route");
    assert_eq!(
        built.load(Ordering::Relaxed),
        2,
        "the moved home's client is reused"
    );
}

/// A home whose client could not be built keeps no entry: the failed add
/// leaves nothing behind, and the next add of that home builds its client.
#[nuclei::test]
#[mantra_macros::req_test("defra.acp.foreign_policy")]
async fn a_client_that_failed_to_build_leaves_no_entry() {
    let node = acp_over(Answers::new(Some(true), "did:key:z6Mkowner"));
    let registry = node.foreign_policies();
    let failed = node
        .route_for_test(SHARED, "home", "http://away", async {
            Err(ProviderError::Unavailable("the home is away".into()))
        })
        .await;
    assert!(
        matches!(failed, Err(ProviderError::Unavailable(_))),
        "{failed:?}"
    );
    assert!(
        registry.clients.is_empty(),
        "no entry is kept for a failed build"
    );
    assert!(!node.is_foreign_policy(SHARED));
    assert!(!registry.claimed(SHARED), "the claim is released");

    let home = acp_over(Answers::new(Some(true), ""));
    route_at(&node, SHARED, "home", "http://away", home.clone())
        .await
        .expect("the next add builds the client");
    assert!(Arc::ptr_eq(&registry.route(SHARED).unwrap(), &home));
    assert_eq!(registry.clients.len(), 1);
}
