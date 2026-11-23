//! A provider that follows its deployment when a second device takes it over.
//!
//! Two devices of one account run the deployment: a laptop, active, and a
//! phone, its backup. The phone is away while the laptop finalizes a grant
//! and a registration the provider sends; then the laptop goes down, and the
//! user confirms a takeover on the phone. Its cut is the phone's newest
//! block, so the grant and the registration are void in the new term,
//! although they verified. The provider reached the laptop and now reaches
//! the phone. It takes the new term only under its certificate, which names
//! the deployment's genesis, sends again what the term voided, and decides
//! from the new term's blocks once it has. A provider without the genesis
//! identifier refuses the new term and reads nothing.
//!
//! `HostedNode` hosts one device and cannot switch, so both devices are
//! `vera_node::EmbeddedNode`s driven here; the provider is the one a
//! DefraDB node builds, over in-process transports.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_core::thread_bounds::MaybeBoxFuture;
use futures::future::BoxFuture;
use regolith::sync::{ArrayQueue, Atom};
use serde_json::Value;
use vera::vera_rs::{
    AcpTransport, ClientError, GenesisSource, HeaderStream, NativeWorker, RpcTransport,
    TransportError, VeraRsProvider,
};
use vera::{AcpTuning, VeraProvider};
use vera_client::term::{approve_term, TermCertificate, TermChange};
use vera_embedded::EmbeddedTransport;
use vera_node::{
    Device, DeviceKey, EmbedError, EmbeddedConfig, EmbeddedNode, SoloDeployment, SoloGenesis,
    Switching, TermRefusal,
};

const CHAIN_ID: u64 = 9_461;
const ACCOUNT_KEY: &str = "0279be667ef9dcbbac55a06295ce870b07029bfcdb2dce28d959f2815b16f81798";
const POLICY: &str = "\
name: takeover
resources:
  - name: document
    permissions:
      - name: read
        expr: reader
    relations:
      - name: reader
        types:
          - actor
";
const READER: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

/// The in-process channel to whichever device the test points it at.
struct Following {
    current: Atom<EmbeddedTransport>,
    proof_gate: ArrayQueue<ProofGate>,
}

struct ProofGate {
    started: futures::channel::oneshot::Sender<()>,
    release: futures::channel::oneshot::Receiver<()>,
}

impl Following {
    fn to(&self, node: &EmbeddedNode, edge: &edge::Edge) {
        self.current
            .store(EmbeddedTransport::new(node.rpc().clone(), timeout(), edge));
    }
}

impl RpcTransport for Following {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<Value, ClientError>> {
        let current = self.current.load_clone();
        Box::pin(async move { RpcTransport::call(&current, method, params, max_bytes).await })
    }
}

impl AcpTransport for Following {
    fn call(
        &self,
        method: &'static str,
        params: Value,
        max_bytes: usize,
    ) -> BoxFuture<'_, Result<Value, TransportError>> {
        let current = self.current.load_clone();
        let gate = if method == "vera_getCurrentPolicyPrefixProof" {
            self.proof_gate.pop()
        } else {
            None
        };
        Box::pin(async move {
            let result = AcpTransport::call(&current, method, params, max_bytes).await;
            if let Some(gate) = gate.filter(|_| result.is_ok()) {
                let _ = gate.started.send(());
                let _ = gate.release.await;
            }
            result
        })
    }

    fn subscribe_headers(&self) -> BoxFuture<'_, Result<HeaderStream, TransportError>> {
        let current = self.current.load_clone();
        Box::pin(async move { current.subscribe_headers().await })
    }
}

fn timeout() -> Duration {
    AcpTuning::default().request_timeout
}

fn free_address() -> SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .unwrap()
        .local_addr()
        .unwrap()
}

fn now() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

/// One device's settings as the active device and as the other's backup.
struct Paired {
    key: DeviceKey,
    active: EmbeddedConfig,
    backup: EmbeddedConfig,
}

