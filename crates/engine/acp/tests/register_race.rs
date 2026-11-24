//! Registrations of one document racing on separate workers: exactly one
//! wins, and the document ends with one owner (`AcpStore::register_doc_atomic`).

use std::sync::Arc;
use std::time::Duration;

use acp::{AcpStore, PersistentAcpStore};
use identity::Did;
use storage::{RegolithStore, RegolithStoreOptions};

/// Every wait carries a deadline, so a regression fails instead of hanging.
const DEADLINE: Duration = Duration::from_secs(120);
/// Workers registering each document at the same moment, one owner each.
const CONTENDERS: usize = 4;
/// Documents raced, one round each.
const DOCS: usize = 128;

fn owner(n: usize) -> Did {
    Did::new(format!("did:key:z6MkRegisterRaceOwner{n}")).unwrap()
}

/// Each round, every worker passes a barrier and registers the same document
/// for its own owner. Each owner writes a different key, so only the commit
/// can refuse all but the first.
#[test]
fn racing_registrations_leave_one_owner_per_document() {
    let pool = storage::start_pool(workers::Config::unplaced(CONTENDERS), None).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let options = RegolithStoreOptions::default().placed_on(&pool);
    let store = Arc::new(RegolithStore::open_with_options(dir.path(), options).unwrap());
    let acp = Arc::new(PersistentAcpStore::from_store(store));
    let barrier = Arc::new(regolith::sync::Barrier::new(CONTENDERS));

    let runs: Vec<_> = (0..CONTENDERS)
        .map(|worker| {
            let acp = acp.clone();
            let barrier = barrier.clone();
            workers::block_on(
                pool.submit(workers::Target::Worker(worker), move || async move {
                    let me = owner(worker);
                    let mut won = Vec::with_capacity(DOCS);
                    for doc in 0..DOCS {
                        barrier.wait().await;
                        let doc_id = format!("doc{doc}");
                        won.push(
                            acp.register_doc_atomic(&me, "users", &doc_id)
                                .await
                                .unwrap(),
                        );
                    }
                    won
                }),
            )
            .unwrap()
            .unwrap()
        })
        .collect();
    let won: Vec<Vec<bool>> = runs
        .into_iter()
        .map(|run| run.wait_timeout(DEADLINE).unwrap())
        .collect();

    for doc in 0..DOCS {
        let doc_id = format!("doc{doc}");
        let winners = won.iter().filter(|run| run[doc]).count();
        let owners = workers::block_on(acp.get_doc_tuples("users", &doc_id))
            .unwrap()
            .unwrap()
            .len();
        assert_eq!(
            (winners, owners),
            (1, 1),
            "{doc_id}: {winners} registrations won, {owners} owners stored"
        );
    }
    pool.shutdown_blocking().unwrap();
}
