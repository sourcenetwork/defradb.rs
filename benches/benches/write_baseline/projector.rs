//! An installed document projector that changes nothing, for the cost a
//! projector adds to every mutation (`DEFRA_BASELINE_PROJECTOR=empty`).

use std::sync::Arc;

use db::{DocumentMutationContext, DocumentMutationProjector, DB};
use storage::RegolithStore;

struct EmptyProjector;

#[async_trait::async_trait]
impl DocumentMutationProjector for EmptyProjector {
    async fn reconcile(
        &self,
        _context: &DocumentMutationContext<'_>,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }

    async fn project(
        &self,
        _context: &DocumentMutationContext<'_>,
        _collection: &str,
        _doc_id: &str,
    ) -> storage::corekv::Result<()> {
        Ok(())
    }
}

/// Whether the run installs the empty projector.
pub fn requested() -> bool {
    match std::env::var("DEFRA_BASELINE_PROJECTOR").as_deref() {
        Ok("empty") => true,
        Ok("none") | Err(_) => false,
        Ok(other) => panic!("DEFRA_BASELINE_PROJECTOR must be none or empty, not {other}"),
    }
}

/// Install the empty projector on `db` before it serves requests.
pub fn install(db: &Arc<DB<RegolithStore>>) {
    db.set_document_projector(
        Arc::new(EmptyProjector),
        Arc::new(acp::LocalDocumentACP::new(Arc::new(
            acp::MemoryAcpStore::new(),
        ))),
        identity::Did::new("did:key:z6MkbaselineProjector").unwrap(),
    )
    .unwrap();
}
