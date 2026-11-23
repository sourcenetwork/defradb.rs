//! The HTTP routes that stay on the network edge (GraphQL, transactions, P2P,
//! document ACP, events) check node permissions through the NAC adapter,
//! whose store reads run on the pool: none of them is made off it, which the
//! store counts.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use storage::corekv::Store;
use tower::ServiceExt;

use defra_http::nac_adapter::NacAdapter;

const OWNER: &str = "did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK";

#[nuclei::test(threads = 2)]
async fn edge_routes_check_permissions_on_the_pool() {
    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let stats = store.transaction_stats_handle().unwrap();
    let nac: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
        Arc::new(acp::PersistentZanzibarStore::from_store(Arc::clone(&store))),
        db::NacConfig::new().with_enabled(),
    ));
    nac.initialize(Some(&identity::Did::new(OWNER).unwrap()))
        .await
        .unwrap();
    let adapter = Arc::new(NacAdapter::new(nac).placed_on(Some(&pool)));
    let router = defra_http::Server::with_config(
        defra_http::mock::MockQueryExecutor::new(),
        defra_http::ServerConfig::default(),
        edge::testing::edge(),
    )
    .with_nac_arc(adapter)
    .with_pool(Some(pool.clone()))
    .router()
    .unwrap();

    let before = stats.snapshot().off_pool_calls;
    let query = r#"{"query":"{ __typename }"}"#;
    for (method, uri, body, transaction) in [
        (Method::POST, "/api/v0/graphql", query, None),
        (Method::POST, "/api/v0/graphql", query, Some("1")),
        (Method::GET, "/api/v0/p2p/info", "", None),
        (Method::GET, "/api/v0/acp/policy", "", None),
        (Method::GET, "/api/v0/events", "", None),
    ] {
        let mut request = Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json");
        if let Some(id) = transaction {
            request = request.header("x-defradb-tx", id);
        }
        let request = request.body(Body::from(body)).unwrap();
        let response = router.clone().oneshot(request).await.unwrap();
        // An anonymous caller is refused once its permission was checked.
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    assert_eq!(
        stats.snapshot().off_pool_calls,
        before,
        "a route read the NAC's store off the pool"
    );
    pool.shutdown_blocking().unwrap();
}

/// The edge's batched check over the real NAC manager names the first
/// permission missing, in order, as checking them one at a time does.
#[test]
fn batched_check_names_the_first_missing_permission() {
    use acp::nac::NodePermission::{CollectionGet, DocumentUpdate};
    use defra_http::router::{NodeAcpOperations, PermissionCheck};

    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let nac: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
        Arc::new(acp::PersistentZanzibarStore::from_store(Arc::clone(&store))),
        db::NacConfig::new().with_enabled(),
    ));
    let owner = identity::Did::new(OWNER).unwrap();
    let reader = identity::Did::wildcard();
    let adapter = NacAdapter::new(Arc::clone(&nac)).placed_on(Some(&pool));
    workers::block_on(async {
        nac.initialize(Some(&owner)).await.unwrap();
        let both = [CollectionGet, DocumentUpdate];
        assert_eq!(
            adapter.check_permissions(&owner, &both).await,
            PermissionCheck::Granted
        );
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Denied(CollectionGet)
        );
        nac.add_permission_grant(&owner, &reader, CollectionGet)
            .await
            .unwrap();
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Denied(DocumentUpdate)
        );
        nac.add_permission_grant(&owner, &reader, DocumentUpdate)
            .await
            .unwrap();
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Granted
        );
    })
    .unwrap();
    pool.shutdown_blocking().unwrap();
}

/// The edge answers from a snapshot of the grants, and a revocation that has
/// returned is in it: the next check is refused, through the adapter and
/// through the manager itself.
#[test]
fn a_returned_revocation_refuses_the_next_edge_check() {
    use acp::nac::NodePermission::{CollectionGet, DocumentUpdate};
    use defra_http::router::{NodeAcpOperations, PermissionCheck};

    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let nac: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
        Arc::new(acp::PersistentZanzibarStore::from_store(Arc::clone(&store))),
        db::NacConfig::new().with_enabled(),
    ));
    let owner = identity::Did::new(OWNER).unwrap();
    let reader = identity::Did::wildcard();
    let adapter = NacAdapter::new(Arc::clone(&nac)).placed_on(Some(&pool));
    workers::block_on(async {
        nac.initialize(Some(&owner)).await.unwrap();
        let both = [CollectionGet, DocumentUpdate];
        for permission in both {
            nac.add_permission_grant(&owner, &reader, permission)
                .await
                .unwrap();
        }
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Granted
        );

        nac.remove_permission_grant(&owner, &reader, DocumentUpdate)
            .await
            .unwrap();
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Denied(DocumentUpdate),
            "the edge still granted a permission whose revocation had returned"
        );
        assert!(!nac.check_permission(&reader, DocumentUpdate).await.unwrap());
        assert!(nac.check_permission(&reader, CollectionGet).await.unwrap());
        // Disabling NAC is a write too: every check passes once it returns.
        let caller = db::NacCaller {
            identity: Some(owner.clone()),
            node: None,
        };
        nac.disable_as(&caller).await.unwrap();
        assert_eq!(
            adapter.check_permissions(&reader, &both).await,
            PermissionCheck::Granted
        );
    })
    .unwrap();
    pool.shutdown_blocking().unwrap();
}

