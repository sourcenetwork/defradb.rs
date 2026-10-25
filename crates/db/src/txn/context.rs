//! One request's view of an explicit transaction, on the worker that owns it.
//!
//! The owner lends the transaction to the request (`OwnedTxn`'s turn), and
//! the context hands it to the request's handles: one lensed fetcher for the
//! whole request, a mutator and a collection provider made on demand. They
//! share the transaction through `&self`, with no lock. None of them holds
//! it: the owner does, and takes it back once no operation of theirs is in
//! flight; a handle kept past its request finds the transaction gone.

use std::sync::{Arc, Weak};

use query::fetcher::CollectionProvider;
use query::mutator::DocMutator;
use query::runner::DocFetcher;
use query::txn::{AcpRegistrationProjection, TransactionContext};
use storage::corekv::Store;

use crate::collection::provider::TxnCollectionProvider;
use crate::database::DB;
use crate::event::emission::TxnBroadcaster;
use crate::read::lensed::fetcher::{FetcherCaches, LensedDocFetcher};
use crate::txn::{DbTxn, LentTxn};
use crate::write::doc::DbDocMutator;

/// One request's view of an explicit transaction.
pub struct DbTransactionContext<S: Store + 'static> {
    db: Arc<DB<S>>,
    id: String,
    readonly: bool,
    txn: Weak<DbTxn<S>>,
    fetcher: Arc<LensedDocFetcher<S>>,
    broadcaster: Option<Arc<dyn TxnBroadcaster>>,
}

impl<S: Store + 'static> DbTransactionContext<S> {
    pub(crate) fn new(
        db: Arc<DB<S>>,
        id: String,
        readonly: bool,
        txn: &LentTxn<S>,
        lens_store: Arc<dyn lens::TransformStore>,
        caches: FetcherCaches,
        broadcaster: Option<Arc<dyn TxnBroadcaster>>,
    ) -> Self {
        let fetcher = Arc::new(LensedDocFetcher::lent(db.clone(), txn, lens_store, caches));
        Self {
            db,
            id,
            readonly,
            txn: Arc::downgrade(txn),
            fetcher,
            broadcaster,
        }
    }
}

impl<S: Store + 'static> TransactionContext for DbTransactionContext<S> {
    fn id(&self) -> &str {
        &self.id
    }

    fn is_readonly(&self) -> bool {
        self.readonly
    }

    fn doc_fetcher(&self) -> Arc<dyn DocFetcher> {
        self.fetcher.clone()
    }

    fn doc_mutator(&self) -> Option<Arc<dyn DocMutator>> {
        if self.readonly {
            return None;
        }
        let txn = self.txn.upgrade()?;
        Some(Arc::new(DbDocMutator::lent_with_broadcaster(
            self.db.clone(),
            &txn,
            self.broadcaster.clone(),
        )))
    }

    fn collection_provider(&self) -> Option<Arc<dyn CollectionProvider>> {
        let txn = self.txn.upgrade()?;
        Some(Arc::new(TxnCollectionProvider::new(self.db.clone(), &txn)))
    }

    #[cfg_attr(target_arch = "wasm32", allow(clippy::arc_with_non_send_sync))]
    fn acp_registration_projection(&self) -> Option<Arc<AcpRegistrationProjection>> {
        Some(
            self.txn
                .upgrade()
                .map(|txn| txn.acp_registration_projection())
                .unwrap_or_else(|| Arc::new(AcpRegistrationProjection::detached())),
        )
    }
}
