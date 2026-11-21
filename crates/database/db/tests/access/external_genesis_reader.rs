//! On a provider that cannot join a transaction, the reader a created
//! document's genesis asks for is owed by its creator from the create's own
//! transaction, behind the document's registration: the addressed node, or
//! everyone for a public collection. Nothing else is granted, and a
//! collection the host did not opt in grants nothing.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use acp::{DocumentACP, OwedOp, OwedScope, OwedSubject};
use db::{DocumentMutationContext, DocumentMutationProjector, GenesisReadAccess};
use identity::Did;
use query::QueryExecutor;
use schema::PolicyDescription;
use storage::corekv::Result as StoreResult;
use storage::RegolithStore;

use super::external_registration::{
    alice, bob, gate, mallory, request, RemoteAcp, POLICY, RESOURCE,
};
use super::external_staging::Staging;

/// The host's opt-in: `Records` and `Mutable` name their reader in
/// `addressee`, `Board` is public, `Plain` grants nothing.
struct Host;

#[async_trait::async_trait]
impl DocumentMutationProjector for Host {
    fn genesis_read_access(
        &self,
        collection: &str,
        _policy: &str,
        _resource: &str,
    ) -> Option<GenesisReadAccess> {
        match collection {
            "Records" | "Mutable" => Some(GenesisReadAccess::AddressedBy("addressee")),
            "Board" => Some(GenesisReadAccess::Public),
            _ => None,
        }
    }

    async fn reconcile(&self, _context: &DocumentMutationContext<'_>) -> StoreResult<()> {
        Ok(())
    }

