use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use db::chunk::split::{split, ChunkConfig};
use db::write::autocommit::batch::BatchMutator;
use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::{json, Value};

use crate::common::fixture::make_test_db_with_bus;
use crate::common::http::HttpServer;

const CONFIG: ChunkConfig = ChunkConfig {
    max_chars: 64,
    overlap: 0,
};
const PARAGRAPHS: usize = 4;
const ARTICLES: usize = 3;

/// An embedding provider that counts the requests it answers.
struct Provider {
    requests: Arc<AtomicUsize>,
    server: HttpServer,
}

impl Provider {
    fn start() -> Self {
        let requests = Arc::new(AtomicUsize::new(0));
        let counted = Arc::clone(&requests);
        let server = HttpServer::start(&["/embeddings"], move |request| {
            counted.fetch_add(1, Ordering::SeqCst);
            let inputs: Vec<String> = match &request.body["input"] {
                Value::String(text) => vec![text.clone()],
                other => serde_json::from_value(other.clone()).unwrap(),
            };
            let data: Vec<Value> = inputs
                .iter()
                .enumerate()
                .map(|(index, text)| json!({"index": index, "embedding": vector_of(text)}))
                .collect();
            json!({ "data": data })
        });
        Self { requests, server }
    }

    fn sdl(&self) -> String {
        format!(
            r#"type Article {{
                body: String @chunk(maxChars: 64)
                body_v: [Float32!] @embedding(fields: ["body"], provider: "openai", model: "test-model", url: "{}") @index(vector: {{dimensions: 2}})
            }}"#,
            self.server.url()
        )
    }

    /// How many HTTP requests the provider answered.
    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// What the provider answers for `text`.
fn vector_of(text: &str) -> Vec<f64> {
    vec![text.len() as f64, 1.0]
}

/// Article `n`'s text, with paragraph `edited` changed when given.
fn book(n: usize, edited: Option<usize>) -> String {
    (0..PARAGRAPHS)
        .map(|i| match edited == Some(i) {
            true => format!("Paragraph {i} of article {n} was edited to say more."),
            false => format!("Paragraph {i} of article {n} tells its own story."),
        })
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

fn chunk_count(text: &str) -> u64 {
    split(&text.replace("\\n", "\n"), &CONFIG).len() as u64
}

type Runner = QueryRunner<
    LensedAutoCommitFetcher<storage::RegolithStore>,
    DbTransactionRegistry<storage::RegolithStore>,
>;

async fn setup(provider: &Provider) -> (Arc<db::DB<storage::RegolithStore>>, Runner) {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(&provider.sdl()).unwrap())
        .await
        .expect("schema");
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(&db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))));
    (db, runner)
}

async fn run(runner: &Runner, query: &str) -> Value {
    let response = runner.execute(QueryRequest::new(query)).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")
}

fn add_articles(count: usize) -> String {
    let inputs: Vec<String> = (0..count)
        .map(|n| format!(r#"{{body: "{}"}}"#, book(n, None)))
        .collect();
    format!(
        "mutation {{ add_Article(input: [{}]) {{ _docID }} }}",
        inputs.join(", ")
    )
}

/// Waves and the inputs they fetched since the process started. nextest runs
/// each test in its own process, so these are this test's.
fn waves_and_inputs() -> (u64, u64) {
    let stats = query::prepared::input_wait_stats();
    (
        stats.waves.load(Ordering::Relaxed),
        stats.inputs.load(Ordering::Relaxed),
    )
}

/// Every stored chunk holds the provider's vector of its text.
async fn assert_chunk_vectors(runner: &Runner, expected: u64) {
    let data = run(runner, "query { Article__body_chunk { text body_v } }").await;
    let chunks = data["Article__body_chunk"].as_array().unwrap();
    assert_eq!(chunks.len() as u64, expected);
    for chunk in chunks {
        let text = chunk["text"].as_str().unwrap();
        let vector: Vec<f64> = serde_json::from_value(chunk["body_v"].clone()).unwrap();
        assert_eq!(vector, vector_of(&format!("{text}\n")));
    }
}

#[test]
fn a_create_of_several_articles_embeds_every_new_chunk_in_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider).await;
        let chunks = ARTICLES as u64 * chunk_count(&book(0, None));
        let (waves, inputs) = waves_and_inputs();

        run(&runner, &add_articles(ARTICLES)).await;

        assert_eq!(waves_and_inputs(), (waves + 1, inputs + chunks));
        assert_eq!(provider.requests(), 1);
        assert_chunk_vectors(&runner, chunks).await;
    })
    .unwrap()
}

#[test]
fn a_create_of_several_articles_in_a_transaction_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider).await;
        let chunks = ARTICLES as u64 * chunk_count(&book(0, None));
        let (waves, inputs) = waves_and_inputs();

        let txn = runner.begin_txn(false).await.unwrap();
        let response = runner
            .execute_in_txn(QueryRequest::new(add_articles(ARTICLES)), &txn)
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        runner.commit_txn(&txn).await.unwrap();

        assert_eq!(waves_and_inputs(), (waves + 1, inputs + chunks));
        assert_chunk_vectors(&runner, chunks).await;
    })
    .unwrap()
}

/// The node's embedding cache would answer the unchanged chunks without a
/// call, so only the memo shows that they are not asked for at all.
#[test]
fn an_edit_does_not_ask_for_its_unchanged_chunks() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider).await;
        let data = run(&runner, &add_articles(1)).await;
        let doc_id = data["add_Article"][0]["_docID"].as_str().unwrap().to_owned();
        let (waves, inputs) = waves_and_inputs();

        run(
            &runner,
            &format!(
                r#"mutation {{ update_Article(docID: "{doc_id}", input: {{body: "{}"}}) {{ _docID }} }}"#,
                book(0, Some(2))
            ),
        )
        .await;

        assert_eq!(waves_and_inputs(), (waves + 1, inputs + 1));
    })
    .unwrap()
}

#[test]
fn a_chunk_write_without_prepared_inputs_is_refused() {
    workers::block_on(async {
        let provider = Provider::start();
        let (db, _runner) = setup(&provider).await;
        let collection = query::parse_sdl(&provider.sdl())
            .unwrap()
            .into_iter()
            .find(|c| c.name == "Article")
            .unwrap();
        let mut doc = Document::new(collection);
        doc.set(
            "body",
            NormalValue::String(book(0, None).replace("\\n", "\n")),
        )
        .unwrap();
        let txn = db.new_txn(false).await.unwrap();
        let batch = BatchMutator::new(Arc::clone(&db), Arc::new(txn)).await;

        let error = batch.create("Article", doc).await.unwrap_err().to_string();

        assert!(
            error.contains(
                "Article__body_chunk needs its embeddings prepared before the transaction"
            ),
            "{error}"
        );
    })
    .unwrap()
}
