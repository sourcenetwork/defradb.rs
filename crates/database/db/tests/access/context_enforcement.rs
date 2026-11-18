//! Node access control checks the identity acting in the request's context,
//! and nothing a thread or task carries beside it.

use acp::nac::NodePermission;
use acp::MemoryZanzibarStore;
use db::{NacConfig, NacManager, DB};
use defra_core::RequestContext;
use identity::Did;
use std::sync::Arc;
use storage::RegolithStore;

const OWNER_DID: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";
const STRANGER_DID: &str = "did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR";

fn acting(did: &str) -> RequestContext {
    RequestContext::default().with_identity(Some(did.to_string()))
}

async fn guarded_db() -> DB<RegolithStore> {
    let store = Arc::new(MemoryZanzibarStore::new());
    let manager = Arc::new(NacManager::new(
        store,
        NacConfig::new().with_enabled().with_dev_mode(),
    ));
    manager
        .initialize(Some(&Did::new(OWNER_DID).unwrap()))
        .await
        .unwrap();
    let db = DB::new(RegolithStore::in_memory().unwrap()).unwrap();
    db.set_nac_manager(manager);
    db
}

#[test]
fn the_identity_acting_in_the_context_is_the_one_checked() {
    workers::block_on(async {
        let db = guarded_db().await;

        db.check_node_access_as(&acting(OWNER_DID), NodePermission::DocumentUpdate)
            .await
            .expect("the owner acting in the context must be allowed");
        let denied = db
            .check_node_access_as(&acting(STRANGER_DID), NodePermission::DocumentUpdate)
            .await
            .expect_err("a stranger acting in the context must be denied");
        assert!(
            matches!(denied, db::error::Error::NotAuthorized { .. }),
            "expected NotAuthorized, got: {denied:?}"
        );
    })
    .unwrap()
}

#[test]
fn an_identity_on_the_thread_grants_a_request_nothing() {
    workers::block_on(async {
        let db = guarded_db().await;

        // The owner set the old way, on this thread, must not reach a request
        // whose context names no one.
        defra_core::current_identity::set_current_identity(Some(OWNER_DID.to_string()));
        let result = db
            .check_node_access_as(&RequestContext::default(), NodePermission::DocumentUpdate)
            .await;
        defra_core::current_identity::set_current_identity(None);

        assert!(
            matches!(result, Err(db::error::Error::NotAuthorized { .. })),
            "an anonymous request was allowed as the identity on its thread: {result:?}"
        );
    })
    .unwrap()
}
