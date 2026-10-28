use super::*;
use blockstore::DefraBlockstore;
use storage::corekv::{self, RetryAttemptFn, RetryTxn};
use storage::RegolithStore;

struct Attempt {
    handler: DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    root: Cid,
    seed: bool,
    generation: [u8; 16],
}

impl RetryAttemptFn for Attempt {
    fn run<'a>(&'a mut self, retry: &'a mut dyn RetryTxn) -> corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let io = retry.begin_attempt_io()?;
            let txn = self
                .handler
                .db
                .new_db_txn(datastore::BasicTxn::from_retry_attempt_io(
                    io,
                    0,
                    self.handler.db.crdts().clone(),
                ));
            let store = txn
                .systemstore()
                .map_err(|error| corekv::RetryAttemptError::Abort(Box::new(error)))?;
            let result = if self.seed {
                let progress = Progress {
                    context: [1; 32],
                    generation: self.generation,
                    ..Progress::default()
                };
                async {
                    write(&store, &state_key(&self.root), &progress).await?;
                    write(
                        &store,
                        &registry::admission_key(&self.root),
                        &Option::<String>::None,
                    )
                    .await?;
                    Ok(MergeOutcome::Yielded)
                }
                .await
            } else {
                self.handler
                    .reject_history(&store, &self.root, &[1; 32], &self.generation, "bad frame")
                    .await
            }
            .map_err(|error| corekv::RetryAttemptError::Abort(Box::new(error)))?;
            drop(store);
            let (io, callbacks) = txn.into_retry_attempt()?;
            retry.finish_attempt_io(io, callbacks)?;
            Ok(Box::new(result) as corekv::RetryAttemptValue)
        })
    }
}

async fn rejection_preserves_replacement(stale: bool) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = Arc::new(DB::from_arc(store.clone()).unwrap());
    let handler = DbMergeHandler::new(
        db.clone(),
        Arc::new(DefraBlockstore::new(store.clone(), true)),
    );
    let root: Cid = "bafyreifbgu2iegwn533qwkl7ayreutckzp23xe5p3uidy6s34w6r56r3zy"
        .parse()
        .unwrap();
    for seed in [true, false] {
        let generation = if !seed && stale { [1; 16] } else { [2; 16] };
        let outcome = corekv::transact::<_, MergeOutcome>(
            &*store,
            db.options().retry_options(),
            None,
            Box::new(Attempt {
                handler: handler.clone(),
                root,
                seed,
                generation,
            }),
        )
        .await
        .unwrap();
        if !seed {
            assert_eq!(
                outcome,
                if stale {
                    MergeOutcome::Yielded
                } else {
                    MergeOutcome::rejected("bad frame")
                }
            );
        }
    }
    let txn = db.new_txn(true).await.unwrap();
    let namespace = txn.systemstore().unwrap();
    assert_eq!(namespace.has(&state_key(&root)).await.unwrap(), stale);
    assert_eq!(
        namespace
            .has(&registry::admission_key(&root))
            .await
            .unwrap(),
        stale
    );
    if stale {
        let progress: Progress = read(&namespace, &state_key(&root)).await.unwrap().unwrap();
        assert!(progress.outcome.is_none());
        assert_eq!(progress.generation, [2; 16]);
    }
}

#[test]
fn stale_rejection_does_not_remove_replacement_cursor() {
    workers::block_on(async {
        rejection_preserves_replacement(true).await;
    })
    .unwrap()
}

#[test]
fn current_rejection_releases_its_own_cursor() {
    workers::block_on(async {
        rejection_preserves_replacement(false).await;
    })
    .unwrap()
}
