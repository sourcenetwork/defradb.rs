//! A collection patch inside an explicit transaction is one statement: a
//! patch that fails leaves nothing behind in the transaction, and one that
//! succeeds is what the transaction's later requests see.

use std::sync::Arc;

use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher, DB};
use defra_core::RequestContext;
use query::txn::TransactionRegistry;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::json;
use storage::RegolithStore;

type Runner =
    QueryRunner<LensedAutoCommitFetcher<RegolithStore>, DbTransactionRegistry<RegolithStore>>;

/// A database with `Users` and `Books`, its registry, and a runner over both.
async fn node() -> (
    Arc<DB<RegolithStore>>,
    Arc<DbTransactionRegistry<RegolithStore>>,
    Runner,
) {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let ctx = RequestContext::default();
    db.add_collection(&ctx, "type Users { name: String }")
        .await
        .unwrap();
    db.add_collection(&ctx, "type Books { title: String }")
        .await
        .unwrap();
    let registry = Arc::new(DbTransactionRegistry::new(Arc::clone(&db)));
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::clone(&registry),
    );
    (db, registry, runner)
}

#[test]
fn a_failed_patch_leaves_nothing_in_the_callers_transaction() {
    workers::block_on(async {
        let (db, registry, runner) = node().await;
        let ctx = RequestContext::default();
        let seed = registry.begin_as(&ctx, false).await.unwrap();
        let book = runner
            .execute_in_txn(
                QueryRequest::new(r#"mutation { add_Books(input: {title: "Dune"}) { title } }"#),
                &seed,
            )
            .await;
        assert!(!book.has_errors(), "{:?}", book.errors);
        registry.commit_as(&ctx, &seed).await.unwrap();
        let users_before = db
            .get_collection("Users")
            .unwrap()
            .unwrap()
            .schema()
            .clone();

        let handle = registry.begin_as(&ctx, false).await.unwrap();
        let patch = r#"[
            {"op":"add","path":"/Users/Fields/-","value":{"Name":"email","Kind":"String"}},
            {"op":"replace","path":"/Books/IsActive","value":false}
        ]"#;
        let error = registry
            .patch_collection_in_txn_as(&ctx, handle.as_str(), patch, None)
            .await
            .expect_err("Books holds a document, so it may not be deactivated");
        assert!(error.to_string().contains("has documents"), "{error}");
        registry.commit_as(&ctx, &handle).await.unwrap();

        let users: Vec<_> = db
            .get_all_collection_versions()
            .await
            .unwrap()
            .into_iter()
            .filter(|version| version.name == "Users")
            .collect();
        assert_eq!(
            users,
            vec![users_before.clone()],
            "the failed patch left a Users version in the transaction"
        );
        assert_eq!(
            db.get_collection("Users").unwrap().unwrap().schema(),
            &users_before,
            "the failed patch left a Users cache update in the transaction"
        );
    })
    .unwrap()
}

#[test]
fn requests_after_a_patch_in_the_transaction_use_the_patched_collection() {
    workers::block_on(async {
        let (_db, registry, runner) = node().await;
        let ctx = RequestContext::default();
        let handle = registry.begin_as(&ctx, false).await.unwrap();
        let in_txn = |query: &'static str| runner.execute_in_txn(QueryRequest::new(query), &handle);

        let before = in_txn("{ Users { name } }").await;
        assert!(!before.has_errors(), "{:?}", before.errors);
        registry
            .patch_collection_in_txn_as(
                &ctx,
                handle.as_str(),
                r#"[{"op":"add","path":"/Users/Fields/-","value":{"Name":"email","Kind":"String"}}]"#,
                None,
            )
            .await
            .unwrap();

        let created =
            in_txn(r#"mutation { add_Users(input: {name: "Ada", email: "ada@x"}) { name } }"#)
                .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let read = in_txn("{ Users { name email } }").await;
        assert_eq!(
            read.data,
            Some(json!({ "Users": [{ "name": "Ada", "email": "ada@x" }] }))
        );

        registry.discard_as(&ctx, &handle).await.unwrap();
    })
    .unwrap()
}
