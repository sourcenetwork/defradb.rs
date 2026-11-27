//! Two nodes of one account share a Vera deployment: a laptop, active, and
//! a phone, its backup. Both hold the deployment's key material, as a host
//! keeps it in a keystore, so neither directory holds a key file.
//!
//! The phone verifies reads against its own Vera node. The laptop hands the
//! active role to the phone and follows it, and a document the laptop then
//! creates owes its registration: a backup refuses submissions, and one in
//! flight when the laptop follows gets no answer. The phone settles its own
//! documents from its first create after the switch on. The phone
//! stops without a handover, the laptop takes the deployment over through
//! the node, which sends what it owes, and the registration settles.

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use defra_node::vera_embedded::{
    Device, DeviceKey, HostError, SoloDeployment, Switching, TermCertificate, TermChange,
    TermRefusal,
};
use defra_node::{
    EmbeddedVeraActor, EmbeddedVeraConfig, EmbeddedVeraDeployment, EmbeddedVeraDevice, Node,
};

use vera_client::term::approve_term;

use super::{
    chain_reads, create_user, created, now, reads_doc, users, ACCOUNT_KEY, POLICY, STRANGER,
};

/// How long a switch, a catch-up or a settlement may take before the test fails.
pub(super) const PATIENCE: Duration = Duration::from_secs(150);

/// How long a create may take on a backup, which refuses its registration
/// at once instead of asking a node that takes no submission again.
const BACKUP_CREATE: Duration = Duration::from_secs(5);

/// How a submission says that a switch stopped the Vera node under it.
const STOPPED: &str = "the embedded Vera node has stopped";

pub(super) fn free_address() -> SocketAddr {
    std::net::TcpListener::bind("127.0.0.1:0")
        .and_then(|listener| listener.local_addr())
        .expect("free port")
}

/// One device of the account, as the host configures it.
pub(super) struct Paired {
    pub(super) key: DeviceKey,
    pub(super) listen: SocketAddr,
    other: Device,
}

impl Paired {
    pub(super) fn switching(&self) -> Option<Switching> {
        Some(Switching::new(self.key.clone(), vec![self.other.clone()]))
    }
}

pub(super) fn pair() -> [Paired; 2] {
    let keys = [
        DeviceKey::generate().unwrap(),
        DeviceKey::generate().unwrap(),
    ];
    let listens = [free_address(), free_address()];
    std::array::from_fn(|this| Paired {
        key: keys[this].clone(),
        listen: listens[this],
        other: Device {
            key: keys[1 - this].public_key(),
            address: listens[1 - this],
        },
    })
}

pub(super) async fn device(
    directory: &Path,
    actor: u8,
    listen: SocketAddr,
    device: EmbeddedVeraDevice,
) -> Node {
    let keyring: Arc<dyn keyring::Keyring> = Arc::new(
        keyring::FileKeyring::open(directory.join("keys"), b"test-password").expect("keyring"),
    );
    Node::builder()
        .with_embedded_vera(EmbeddedVeraConfig {
            data_dir: directory.join("vera"),
            deployment: identity(),
            device,
            listen,
            start_deadline: PATIENCE,
            stop_deadline: Duration::from_secs(30),
            actor: EmbeddedVeraActor::Secp256k1Key(vec![actor; 32]),
            rpc_listen: None,
            worker_dir: directory.join("worker"),
            keyring,
            tuning: vera::AcpTuning::default(),
        })
        .build()
        .await
        .expect("node")
}

pub(super) fn identity() -> EmbeddedVeraDeployment {
    EmbeddedVeraDeployment {
        chain_id: 9_471,
        threshold: 1,
        operator_keys: vec![ACCOUNT_KEY.to_string()],
        created: now(),
        policies: Vec::new(),
    }
}

/// `change` approved by the deployment's only operator, whose secret key is 1.
pub(super) fn certify(change: TermChange) -> TermCertificate {
    let mut secret = [0u8; 32];
    secret[31] = 1;
    let key = vera_client::Secp256k1SigningKey::from_bytes((&secret).into()).unwrap();
    TermCertificate {
        change,
        approvals: vec![approve_term(&change, 0, &key).unwrap()],
    }
}

pub(super) fn vera(node: &Node) -> &defra_node::EmbeddedVeraNode {
    node.embedded_vera().expect("embedded Vera node")
}

fn owner(node: &Node) -> identity::Did {
    identity::Did::new(node.vera_acp().unwrap().self_did().unwrap()).unwrap()
}

