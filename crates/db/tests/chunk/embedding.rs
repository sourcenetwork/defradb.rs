use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use db::chunk::split::{split, ChunkConfig};
use db::write::autocommit::batch::BatchMutator;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use rapidhash::RapidHashSet;
use regolith::sync::Atom;
use serde_json::{json, Value};

use crate::common::fixture::make_test_db_with_bus;
use crate::common::http::HttpServer;

const CONFIG: ChunkConfig = ChunkConfig {
    max_chars: 64,
    overlap: 0,
};
const PARAGRAPHS: usize = 6;

/// An embedding provider that records every text it is asked to embed.
struct Provider {
    texts: Arc<Atom<Vec<String>>>,
    requests: Arc<AtomicUsize>,
    server: HttpServer,
}

impl Provider {
    fn start() -> Self {
        let texts = Arc::new(Atom::new(Vec::new()));
        let requests = Arc::new(AtomicUsize::new(0));
        let (recorded, counted) = (Arc::clone(&texts), Arc::clone(&requests));
        let server = HttpServer::start(&["/embeddings"], move |request| {
            counted.fetch_add(1, Ordering::SeqCst);
            let inputs: Vec<String> = match &request.body["input"] {
                Value::String(text) => vec![text.clone()],
                other => serde_json::from_value(other.clone()).unwrap(),
            };
            recorded.rcu(|all| {
                let mut all = all.clone();
                all.extend(inputs.iter().cloned());
                all
            });
            let data: Vec<Value> = inputs
                .iter()
                .enumerate()
                .map(|(index, text)| json!({"index": index, "embedding": vector_of(text)}))
                .collect();
            json!({ "data": data })
        });
        Self {
            texts,
            requests,
            server,
        }
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

    fn texts(&self) -> Vec<String> {
        self.texts.load_clone()
    }

    /// How many HTTP requests the provider answered.
    fn requests(&self) -> usize {
        self.requests.load(Ordering::SeqCst)
    }
}

/// What the provider answers for `text`: the embedding input is the chunk's
/// text and a newline.
fn vector_of(text: &str) -> Vec<f64> {
    vec![text.len() as f64, 1.0]
}

fn book(edited: Option<usize>) -> String {
    (0..PARAGRAPHS)
        .map(|i| match edited == Some(i) {
            true => format!("Paragraph {i} was edited and says something new."),
            false => format!("Paragraph {i} tells a small story of its own."),
        })
        .collect::<Vec<_>>()
        .join("\n\n")
}

fn article(sdl: &str, text: &str) -> Document {
    let collection = query::parse_sdl(sdl)
        .unwrap()
        .into_iter()
        .find(|c| c.name == "Article")
        .unwrap();
    let mut doc = Document::new(collection);
    doc.set("body", NormalValue::String(text.into())).unwrap();
    doc
}

/// Runs `write` in a fresh explicit transaction until it commits, answering
/// each input it asks for outside the transaction, as the query runner does.
/// Returns the write's result and the size of each wave it needed.
async fn in_waves<W, Fut, T>(db: &Arc<db::DB<storage::RegolithStore>>, write: W) -> (T, Vec<usize>)
where
    W: Fn(Arc<dyn DocMutator>) -> Fut,
    Fut: std::future::Future<Output = query::error::Result<T>>,
{
    let memo = query::prepared::PreparedInputMemo::default();
    let mut waves = Vec::new();
    loop {
        let txn = db.new_txn(false).await.unwrap();
        let owner = db::DbDocMutator::new(Arc::clone(db), txn);
        let mutator = owner.with_prepared_inputs(memo.clone()).unwrap();
        match write(Arc::clone(&mutator)).await {
            Ok(value) => {
                drop(mutator);
                owner.take_txn().await.unwrap().commit().await.unwrap();
                return (value, waves);
            }
            Err(query::error::QueryError::NeedsInput(request)) => {
                drop(mutator);
                owner.take_txn().await.unwrap().discard().unwrap();
                memo.answer_wave(&request, |wave| {
                    waves.push(wave.len());
                    owner.resolve_prepared_inputs(wave)
                })
                .await
                .unwrap();
            }
            Err(error) => panic!("write failed: {error}"),
        }
    }
}

async fn setup(provider: &Provider) -> Arc<db::DB<storage::RegolithStore>> {
    let (db, _bus) = make_test_db_with_bus().await;
    db.create_collections_atomic(query::parse_sdl(&provider.sdl()).unwrap())
        .await
        .expect("schema");
    db
}

/// Every stored chunk's text and vector, read through GraphQL.
async fn stored_chunks(db: &Arc<db::DB<storage::RegolithStore>>) -> Vec<Value> {
    let runner = query::QueryRunner::with_arc_registry_and_provider(
        db::LensedAutoCommitFetcher::new(Arc::clone(db)),
        db::DbCollectionProvider::new_arc(Arc::clone(db)),
        Arc::new(db::DbTransactionRegistry::new(Arc::clone(db))),
    );
    let response = query::QueryExecutor::execute(
        &runner,
        query::QueryRequest::new("query { Article__body_chunk { text body_v } }"),
    )
    .await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.unwrap()["Article__body_chunk"]
        .as_array()
        .unwrap()
        .clone()
}

#[test]
fn every_new_chunk_is_embedded_in_one_wave_outside_the_transaction() {
    workers::block_on(async {
        let provider = Provider::start();
        let db = setup(&provider).await;
        let sdl = provider.sdl();

        let (created, waves) = in_waves(&db, |mutator| {
            let doc = article(&sdl, &book(None));
            async move { mutator.create("Article", doc).await }
        })
        .await;

        let chunks = split(&book(None), &CONFIG);
        assert_eq!(waves, vec![chunks.len()]);
        assert_eq!(provider.requests(), 1);
        drop(created);
        let stored = stored_chunks(&db).await;
        assert_eq!(stored.len(), chunks.len());
        for chunk in stored {
            let text = chunk["text"].as_str().unwrap();
            let vector: Vec<f64> = serde_json::from_value(chunk["body_v"].clone()).unwrap();
            assert_eq!(vector, vector_of(&format!("{text}\n")));
        }
    })
    .unwrap()
}

#[test]
fn editing_one_paragraph_embeds_only_its_new_chunk() {
    workers::block_on(async {
        let provider = Provider::start();
        let db = setup(&provider).await;
        let sdl = provider.sdl();
        let (created, _) = in_waves(&db, |mutator| {
            let doc = article(&sdl, &book(None));
            async move { mutator.create("Article", doc).await }
        })
        .await;
        let asked_before = provider.texts().len();

        let (updated, waves) = in_waves(&db, |mutator| {
            let mut doc = article(&sdl, &book(Some(2)));
            doc.set_id(created.doc_id.clone());
            let fields = RapidHashSet::from_iter(["body".to_string()]);
            async move { mutator.update("Article", doc, fields).await }
        })
        .await;

        assert_eq!(waves, vec![1]);
        drop(updated);
        let edited = split(&book(Some(2)), &CONFIG)
            .into_iter()
            .find(|chunk| chunk.text.contains("edited"))
            .unwrap();
        let asked: Vec<String> = provider.texts().split_off(asked_before);
        assert_eq!(asked, vec![format!("{}\n", edited.text)]);
    })
    .unwrap()
}

#[test]
fn a_chunk_write_without_prepared_inputs_is_refused_before_the_model_is_called() {
    workers::block_on(async {
        let provider = Provider::start();
        let db = setup(&provider).await;
        let txn = db.new_txn(false).await.unwrap();
        let batch = BatchMutator::new(Arc::clone(&db), Arc::new(txn)).await;

        let error = batch
            .create("Article", article(&provider.sdl(), &book(None)))
            .await
            .unwrap_err()
            .to_string();

        assert!(
            error.contains(
                "Article__body_chunk needs its embeddings prepared before the transaction"
            ),
            "{error}"
        );
        assert!(provider.texts().is_empty());
    })
    .unwrap()
}