    async fn project(
        &self,
        _context: &DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> StoreResult<()> {
        Ok(())
    }
}

const SCHEMA: &str = "\
type Records { name: String addressee: String @immutable }
type Mutable { name: String addressee: String }
type Board { name: String addressee: String @immutable }
type Plain { name: String addressee: String @immutable }";

async fn host() -> Staging {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let remote = RemoteAcp::new();
    let staging = Staging::on(store.clone(), remote.clone(), false).await;
    for mut schema in query::parse_sdl(SCHEMA).unwrap() {
        schema.policy = Some(PolicyDescription::new(POLICY, RESOURCE));
        staging.node.db.create_collection(schema).await.unwrap();
    }
    let provider: Arc<dyn DocumentACP> = gate(&remote, store);
    staging
        .node
        .db
        .set_document_projector(
            Arc::new(Host),
            provider,
            Did::new("did:key:z6MknodE").unwrap(),
        )
        .unwrap();
    staging
}

impl Staging {
    /// Create a row of `collection` as `creator`, naming `addressee`.
    async fn create_note(&self, collection: &str, creator: Did, addressee: &Did) -> String {
        let created = self
            .node
            .runner
            .execute(request(
                &format!(
                    r#"mutation {{ add_{collection}(input: {{name: "row", addressee: "{addressee}"}}) {{ _docID }} }}"#
                ),
                Some(creator),
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        created.data.unwrap()[format!("add_{collection}")][0]["_docID"]
            .as_str()
            .unwrap()
            .to_string()
    }

    /// The readers owed on `doc`, each as its actor grants it.
    async fn owed_readers(&self, doc: &str) -> Vec<(Did, OwedSubject, OwedOp)> {
        self.queue()
            .await
            .into_iter()
            .filter(|change| change.tuple.object == doc && change.tuple.relation == "reader")
            .map(|change| (change.actor, change.tuple.subject, change.op))
            .collect()
    }
}

#[test]
fn an_addressed_create_owes_its_reader_and_the_provider_takes_it_after_the_registration() {
    workers::block_on(async {
        let staging = host().await;
        // The provider cannot be reached while the create commits, so both
        // the registration and the grant stay owed.
        staging.remote.down.store(true, Ordering::SeqCst);
        let doc = staging.create_note("Records", alice(), &bob()).await;
        assert_eq!(
            staging.owed_readers(&doc).await,
            vec![(alice(), OwedSubject::Actor { did: bob() }, OwedOp::Add)]
        );
        assert_eq!(staging.remote.owner(&doc), None);

        // The stub refuses a change by anyone but the document's owner, so
        // the grant lands only behind the registration.
        staging.remote.down.store(false, Ordering::SeqCst);
        staging
            .gate
            .settle_owed_changes_once(OwedScope::All)
            .await
            .unwrap();
        assert_eq!(staging.remote.owner(&doc), Some(alice()));
        assert!(staging.remote.holds(&doc, &bob()));
        assert!(!staging.remote.holds(&doc, &mallory()));
        assert!(staging.owed_readers(&doc).await.is_empty());
    })
    .unwrap()
}

#[test]
fn a_public_create_owes_everyone_as_its_reader() {
    workers::block_on(async {
        let staging = host().await;
        staging.remote.down.store(true, Ordering::SeqCst);
        let doc = staging.create_note("Board", alice(), &bob()).await;
        assert_eq!(
            staging.owed_readers(&doc).await,
            vec![(alice(), OwedSubject::Wildcard, OwedOp::Add)]
        );
    })
    .unwrap()
}

#[test]
fn a_mutable_address_or_a_collection_not_opted_in_owes_no_reader() {
    workers::block_on(async {
        let staging = host().await;
        staging.remote.down.store(true, Ordering::SeqCst);
        for collection in ["Mutable", "Plain"] {
            let doc = staging.create_note(collection, alice(), &bob()).await;
            assert!(
                staging.owed_readers(&doc).await.is_empty(),
                "{collection} owes a reader"
            );
        }
    })
    .unwrap()
}

impl Staging {
    /// Create `count` public `Board` rows as `creator` in one mutation, in
    /// `handle`'s transaction or one of its own.
    async fn create_board_batch(
        &self,
        creator: Did,
        count: usize,
        handle: Option<&query::TransactionHandle>,
    ) -> Vec<String> {
        let inputs: Vec<String> = (0..count)
            .map(|n| format!(r#"{{name: "row {n}", addressee: "{}"}}"#, bob()))
            .collect();
        let mutation = request(
            &format!(
                "mutation {{ add_Board(input: [{}]) {{ _docID }} }}",
                inputs.join(", ")
            ),
            Some(creator),
        );
        let created = match handle {
            Some(handle) => self.node.runner.execute_in_txn(mutation, handle).await,
            None => self.node.runner.execute(mutation).await,
        };
        assert!(!created.has_errors(), "{:?}", created.errors);
        created.data.unwrap()["add_Board"]
            .as_array()
            .unwrap()
            .iter()
            .map(|doc| doc["_docID"].as_str().unwrap().to_string())
            .collect()
    }
}

/// More creates in one transaction than it may stage changes, then an
/// ordinary grant in the same transaction: every create owes its reader
/// and the grant is still staged, as the readers are counted with their
/// registrations.
#[test]
fn a_batch_past_the_transactions_change_bound_owes_every_reader_and_takes_another_grant() {
    workers::block_on(async {
        let staging = host().await;
        staging.remote.down.store(true, Ordering::SeqCst);
        let count = acp::outbox::MAX_OWED_CHANGES_PER_TRANSACTION as usize + 1;
        let handle = staging.begin().await;
        let docs = staging
            .create_board_batch(alice(), count, Some(&handle))
            .await;
        assert_eq!(docs.len(), count);
        staging
            .stage(
                &handle,
                super::external_staging::Change::new(alice(), &docs[0], OwedOp::Add, bob()),
            )
            .await
            .expect("an ordinary grant after the batch");
        staging.commit(&handle).await;

        let queue = staging.queue().await;
        let readers = queue
            .iter()
            .filter(|change| change.tuple.subject == OwedSubject::Wildcard)
            .count();
        assert_eq!(readers, count);
        assert_eq!(queue.len(), count + 1);
    })
    .unwrap()
}

/// More creates by one actor than it may hold changes unsettled: the batch
/// commits, every create owing its reader.
#[test]
fn a_batch_past_the_actors_unsettled_bound_owes_every_reader() {
    workers::block_on(async {
        let staging = host().await;
        staging.remote.down.store(true, Ordering::SeqCst);
        let count = acp::outbox::MAX_UNSETTLED_OWED_CHANGES_PER_ACTOR as usize + 1;
        let docs = staging.create_board_batch(alice(), count, None).await;
        assert_eq!(docs.len(), count);
        let readers = staging
            .queue()
            .await
            .iter()
            .filter(|change| change.tuple.subject == OwedSubject::Wildcard)
            .count();
        assert_eq!(readers, count);
    })
    .unwrap()
}