/// Asks `attempt` again while the node's term refuses it with a refusal
/// that passes, for at most [`PATIENCE`].
pub(super) async fn until_settled<T, F, Fut>(mut attempt: F) -> T
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = Result<T, HostError>>,
{
    let deadline = Instant::now() + PATIENCE;
    loop {
        match attempt().await {
            Ok(value) => return value,
            Err(error)
                if Instant::now() < deadline
                    && matches!(
                        error.term_refusal(),
                        Some(
                            TermRefusal::Unsettled
                                | TermRefusal::SuccessorNotCurrent { .. }
                                | TermRefusal::NotAtCut { .. }
                        )
                    ) =>
            {
                pause().await;
            }
            Err(error) => panic!("{error}"),
        }
    }
}

/// Retries `check` until it holds, for at most [`PATIENCE`].
pub(super) async fn eventually<F, Fut>(what: &str, mut check: F)
where
    F: FnMut() -> Fut,
    Fut: std::future::Future<Output = bool>,
{
    let deadline = Instant::now() + PATIENCE;
    while !check().await {
        assert!(Instant::now() < deadline, "{what}");
        pause().await;
    }
}

pub(super) async fn pause() {
    edge::testing::edge()
        .sleep(Duration::from_millis(250))
        .await;
}

async fn owed(node: &Node, actor: &identity::Did) -> Vec<acp::OwedEntry> {
    node.with_identity(actor.clone())
        .owed_acp(actor.as_str())
        .await
        .expect("what is owed")
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn two_devices_switch_the_active_role_and_settle_what_the_backup_owed() {
    workers::block_on(async {
        let directories = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let [laptop_device, phone_device] = pair();
        let deployment: SoloDeployment = identity().deal(laptop_device.listen).expect("deployment");
        let held = |deployment: &SoloDeployment, paired: &Paired| EmbeddedVeraDevice::Active {
            deployment: Box::new(deployment.clone()),
            switching: paired.switching(),
        };
        let backup = |of: &Paired, paired: &Paired| EmbeddedVeraDevice::Backup {
            record: deployment.record().to_vec(),
            active: of.listen,
            switching: paired.switching(),
        };

        let laptop = device(
            directories[0].path(),
            42,
            laptop_device.listen,
            held(&deployment, &laptop_device),
        )
        .await;
        let phone = device(
            directories[1].path(),
            43,
            phone_device.listen,
            backup(&laptop_device, &phone_device),
        )
        .await;
        for directory in &directories {
            let vera = directory.path().join("vera");
            assert!(!vera.join("validator.key").exists() && !vera.join("secrets.json").exists());
        }
        assert_eq!(
            vera(&phone).consensus_key(),
            vera(&laptop).consensus_key(),
            "both devices verify against the one deployment"
        );

        // The laptop creates a policy and a document; the phone reads them
        // from its own Vera node.
        let laptop_owner = owner(&laptop);
        let stranger = identity::Did::new(STRANGER).unwrap();
        let policy = laptop
            .vera_acp()
            .unwrap()
            .add_policy(laptop_owner.as_str(), POLICY, None)
            .await
            .expect("policy");
        laptop
            .add_collection(&users(&policy))
            .await
            .expect("collection");
        let first = created(
            laptop
                .execute_request(create_user("first", &laptop_owner))
                .await,
        );
        eventually("the phone reads the laptop's policy", || async {
            phone.add_collection(&users(&policy)).await.is_ok()
        })
        .await;
        eventually("the phone verifies the laptop's document", || async {
            reads_doc(&phone, &laptop_owner, &policy, &first).await
        })
        .await;
        assert!(!reads_doc(&phone, &stranger, &policy, &first).await);
        assert!(
            vera(&phone).export().await.is_err(),
            "a backup holds no keys"
        );

        // The laptop hands over to the phone and follows it. The handover
        // holds the laptop's proposals, so the registration of a document
        // the laptop creates from then on gets no answer: the create is
        // still waiting for one when the follow stops its Vera node, and the
        // registration stays owed. The switch starts once the create has
        // committed, which is when the node owes the registration.
        let successor = phone_device.key.public_key();
        let change = until_settled(|| vera(&laptop).hand_over(&successor)).await;
        let certificate = certify(change);
        let settled = owed(&laptop, &laptop_owner).await;
        assert!(settled.is_empty(), "{settled:?}");
        let (in_flight, discarded) = futures::join!(
            laptop.execute_request(create_user("in flight", &laptop_owner)),
            async {
                eventually("the laptop's create owes its registration", || async {
                    !owed(&laptop, &laptop_owner).await.is_empty()
                })
                .await;
                vera(&phone)
                    .take_over(deployment.keys().clone(), certificate.clone())
                    .await
                    .expect("take-over");
                vera(&laptop).follow(certificate).await.expect("follow")
            }
        );
        let in_flight = created(in_flight);
        assert!(discarded.is_empty(), "a handover voids nothing");
        assert_eq!(vera(&phone).term().await.unwrap(), 1);
        assert_eq!(vera(&laptop).term().await.unwrap(), 1);
        assert!(vera(&phone).is_active().await, "the phone took over");
        assert!(!vera(&laptop).is_active().await, "the laptop follows");
        let waiting = owed(&laptop, &laptop_owner).await;
        let unanswered = waiting
            .iter()
            .find(|entry| entry.object == in_flight)
            .unwrap_or_else(|| panic!("the registration in flight is owed: {waiting:?}"));
        assert_eq!(unanswered.what, acp::OwedWhat::Registration, "{waiting:?}");
        assert!(
            unanswered
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains(STOPPED) && !reason.contains("backup")),
            "the follow stopped the node under the registration; no backup refused it: {waiting:?}"
        );

        // The phone, now active, settles its own documents with the create,
        // the first one after the switch included, whose read may be what
        // tells its provider of the new term; the laptop, now a backup,
        // verifies them from its own Vera node.
        let phone_owner = owner(&phone);
        let second = created(
            phone
                .execute_request(create_user("second", &phone_owner))
                .await,
        );
        let left = owed(&phone, &phone_owner).await;
        assert!(
            left.is_empty(),
            "the active phone settles its own registration: {left:?}"
        );
        eventually("the laptop verifies the phone's document", || async {
            reads_doc(&laptop, &phone_owner, &policy, &second).await
        })
        .await;

        // A document created on the backup owes its registration.
        let creating = Instant::now();
        let third = created(
            laptop
                .execute_request(create_user("third", &laptop_owner))
                .await,
        );
        assert!(
            creating.elapsed() < BACKUP_CREATE,
            "a create on a backup took {:?}",
            creating.elapsed()
        );
        let waiting = owed(&laptop, &laptop_owner).await;
        assert_eq!(waiting.len(), 2, "{waiting:?}");
        let owed_third = waiting
            .iter()
            .find(|entry| entry.object == third)
            .unwrap_or_else(|| panic!("the backup's document is owed: {waiting:?}"));
        assert_eq!(owed_third.what, acp::OwedWhat::Registration, "{waiting:?}");
        assert!(
            owed_third
                .reason
                .as_deref()
                .is_some_and(|reason| reason.contains("backup")),
            "the registration was refused as a backup: {waiting:?}"
        );
        assert!(reads_doc(&laptop, &laptop_owner, &policy, &third).await);
        let still = laptop
            .with_identity(laptop_owner.clone())
            .send_owed_acp(laptop_owner.as_str())
            .await
            .expect("a backup lists what it still owes");
        assert_eq!(still.len(), 2, "{still:?}");

        // The phone stops without handing over, and the laptop takes over
        // from its own newest block.
        phone.shutdown().await.expect("phone shutdown");
        drop(phone);
        let change = until_settled(|| vera(&laptop).takeover_change()).await;
        assert_eq!(change.term, 2);
        laptop
            .take_over_embedded_vera(deployment.keys().clone(), certify(change))
            .await
            .expect("take-over after the phone stopped");
        assert_eq!(vera(&laptop).term().await.unwrap(), 2);
        assert!(vera(&laptop).is_active().await);

        // What the laptop owed while it was a backup settled with the
        // take-over, without another call.
        let left = owed(&laptop, &laptop_owner).await;
        assert!(left.is_empty(), "{left:?}");
        assert!(chain_reads(&laptop, &laptop_owner, &policy, &in_flight).await);
        assert!(chain_reads(&laptop, &laptop_owner, &policy, &third).await);
        assert!(!chain_reads(&laptop, &stranger, &policy, &third).await);
        assert_eq!(
            laptop
                .document_acp()
                .get_doc_owner(&policy, "users", &third)
                .await
                .expect("owner"),
            Some(laptop_owner)
        );
        laptop.shutdown().await.expect("laptop shutdown");
        assert!(matches!(
            vera(&laptop).term().await,
            Err(HostError::NotRunning)
        ));
    })
    .unwrap();
}