fn pair(directories: [&Path; 2], deployment: &SoloDeployment) -> [Paired; 2] {
    let keys = [
        DeviceKey::generate().unwrap(),
        DeviceKey::generate().unwrap(),
    ];
    let addresses = [free_address(), free_address()];
    std::array::from_fn(|this| {
        let other = 1 - this;
        let mut active =
            EmbeddedConfig::adopt(directories[this], deployment.clone(), addresses[this]).unwrap();
        active.switching = Some(Switching::new(
            keys[this].clone(),
            vec![Device {
                key: keys[other].public_key(),
                address: addresses[other],
            }],
        ));
        active.pacing = None;
        active.worker_threads = Some(1);
        active.verification_threads = 2;
        let backup = active.following(&keys[other].public_key()).unwrap();
        Paired {
            key: keys[this].clone(),
            active,
            backup,
        }
    })
}

/// `change` approved by the deployment's only operator, whose secret key is 1.
fn certify(change: TermChange) -> TermCertificate {
    let mut secret = [0u8; 32];
    secret[31] = 1;
    let key = vera_client::Secp256k1SigningKey::from_bytes((&secret).into()).unwrap();
    TermCertificate {
        change,
        approvals: vec![approve_term(&change, 0, &key).unwrap()],
    }
}

async fn height(node: &EmbeddedNode) -> u64 {
    let status: vera_jsonrpc::NodeStatus = node.rpc().call_empty("vera_nodeStatus").await.unwrap();
    status.finalized_height.unwrap_or(0)
}

async fn run_until(node: &EmbeddedNode, reached: u64) {
    let deadline = Instant::now() + Duration::from_secs(120);
    while height(node).await < reached {
        assert!(Instant::now() < deadline, "stalled below {reached}");
        nuclei::time::sleep(Duration::from_millis(50)).await;
    }
}

/// Retries `attempt` while it fails, for at most `patience`.
async fn eventually<T, E, F, Fut>(patience: Duration, mut attempt: F) -> T
where
    E: std::fmt::Debug,
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
{
    let deadline = Instant::now() + patience;
    loop {
        match attempt().await {
            Ok(value) => return value,
            Err(error) => {
                assert!(Instant::now() < deadline, "still failing: {error:?}");
                nuclei::time::sleep(Duration::from_millis(200)).await;
            }
        }
    }
}

/// The submission the provider's worker journaled as confirmed under `sequence`.
fn journaled(root: &Path, sequence: u64) -> Value {
    let entry = std::fs::read(root.join(format!("worker/confirmed/{sequence:020}.entry"))).unwrap();
    serde_json::from_slice::<Value>(&entry).unwrap()["submission"].clone()
}

/// Whether `node` serves a receipt for `submission`.
async fn holds(node: &EmbeddedNode, submission: &Value) -> bool {
    let receipt: Option<Value> = node
        .rpc()
        .request("vera_getReceiptProof", vec![submission.clone()])
        .await
        .unwrap();
    receipt.is_some()
}

/// Whether `READER` holds `reader` on "kept", as `owner` observes it.
async fn observe(
    provider: &VeraRsProvider,
    owner: &str,
    policy: &str,
) -> Result<bool, vera::ProviderError> {
    provider
        .query_actor_relationship(vera::ActorRelationshipQuery {
            requester: owner,
            actor: READER,
            policy,
            resource: "document",
            object: "kept",
            relation: "reader",
        })
        .await
}

async fn provider(
    transport: Arc<Following>,
    consensus_key: &str,
    genesis: GenesisSource,
    root: &Path,
    edge: &edge::Edge,
) -> VeraRsProvider {
    let keyring = keyring::FileKeyring::open(root.join("keys"), b"test-password").unwrap();
    let worker = NativeWorker::open(&root.join("worker"), &keyring, CHAIN_ID).unwrap();
    VeraRsProvider::with_transport(
        transport.clone(),
        transport,
        consensus_key,
        genesis,
        &[42; 32],
        worker,
        &AcpTuning::default(),
        None,
        edge,
    )
    .await
    .expect("provider")
}

