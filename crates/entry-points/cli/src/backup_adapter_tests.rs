//! The backup adapter sends every write of an import through the host it is
//! given, never through a runner of its own.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use db::backup::{DbJob, ImportHost, RunnerHost};
use db::DB;
use defra_core::thread_bounds::MaybeBoxFuture;
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryResponse, QueryRunner};
use storage::RegolithStore;

use crate::backup_adapter::BackupAdapter;

const USERS_SDL: &str = "type User { name: String }";

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
    let users = database
        .add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
        .await
        .unwrap();
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(
        database.clone(),
    ));
    let runner: Arc<dyn QueryExecutor> = Arc::new(
        QueryRunner::new(db::AutoCommitFetcher::new(database.clone()), users)
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

const FIRST: &str = "bae-11111111-1111-5111-8111-111111111111";
const SECOND: &str = "bae-22222222-2222-5222-8222-222222222222";

/// A host on `inner` that, right after the import's first save, lets another
/// writer import a document under the file ID [`SECOND`].
struct ClaimingAfterSave {
    inner: RunnerHost,
    database: Arc<DB<RegolithStore>>,
    claimed: AtomicUsize,
}

impl ImportHost for ClaimingAfterSave {
    fn run(&self, request: QueryRequest) -> MaybeBoxFuture<'_, QueryResponse> {
        Box::pin(async move {
            let response = self.inner.run(request).await;
            if self.claimed.fetch_add(1, Ordering::SeqCst) == 0 {
                let other = format!(r#"{{"User": [{{"_docID": "{SECOND}", "name": "Other"}}]}}"#);
                db::backup::import_database_as(
                    &defra_core::RequestContext::ambient(),
                    &self.database,
                    &self.inner,
                    &other,
                )
                .await
                .unwrap();
            }
            response
        })
    }

    fn on_db(&self, job: DbJob) -> MaybeBoxFuture<'_, Result<(), String>> {
        self.inner.on_db(job)
    }
}

#[nuclei::test(threads = 1)]
async fn an_id_taken_after_its_check_leaves_every_other_saved_document_its_id() {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let database = Arc::new(DB::open_from_arc(store).await.unwrap());
    let users = database
        .add_collection(&defra_core::RequestContext::ambient(), USERS_SDL)
        .await
        .unwrap();
    let mutator = Arc::new(db::write::autocommit::AutoCommitMutator::new(
        database.clone(),
    ));
    let runner: Arc<dyn QueryExecutor> = Arc::new(
        QueryRunner::new(db::AutoCommitFetcher::new(database.clone()), users)
            .with_mutator(mutator as Arc<dyn DocMutator>),
    );
    let host = ClaimingAfterSave {
        inner: RunnerHost::new(Arc::clone(&runner)),
        database: database.clone(),
        claimed: AtomicUsize::new(0),
    };
    let file = format!(
        r#"{{"User": [{{"_docID": "{FIRST}", "name": "John"}}, {{"_docID": "{SECOND}", "name": "Bob"}}]}}"#
    );

    let raced = db::backup::import_database_as(
        &defra_core::RequestContext::ambient(),
        &database,
        &host,
        &file,
    )
    .await;
    assert_eq!(
        raced.map(|stats| stats.documents_imported),
        Err("a document with the given ID already exists".to_string())
    );

    let again = format!(r#"{{"User": [{{"_docID": "{FIRST}", "name": "Again"}}]}}"#);
    let refused = db::backup::import_database_as(
        &defra_core::RequestContext::ambient(),
        &database,
        &RunnerHost::new(Arc::clone(&runner)),
        &again,
    )
    .await;
    assert_eq!(
        refused.map(|stats| stats.documents_imported),
        Err("a document with the given ID already exists".to_string()),
        "John, saved in the batch whose registration lost SECOND, still answers to FIRST"
    );
}
