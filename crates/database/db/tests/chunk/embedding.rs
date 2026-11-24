//! How many embedding waves chunk writes need, counted through the real
//! query runner. testo checks the vectors themselves.

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
                .map(|(index, text)| json!({"index": index, "embedding": [text.len(), 1]}))
                .collect();
            json!({ "data": data })
        });
        Self { requests, server }
    }

    /// `@embedding` arguments for this provider.
    fn embedding(&self, field: &str) -> String {
        format!(
            r#"@embedding(fields: ["{field}"], provider: "openai", model: "test-model", url: "{}")"#,
            self.server.url()
        )
    }

    /// `Article` with an embedded chunked `body`, and `extra` fields.
    fn sdl(&self, extra: &str) -> String {
        format!(
            "type Article {{ body: String @chunk(maxChars: 64) body_v: [Float32!] {} {extra} }}",
            self.embedding("body")
        )
    }

    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// Article `n`'s text, with paragraph `edited` changed when given, escaped
/// for a GraphQL string.
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

async fn setup(sdl: &str) -> (Arc<db::DB<storage::RegolithStore>>, Runner) {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(sdl).unwrap())
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

async fn run_in_txn(runner: &Runner, query: &str) -> Value {
    let txn = runner.begin_txn(false).await.unwrap();
    let response = runner.execute_in_txn(QueryRequest::new(query), &txn).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    runner.commit_txn(&txn).await.unwrap();
    response.data.expect("data")
}

fn add_articles(articles: &[String]) -> String {
    format!(
        "mutation {{ add_Article(input: [{}]) {{ _docID }} }}",
        articles.join(", ")
    )
}

fn books(count: usize) -> Vec<String> {
    (0..count)
        .map(|n| format!(r#"{{body: "{}"}}"#, book(n, None)))
        .collect()
}

fn doc_ids(data: &Value, mutation: &str) -> Vec<String> {
    data[mutation]
        .as_array()
        .unwrap()
        .iter()
        .map(|doc| doc["_docID"].as_str().unwrap().to_owned())
        .collect()
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

#[test]
fn a_create_of_several_articles_embeds_every_new_chunk_in_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("")).await;
        let chunks = ARTICLES as u64 * chunk_count(&book(0, None));
        let (waves, inputs) = waves_and_inputs();

        run(&runner, &add_articles(&books(ARTICLES))).await;

        assert_eq!(waves_and_inputs(), (waves + 1, inputs + chunks));
        assert_eq!(provider.requests(), 1);
    })
    .unwrap()
}

#[test]
fn a_create_of_several_articles_in_a_transaction_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("")).await;
        let chunks = ARTICLES as u64 * chunk_count(&book(0, None));
        let (waves, inputs) = waves_and_inputs();

        run_in_txn(&runner, &add_articles(&books(ARTICLES))).await;

        assert_eq!(waves_and_inputs(), (waves + 1, inputs + chunks));
    })
    .unwrap()
}

#[test]
fn a_bad_article_after_one_that_waits_for_its_vectors_fails_with_its_own_error() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("title: String @index(unique: true)")).await;
        let twins: Vec<String> = (0..2)
            .map(|n| format!(r#"{{title: "same", body: "{}"}}"#, book(n, None)))
            .collect();

        let response = runner
            .execute(QueryRequest::new(add_articles(&twins)))
            .await;

        let errors = format!("{:?}", response.errors);
        assert!(errors.contains("unique"), "{errors}");
        let stored = run(
            &runner,
            "query { Article { _docID } Article__body_chunk { _docID } }",
        )
        .await;
        assert_eq!(stored, json!({ "Article": [], "Article__body_chunk": [] }));
    })
    .unwrap()
}

#[test]
fn a_parent_with_two_chunked_fields_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let summary = format!(
            "summary: String @chunk(maxChars: 64) summary_v: [Float32!] {}",
            provider.embedding("summary")
        );
        let (_db, runner) = setup(&provider.sdl(&summary)).await;
        let (waves, inputs) = waves_and_inputs();

        run(
            &runner,
            &add_articles(&[format!(
                r#"{{body: "{}", summary: "{}"}}"#,
                book(0, None),
                book(1, None)
            )]),
        )
        .await;

        let chunks = chunk_count(&book(0, None)) + chunk_count(&book(1, None));
        assert_eq!(waves_and_inputs(), (waves + 1, inputs + chunks));
    })
    .unwrap()
}

/// Updates several articles to one new text. Patching `tags` too makes the
/// update read whole documents instead of only the parts it writes.
async fn update_several(runner: &Runner, in_txn: bool, also_tags: bool) {
    let created = run(runner, &add_articles(&books(ARTICLES))).await;
    let ids = doc_ids(&created, "add_Article");
    let (waves, inputs) = waves_and_inputs();
    let tags = if also_tags { r#", tags: ["new"]"# } else { "" };
    let update = format!(
        r#"mutation {{ update_Article(docID: {ids:?}, input: {{body: "{}"{tags}}}) {{ _docID }} }}"#,
        book(9, None)
    );

    match in_txn {
        true => run_in_txn(runner, &update).await,
        false => run(runner, &update).await,
    };

    // Every article gets the same text, so the wave asks for its chunks once.
    assert_eq!(
        waves_and_inputs(),
        (waves + 1, inputs + chunk_count(&book(9, None)))
    );
}

#[test]
fn an_update_of_several_articles_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("tags: [String]")).await;
        update_several(&runner, false, false).await;
    })
    .unwrap()
}