#[nuclei::test]
async fn a_provider_follows_a_confirmed_takeover_and_sends_again_what_it_voided() {
    let directories = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    let root = tempfile::tempdir().unwrap();
    let edge = edge::testing::edge();
    let identity = SoloGenesis {
        chain_id: CHAIN_ID,
        operators: serde_json::from_value(serde_json::json!({
            "threshold": 1,
            "keys": [ACCOUNT_KEY],
        }))
        .unwrap(),
        timestamp: now(),
        policies: Vec::new(),
    };
    let deployment = SoloDeployment::create(&identity, free_address()).unwrap();
    let [laptop_device, phone_device] =
        pair([directories[0].path(), directories[1].path()], &deployment);
    let laptop = EmbeddedNode::start(laptop_device.active.clone())
        .await
        .unwrap();
    let phone = EmbeddedNode::start(phone_device.backup.clone())
        .await
        .unwrap();
    let consensus_key = hex::encode(commonware_codec::Encode::encode(laptop.consensus_key()));

    let transport = Arc::new(Following {
        current: Atom::new(EmbeddedTransport::new(
            laptop.rpc().clone(),
            timeout(),
            &edge,
        )),
        proof_gate: ArrayQueue::new(1),
    });
    let named = provider(
        transport.clone(),
        &consensus_key,
        GenesisSource::Hosted,
        &root.path().join("named"),
        &edge,
    )
    .await;
    let unnamed = provider(
        transport.clone(),
        &consensus_key,
        GenesisSource::Unnamed,
        &root.path().join("unnamed"),
        &edge,
    )
    .await;
    let owner = named.self_did().unwrap();
    let delegation = named.create_bearer_token(&owner).await.unwrap();
    let policy = named.create_policy(POLICY, None).await.unwrap();
    named
        .register_object(&delegation, &policy, "document", "kept")
        .await
        .unwrap();
    // The phone holds everything so far.
    run_until(&phone, height(&laptop).await).await;

    // The phone is away while the laptop finalizes a grant and a
    // registration. The laptop first holds its proposals, so no block is on
    // its way to the phone when it stops.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match laptop.hand_over(&phone_device.key.public_key()).await {
            Ok(_) => break,
            Err(EmbedError::Term(
                TermRefusal::Unsettled | TermRefusal::SuccessorNotCurrent { .. },
            )) if Instant::now() < deadline => nuclei::time::sleep(Duration::from_secs(1)).await,
            Err(error) => panic!("{error}"),
        }
    }
    phone.stop().await.unwrap();
    laptop.resume().await.unwrap();
    let reader = vera::SubjectRef::Actor(READER.into());
    named
        .set_relationship(&delegation, &policy, "document", "kept", "reader", &reader)
        .await
        .unwrap();
    named
        .register_object(&delegation, &policy, "document", "lost")
        .await
        .unwrap();
    assert!(named
        .verify_access(&policy, "document", "kept", "read", READER)
        .await
        .unwrap());
    let query = vera::AssignmentQuery {
        policy: &policy,
        resource: "document",
        relation: "reader",
        subject: READER,
        issuer: &owner,
        not_before: None,
    };
    let before = named.read_certified_assignments(query).await.unwrap();
    assert_eq!(before.object_ids, ["kept"]);
    assert_eq!(before.position.term, 0);
    let (started, started_rx) = futures::channel::oneshot::channel();
    let (release, release_rx) = futures::channel::oneshot::channel();
    assert!(transport
        .proof_gate
        .push(ProofGate {
            started,
            release: release_rx,
        })
        .is_ok());
    let in_flight = named.read_certified_assignments(query);
    futures::pin_mut!(in_flight);
    match futures::future::select(in_flight.as_mut(), started_rx).await {
        futures::future::Either::Left((answer, _)) => {
            panic!("assignment read completed before takeover: {answer:?}")
        }
        futures::future::Either::Right((started, _)) => started.unwrap(),
    }
    let laptop_head = height(&laptop).await;
    laptop.stop().await.unwrap();

    // The user confirms the takeover on the phone; its cut is its own newest
    // block, below the grant.
    let mut phone = EmbeddedNode::start(phone_device.backup.clone())
        .await
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(60);
    let change = loop {
        match phone.takeover_change().await {
            Ok(change) => break change,
            // Still executing blocks it holds; each call waits for that up
            // to the device's answer timeout.
            Err(EmbedError::Term(TermRefusal::NotAtCut { head, cut })) => {
                assert!(
                    Instant::now() < deadline,
                    "the phone stayed at block {cut} below finalized block {head}"
                );
                nuclei::time::sleep(Duration::from_millis(200)).await;
            }
            Err(error) => panic!("{error}"),
        }
    };
    assert!(change.cut.height < laptop_head);
    phone
        .take_over(deployment.keys().clone(), certify(change))
        .await
        .unwrap();
    assert_eq!(phone.term(), 1);
    // The grant (sequence 2) and the registration (3) are void: the phone
    // never had them. The registration before them (1) is kept.
    let named_root = root.path().join("named");
    let [kept, granted, lost] = [1, 2, 3].map(|sequence| journaled(&named_root, sequence));
    assert!(holds(&phone, &kept).await);
    assert!(!holds(&phone, &granted).await && !holds(&phone, &lost).await);
    transport.to(&phone, &edge);

    // The provider takes the new term under its certificate and sends the
    // grant and the registration again; then it decides from the new term.
    let followed = Duration::from_secs(120);
    assert!(
        eventually(followed, || named
            .verify_access(&policy, "document", "kept", "read", READER))
        .await,
        "the grant the term voided holds again"
    );
    let (registered, registrant) = eventually(followed, || {
        named.query_object_owner(&policy, "document", "lost")
    })
    .await;
    assert!(registered, "the registration the term voided holds again");
    assert_eq!(registrant, owner);
    // They were sent again exactly as signed: the phone holds the same
    // submissions now.
    assert!(holds(&phone, &granted).await && holds(&phone, &lost).await);
    let status = named.acp_light_client_status().unwrap();
    assert!(status.height > change.cut.height);
    assert!(named.decision_state().unwrap().starts_with("1:"));

    // An exact relationship observation is answered from the new term too.
    assert!(
        observe(&named, &owner, &policy).await.unwrap(),
        "the observation follows the term"
    );

    let after = named.read_certified_assignments(query).await.unwrap();
    assert_eq!(after.object_ids, ["kept"]);
    assert_eq!(after.position.term, 1);
    assert_ne!(after.position.block_hash, before.position.block_hash);
    release.send(()).unwrap();
    assert!(
        in_flight.await.is_err(),
        "a read overtaken by a term change must not be accepted"
    );
    assert!(unnamed.read_certified_assignments(query).await.is_err());

    assert!(named
        .delete_relationship(&delegation, &policy, "document", "kept", "reader", &reader)
        .await
        .unwrap());
    let empty = named
        .read_certified_assignments(vera::AssignmentQuery {
            not_before: Some(&after.position),
            ..query
        })
        .await
        .unwrap();
    assert!(empty.object_ids.is_empty());
    assert_eq!(empty.position.term, 1);
    assert!(empty.position.height > after.position.height);

    // Without the genesis identifier, the term above 0 is refused: no read,
    // and no observation, is answered from the new term.
    assert!(unnamed
        .verify_access(&policy, "document", "kept", "read", &owner)
        .await
        .is_err());
    assert!(
        observe(&unnamed, &owner, &policy).await.is_err(),
        "an observation is not answered from an unchecked term"
    );

    phone.stop().await.unwrap();
}
