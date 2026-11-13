//! A page read by `DbDocFetcher`, which holds a transaction and no database,
//! for a provider that stands outside that transaction.

use std::sync::Arc;

use acp::DocumentACP;
use db::{DbDocFetcher, DbTxn};
use query::fetcher::{DocFetcher, DocumentReader, DocumentUpdateOptions, DocumentUpdatePage};
use storage::RegolithStore;

use super::decisions::DeletesMidPage;
use super::{owner, Node};

/// The first page of `Users`, read through `txn` as `identity`.
async fn page(
    txn: DbTxn<RegolithStore>,
    acp: &dyn DocumentACP,
    identity: acp::Identity,
) -> query::error::Result<DocumentUpdatePage> {
    DbDocFetcher::new(txn)
        .get_document_updates(
            &DocumentUpdateOptions {
                collection: "Users".into(),
                after: 0,
                limit: 10,
            },
            &DocumentReader {
                context: &defra_core::RequestContext::default(),
                acp,
                projection: None,
                identity: &identity,
            },
        )
        .await
}

fn documents(page: DocumentUpdatePage) -> Vec<String> {
    page.entries.into_iter().map(|entry| entry.doc_id).collect()
}

#[test]
fn a_delete_during_the_page_hides_the_row() {
    workers::block_on(async {
        let node = Node::outside().await;
        let removed = node.updated_document("removed").await;
        let kept = node.updated_document("kept").await;
        let racing = DeletesMidPage::new(node.acp.clone(), node.db.clone(), removed.clone());

        let read = page(
            node.db.new_txn(true).await.unwrap(),
            &racing,
            acp::Identity::Anonymous,
        )
        .await
        .unwrap();
        assert_eq!(read.next, 2);
        assert!(
            read.entries.is_empty(),
            "deleted after the page began, public to the provider: {:?}",
            read.entries
        );

        let owned = page(
            node.db.new_txn(true).await.unwrap(),
            node.acp.as_ref(),
            acp::Identity::Authenticated(owner()),
        )
        .await
        .unwrap();
        assert_eq!(documents(owned), [kept]);
    })
    .unwrap();
}

#[test]
fn a_page_in_a_writing_transaction_reads_its_later_snapshot_too() {
    workers::block_on(async {
        let node = Node::outside().await;
        let kept = node.updated_document("kept").await;
        let read = page(
            node.db.new_txn(false).await.unwrap(),
            node.acp.as_ref(),
            acp::Identity::Authenticated(owner()),
        )
        .await
        .unwrap();
        assert_eq!(documents(read), [kept]);
    })
    .unwrap();
}

#[test]
fn a_transaction_no_database_began_says_why_it_cannot_confirm() {
    workers::block_on(async {
        let node = Node::outside().await;
        node.updated_document("kept").await;
        let store: &Arc<RegolithStore> = node.db.store();
        let detached = DbTxn::new(datastore::BasicTxn::new(&**store, 1, true).await.unwrap());
        let refused = page(
            detached,
            node.acp.as_ref(),
            acp::Identity::Authenticated(owner()),
        )
        .await
        .unwrap_err();
        assert!(
            refused
                .to_string()
                .contains("no database began this transaction"),
            "{refused}"
        );
    })
    .unwrap();
}