#[test]
fn an_update_of_several_whole_articles_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("tags: [String]")).await;
        update_several(&runner, false, true).await;
    })
    .unwrap()
}

#[test]
fn an_update_in_a_transaction_needs_one_wave() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("tags: [String]")).await;
        update_several(&runner, true, false).await;
    })
    .unwrap()
}

/// The node's embedding cache would answer the unchanged chunks without a
/// call, so only the memo shows that they are not asked for at all.
#[test]
fn an_edit_does_not_ask_for_its_unchanged_chunks() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("")).await;
        let created = run(&runner, &add_articles(&books(1))).await;
        let doc_id = &doc_ids(&created, "add_Article")[0];
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
fn a_revert_reuses_the_old_chunks_and_asks_for_nothing() {
    workers::block_on(async {
        let provider = Provider::start();
        let (_db, runner) = setup(&provider.sdl("")).await;
        let created = run(&runner, &add_articles(&books(1))).await;
        let doc_id = &doc_ids(&created, "add_Article")[0];
        let set_body = |body: String| {
            format!(
                r#"mutation {{ update_Article(docID: "{doc_id}", input: {{body: "{body}"}}) {{ _docID }} }}"#
            )
        };
        run(&runner, &set_body(book(0, Some(2)))).await;
        let (waves, inputs) = waves_and_inputs();

        run(&runner, &set_body(book(0, None))).await;

        assert_eq!(waves_and_inputs(), (waves, inputs));
    })
    .unwrap()
}

/// A statement run straight on the transaction's mutator, as a run with no
/// write preparation does: the first article waits for its title's vector
/// before it draws a short ID, the second brings its own and draws one in
/// the first run.
#[test]
fn a_create_keeps_its_short_id_when_one_before_it_waits_for_an_input() {
    workers::block_on(async {
        let provider = Provider::start();
        let title = format!(
            "title: String title_v: [Float32!] {}",
            provider.embedding("title")
        );
        let sdl = provider.sdl(&title);
        let (db, _runner) = setup(&sdl).await;
        let article = |title: Option<&str>, n: usize| {
            let collection = query::parse_sdl(&sdl)
                .unwrap()
                .into_iter()
                .find(|c| c.name == "Article")
                .unwrap();
            let mut doc = Document::new(collection);
            match title {
                Some(title) => doc.set("title", NormalValue::String(title.into())).unwrap(),
                // A vector given by the user is never asked of the model.
                None => doc
                    .set("title_v", NormalValue::Float64Array(vec![0.0, 1.0]))
                    .unwrap(),
            }
            doc.set(
                "body",
                NormalValue::String(book(n, None).replace("\\n", "\n")),
            )
            .unwrap();
            doc
        };
        let memo = query::prepared::PreparedInputMemo::default();
        let ids = loop {
            let owner = db::DbDocMutator::new(Arc::clone(&db), db.new_txn(false).await.unwrap());
            let statement = owner.with_prepared_inputs(memo.clone()).unwrap();
            let docs = vec![article(Some("t"), 0), article(None, 1)];
            match statement.create_many("Article", docs).await {
                Ok(created) => {
                    drop(statement);
                    owner.take_txn().await.unwrap().commit().await.unwrap();
                    break created
                        .into_iter()
                        .map(|c| c.doc_id.to_string())
                        .collect::<Vec<_>>();
                }
                Err(query::error::QueryError::NeedsInput(request)) => {
                    drop(statement);
                    owner.take_txn().await.unwrap().discard().unwrap();
                    memo.answer_wave(&request, |wave| owner.resolve_prepared_inputs(wave))
                        .await
                        .unwrap();
                }
                Err(error) => panic!("create failed: {error}"),
            }
        };

        let txn = db.new_txn(true).await.unwrap();
        let systemstore = txn.systemstore().unwrap();
        let mut short_ids = Vec::new();
        for id in &ids {
            let doc_ref = db::docid::map::get_doc_ref(&systemstore, id)
                .await
                .unwrap()
                .unwrap();
            short_ids.push(doc_ref.doc_short_id);
        }
        drop(systemstore);
        txn.force_discard().unwrap();
        assert!(
            short_ids[1] < short_ids[0],
            "the second article drew first and keeps that ID: {short_ids:?}"
        );
    })
    .unwrap()
}

#[test]
fn a_chunk_write_without_prepared_inputs_is_refused() {
    workers::block_on(async {
        let provider = Provider::start();
        let (db, _runner) = setup(&provider.sdl("")).await;
        let collection = query::parse_sdl(&provider.sdl(""))
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
