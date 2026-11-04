//! The HTTP routes that stay on the network edge (GraphQL, transactions, P2P,
//! document ACP, events) check node permissions through the NAC adapter,
//! whose store reads run on the pool: none of them is made off it, which the
//! store counts.

use std::sync::Arc;

use axum::body::Body;
use axum::http::{Method, Request, StatusCode};
use storage::corekv::Store;
use tower::ServiceExt;

use super::NacAdapter;

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
