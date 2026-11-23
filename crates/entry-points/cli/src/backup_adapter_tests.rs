//! The backup adapter sends every write of an import through the host it is
//! given, never through a runner of its own.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use db::backup::{DbJob, ImportHost, RunnerHost};
use db::DB;
use defra_core::thread_bounds::MaybeBoxFuture;
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryResponse, QueryRunner};
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

use crate::backup_adapter::BackupAdapter;

fn users() -> CollectionVersion {
    CollectionVersion::new(
        "User",
        "bafkbackupadapter",
        "backup-adapter",
        vec![
            FieldDescription::new(
                "1",
                "_docID",
                FieldKind::doc_id(),
                schema::CType::LwwRegister,
            ),
            FieldDescription::new("2", "name", FieldKind::string(), schema::CType::LwwRegister),
        ],
    )
}

/// A host that counts what it is sent and runs it on `inner`.
struct Counting {
    inner: RunnerHost,
    writes: AtomicUsize,
    jobs: AtomicUsize,
}

impl ImportHost for Counting {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        self.writes.fetch_add(1, Ordering::SeqCst);
        self.inner.run(request)
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        self.jobs.fetch_add(1, Ordering::SeqCst);
        self.inner.on_db(job)
    }
}

#[nuclei::test(threads = 1)]
async fn every_write_of_an_import_goes_through_the_given_host() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let database = Arc::new(DB::open_from_arc(store).await.unwrap());
    database.create_collection(users()).await.unwrap();
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(
        database.clone(),
    ));
    let runner: Arc<dyn QueryExecutor> = Arc::new(
        QueryRunner::new(db::AutoCommitFetcher::new(database.clone()), vec![users()])
            .with_mutator(mutator as Arc<dyn DocMutator>),
    );
    // The adapter's own runner writes nowhere: only the host reaches the store.
    let adapter = BackupAdapter::new_arc(
        database,
        Arc::new(defra_http::mock::MockQueryExecutor::new()),
    );
    let host = Counting {
        inner: RunnerHost::new(Arc::clone(&runner)),
        writes: AtomicUsize::new(0),
        jobs: AtomicUsize::new(0),
    };

    let result = adapter
        .import(r#"{"User": [{"name": "John"}, {"name": "Bob"}]}"#, &host)
        .await
        .unwrap();

    assert_eq!(result.documents_imported, 2);
    assert_eq!(
        host.writes.load(Ordering::SeqCst),
        1,
        "one save holds both documents"
    );
    assert_eq!(
        host.jobs.load(Ordering::SeqCst),
        2,
        "the access check and the alias registration"
    );
    let names = runner
        .execute(QueryRequest::new("{ User { name } }"))
        .await
        .data
        .unwrap();
    assert_eq!(names["User"].as_array().map(Vec::len), Some(2), "{names}");
}