/// The edge's snapshot decides every permission ahead of time. Across the
/// grant sets the edge tests cover, and admin, all-actors and NAC-off states,
/// its answer for every identity and every permission is the engine's, and so
/// is the first refusal of the edge's batched check.
#[test]
fn the_snapshot_decides_every_permission_as_the_engine_does() {
    use acp::nac::NodePermission::{self, CollectionGet, DocumentUpdate};
    use defra_http::router::{NodeAcpOperations, PermissionCheck};

    let pool = workers::Pool::start(storage::with_worker_io(workers::Config::unplaced(1))).unwrap();
    let options = storage::RegolithStoreOptions::memory().placed_on(&pool);
    let store =
        Arc::new(storage::RegolithStore::open_with_options("regolith-memory", options).unwrap());
    let zanzibar = Arc::new(acp::PersistentZanzibarStore::from_store(Arc::clone(&store)));
    let nac: Arc<dyn db::NacManagerApi> = Arc::new(db::NacManager::new(
        Arc::clone(&zanzibar),
        db::NacConfig::new().with_enabled(),
    ));
    let owner = identity::Did::new(OWNER).unwrap();
    let reader =
        identity::Did::new("did:key:z6MkjchhfUsD6mmvni8mCdXHw216Xrm9bQe2mBH1P5RDjVJG").unwrap();
    let stranger =
        identity::Did::new("did:key:z6MkpTHR8VNsBxYAAWHut2Geadd9jSwuBV8xRoAnwWsdvktH").unwrap();
    let everyone = identity::Did::wildcard();
    let adapter = NacAdapter::new(Arc::clone(&nac)).placed_on(Some(&pool));
    let both = [CollectionGet, DocumentUpdate];

    // Every identity's every permission through the manager (the snapshot)
    // against a fresh engine over the same store, and the edge's batched
    // check against the first permission the engine refuses.
    let agree = |stage: &'static str| {
        let nac = Arc::clone(&nac);
        let zanzibar = Arc::clone(&zanzibar);
        let adapter = &adapter;
        let identities = [
            owner.clone(),
            reader.clone(),
            stranger.clone(),
            everyone.clone(),
        ];
        async move {
            let engine = acp::nac::NodeACP::new(zanzibar);
            engine.load().await.unwrap();
            for identity in &identities {
                for &permission in NodePermission::all() {
                    assert_eq!(
                        nac.check_permission(identity, permission).await.unwrap(),
                        engine.check_permission(identity, permission).await.unwrap(),
                        "{stage}: {identity:?} {permission:?}"
                    );
                }
                let mut expected = PermissionCheck::Granted;
                for permission in both {
                    if !engine.check_permission(identity, permission).await.unwrap() {
                        expected = PermissionCheck::Denied(permission);
                        break;
                    }
                }
                assert_eq!(
                    adapter.check_permissions(identity, &both).await,
                    expected,
                    "{stage}: {identity:?}"
                );
            }
        }
    };

    workers::block_on(async {
        nac.initialize(Some(&owner)).await.unwrap();
        agree("neither").await;
        nac.add_permission_grant(&owner, &reader, DocumentUpdate)
            .await
            .unwrap();
        agree("only update-document").await;
        nac.remove_permission_grant(&owner, &reader, DocumentUpdate)
            .await
            .unwrap();
        nac.add_permission_grant(&owner, &reader, CollectionGet)
            .await
            .unwrap();
        agree("only get-collection").await;
        nac.add_permission_grant(&owner, &reader, DocumentUpdate)
            .await
            .unwrap();
        agree("both").await;
        nac.add_admin(&owner, &stranger).await.unwrap();
        agree("an admin").await;
        nac.add_permission_grant(&owner, &everyone, CollectionGet)
            .await
            .unwrap();
        agree("get-collection for all actors").await;
        let caller = db::NacCaller {
            identity: Some(owner.clone()),
            node: None,
        };
        nac.disable_as(&caller).await.unwrap();
        agree("NAC off").await;
        nac.re_enable_as(&caller).await.unwrap();
        agree("NAC back on").await;
    })
    .unwrap();
    pool.shutdown_blocking().unwrap();
}
