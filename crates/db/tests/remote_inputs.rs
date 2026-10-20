//! Round trips of the remote inputs a write or a merge fetches before its
//! transaction, pinned with counting fakes: a composite with F encrypted
//! fields makes one KMS request; a create of N documents makes two signing
//! waves; time scales with waves, not items.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use blockstore::Blockstore as _;
use blockstore::DefraBlockstore;
use crypto::PrivateKey as _;
use db::database::DB;
use db::merge::merge_handler::DbMergeHandler;
use db::AutoCommitMutator;
use defra_core::block::{
    Block, CompositeDeltaPayload, CrdtDelta, DAGLink, Encryption, LwwDeltaPayload,
};
use defra_core::merge::{BlockMetadata, MergeHandler, MergeOutcome};
use defra_core::signing::{RemoteSigner, SignFailure, SigningAuthorization};
use defra_core::thread_bounds::MaybeBoxFuture;
use document::{Document, NormalValue};
use query::mutator::DocMutator;
use schema::{CollectionVersion, FieldDescription, FieldKind};
use storage::RegolithStore;

#[path = "common/http.rs"]
mod http;
use http::HttpServer;

const FIELDS: [&str; 4] = ["a", "b", "c", "d"];

async fn database() -> Arc<DB<RegolithStore>> {
    database_on(Arc::new(RegolithStore::in_memory().unwrap())).await
}

async fn database_on(store: Arc<RegolithStore>) -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::from_arc(store).unwrap());
    let mut fields = vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())];
    for (index, name) in FIELDS.iter().enumerate() {
        fields.push(FieldDescription::new(
            (index + 2).to_string(),
            *name,
            FieldKind::string(),
        ));
    }
    db.create_collection(CollectionVersion::new("Things", "v1", "col-things", fields))
        .await
        .unwrap();
    db
}

/// A merge handler over one store, as a node has.
async fn handler() -> (
    DbMergeHandler<RegolithStore, DefraBlockstore<RegolithStore>>,
    Arc<DefraBlockstore<RegolithStore>>,
) {
    let store = Arc::new(RegolithStore::in_memory().unwrap());
    let db = database_on(Arc::clone(&store)).await;
    let blockstore = Arc::new(DefraBlockstore::new(store, false));
    (DbMergeHandler::new(db, Arc::clone(&blockstore)), blockstore)
}

/// A KMS that knows a fixed set of keys and counts the requests it serves.
struct CountingKms {
    keys: Vec<(cid::Cid, [u8; 32])>,
    requests: AtomicUsize,
}

#[async_trait]
impl kms::KmsService for CountingKms {
    async fn get_keys(
        &self,
        _: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<kms::KeyResults> {
        self.requests.fetch_add(1, Ordering::SeqCst);
        let (results, sender) = kms::KeyResults::new(cids.len().max(1));
        for cid in cids {
            let key = self
                .keys
                .iter()
                .find(|(known, _)| known == cid)
                .map(|(_, key)| *key)
                .ok_or(kms::Error::KeyUnavailable);
            sender.send(key.map(|key| (*cid, key))).await.unwrap();
        }
        Ok(results)
    }

    async fn generate_key(
        &self,
        _: &kms::RequestContext,
        _: kms::KeyScope,
    ) -> kms::Result<(kms::EncryptionCid, [u8; 32])> {
        Err(kms::Error::Unsupported("merges never generate keys"))
    }

    async fn serve_request(
        &self,
        _: kms::PeerIdentity,
        _: kms::FetchEncryptionKeyRequest,
    ) -> kms::Result<kms::FetchEncryptionKeyReply> {
        Err(kms::Error::Unsupported("no peers"))
    }
}

/// A composite linking one encrypted field block per field, each under its
/// own key, stored in `blockstore`. Returns the composite and the keys.
async fn encrypted_composite(
    blockstore: &DefraBlockstore<RegolithStore>,
) -> (cid::Cid, Vec<u8>, Vec<(cid::Cid, [u8; 32])>) {
    let mut links = Vec::new();
    let mut keys = Vec::new();
    for (index, field) in FIELDS.iter().enumerate() {
        let key = [u8::try_from(index + 1).unwrap(); 32];
        let encryption = Encryption::new(key.to_vec());
        let encryption_cid = encryption.generate_cid().unwrap();
        let mut plaintext = Vec::new();
        ciborium::into_writer(
            &NormalValue::String(format!("value {field}")),
            &mut plaintext,
        )
        .unwrap();
        let (ciphertext, _) =
            crypto::encryption::aes::encrypt_aes(&plaintext, &key, &[], true).unwrap();
        let block = Block::new_with_options(
            CrdtDelta::Lww(LwwDeltaPayload {
                field_name: field.to_string(),
                schema_version_id: "v1".into(),
                priority: 1,
                data: ciphertext,
            }),
            vec![],
            vec![],
            Some(encryption_cid),
            None,
        );
        let block_cid = block.generate_cid().unwrap();
        blockstore
            .put(&block_cid, &block.to_dag_cbor().unwrap())
            .await
            .unwrap();
        links.push(DAGLink::new(*field, block_cid));
        keys.push((encryption_cid, key));
    }
    let composite = Block::new(
        CrdtDelta::Composite(CompositeDeltaPayload {
            schema_version_id: "v1".into(),
            priority: 1,
            status: 1,
        }),
        vec![],
        links,
    );
    let cid = composite.generate_cid().unwrap();
    let data = composite.to_dag_cbor().unwrap();
    blockstore.put(&cid, &data).await.unwrap();
    (cid, data, keys)
}

#[test]
fn a_merged_composite_with_four_encrypted_fields_makes_one_kms_request() {
    workers::block_on(async {
        let (handler, blockstore) = handler().await;
        let (cid, data, keys) = encrypted_composite(&blockstore).await;
        let kms = Arc::new(CountingKms {
            keys,
            requests: AtomicUsize::new(0),
        });
        handler.set_kms(kms.clone());
        let doc_id = db::block::builder::derive_doc_id(&cid);
        let metadata =
            BlockMetadata::normal(&doc_id, "col-things", "did:key:z6MkMergeKeys", None, false);

        let outcome = handler.handle_block(&cid, &data, metadata).await.unwrap();

        assert_eq!(outcome, MergeOutcome::Merged);
        assert_eq!(
            kms.requests.load(Ordering::SeqCst),
            1,
            "every field's key in one request, before the merge transaction"
        );
    })
    .unwrap()
}

#[test]
fn a_composite_merged_in_its_own_transaction_makes_one_kms_request() {
    workers::block_on(async {
        let (handler, blockstore) = handler().await;
        let (cid, data, keys) = encrypted_composite(&blockstore).await;
        let kms = Arc::new(CountingKms {
            keys,
            requests: AtomicUsize::new(0),
        });
        handler.set_kms(kms.clone());
        let doc_id = db::block::builder::derive_doc_id(&cid);
        let metadata =
            BlockMetadata::normal(&doc_id, "col-things", "did:key:z6MkMergeKeys", None, false);
        let block = Block::from_dag_cbor(&data).unwrap();
        let CrdtDelta::Composite(payload) = &block.delta else {
            unreachable!("built as a composite")
        };

        let outcome = handler
            .process_composite_delta(&cid, &block, payload, &metadata, false, 0)
            .await
            .unwrap();

        assert_eq!(outcome, MergeOutcome::Merged);
        assert_eq!(kms.requests.load(Ordering::SeqCst), 1);
    })
    .unwrap()
}

/// A remote signer that counts its waves and the signatures in them, and
/// takes `latency` per wave.
struct WaveSigner {
    key: crypto::Ed25519PrivateKey,
    waves: AtomicUsize,
    signatures: AtomicUsize,
    latency: Duration,
}

impl RemoteSigner for WaveSigner {
    fn sign_sync(&self, data: &[u8], _: Option<&SigningAuthorization>) -> Result<Vec<u8>, String> {
        self.signatures.fetch_add(1, Ordering::SeqCst);
        self.key.sign(data).map_err(|error| error.to_string())
    }

    fn sign_batch(
        &self,
        payloads: Vec<Arc<[u8]>>,
        authorization: Option<SigningAuthorization>,
    ) -> MaybeBoxFuture<'static, Vec<Result<Vec<u8>, SignFailure>>> {
        self.waves.fetch_add(1, Ordering::SeqCst);
        let signatures: Vec<_> = payloads
            .iter()
            .map(|payload| {
                self.sign_sync(payload, authorization.as_ref())
                    .map_err(SignFailure::refused)
            })
            .collect();
        let latency = self.latency;
        Box::pin(async move {
            workers::delay(latency).unwrap().await.unwrap();
            signatures
        })
    }
}

fn signer(latency: Duration) -> Arc<WaveSigner> {
    Arc::new(WaveSigner {
        key: crypto::generate_ed25519().unwrap(),
        waves: AtomicUsize::new(0),
        signatures: AtomicUsize::new(0),
        latency,
    })
}

fn sign_with(signer: &Arc<WaveSigner>) {
    let public = signer.key.public_key();
    defra_core::signing::set_signing_config(Some(defra_core::signing::SigningConfig {
        key_type: defra_core::signing::SigningKeyType::Ed25519,
        private_key_bytes: Vec::new(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: hex::encode(public.raw()),
        remote_signer: Some(signer.clone() as Arc<dyn RemoteSigner>),
        signing_authorization: None,
    }));
}

fn documents(count: usize) -> Vec<Document> {
    (0..count)
        .map(|index| {
            let mut doc = Document::new();
            for field in FIELDS {
                doc.set(field, format!("{field} {index}"));
            }
            doc
        })
        .collect()
}

#[test]
fn a_create_of_n_documents_signs_in_two_waves_and_takes_two_round_trips() {
    workers::block_on(async {
        const DOCUMENTS: usize = 8;
        let latency = Duration::from_millis(200);
        let db = database().await;
        let signer = signer(latency);
        sign_with(&signer);

        let started = std::time::Instant::now();
        let created = AutoCommitMutator::new(Arc::clone(&db))
            .create_many("Things", documents(DOCUMENTS))
            .await;
        let elapsed = started.elapsed();
        defra_core::signing::set_signing_config(None);

        assert_eq!(created.unwrap().len(), DOCUMENTS);
        assert_eq!(
            signer.waves.load(Ordering::SeqCst),
            2,
            "every field block in the first wave, every composite in the second"
        );
        assert_eq!(
            signer.signatures.load(Ordering::SeqCst),
            DOCUMENTS * (FIELDS.len() + 1),
            "each block signed once"
        );
        assert!(
            elapsed >= latency * 2,
            "two waves of {latency:?} take at least twice that: {elapsed:?}"
        );
    })
    .unwrap()
}

/// An OpenAI-compatible embedding endpoint that counts its requests and the
/// texts in them, and answers in reverse order with each entry's index, so a
/// caller that placed vectors by position would mix them up. A text's vector
/// is `[its length, its first byte]`.
fn embedding_server() -> (HttpServer, Arc<AtomicUsize>, Arc<AtomicUsize>) {
    use serde_json::{json, Value};

    let requests = Arc::new(AtomicUsize::new(0));
    let texts = Arc::new(AtomicUsize::new(0));
    let (counted_requests, counted_texts) = (Arc::clone(&requests), Arc::clone(&texts));
    let server = HttpServer::start(&["/embeddings"], move |request| {
        counted_requests.fetch_add(1, Ordering::SeqCst);
        let inputs: Vec<String> = match &request.body["input"] {
            Value::Array(inputs) => inputs
                .iter()
                .map(|input| input.as_str().unwrap().to_string())
                .collect(),
            input => vec![input.as_str().unwrap().to_string()],
        };
        counted_texts.fetch_add(inputs.len(), Ordering::SeqCst);
        let data: Vec<Value> = inputs
            .iter()
            .enumerate()
            .rev()
            .map(|(index, text)| {
                json!({
                    "index": index,
                    "embedding": [text.len() as f64, f64::from(text.as_bytes()[0])],
                })
            })
            .collect();
        json!({ "data": data })
    });
    (server, requests, texts)
}

async fn embedded_database(url: &str) -> Arc<DB<RegolithStore>> {
    let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
    let mut collection = CollectionVersion::new(
        "Texts",
        "texts-v1",
        "col-texts",
        vec![
            FieldDescription::new("1", "_docID", FieldKind::doc_id()),
            FieldDescription::new("2", "text", FieldKind::string()),
            FieldDescription::new("3", "vector", FieldKind::float64_array()),
            FieldDescription::new("4", "tag", FieldKind::string()),
        ],
    );
    collection
        .vector_embeddings
        .push(schema::VectorEmbeddingDescription {
            field_name: "vector".to_string(),
            fields: vec!["text".to_string()],
            model: "test-model".to_string(),
            provider: "openai".to_string(),
            template: String::new(),
            url: url.to_string(),
        });
    db.create_collection(collection).await.unwrap();
    db
}

fn text_document(text: &str) -> Document {
    tagged_document(text, "")
}

/// A document whose tag, which nothing embeds, tells it apart from another
/// with the same text.
fn tagged_document(text: &str, tag: &str) -> Document {
    let mut doc = Document::new();
    doc.set("text", text);
    doc.set("tag", tag);
    doc
}

#[test]
fn a_create_of_n_documents_embeds_them_in_one_request() {
    workers::block_on(async {
        let (server, requests, texts) = embedding_server();
        let db = embedded_database(&server.url()).await;
        let words = ["alpha", "bravo", "charlie", "delta", "echo", "foxtrot"];

        let created = AutoCommitMutator::new(Arc::clone(&db))
            .create_many(
                "Texts",
                words.iter().map(|word| text_document(word)).collect(),
            )
            .await
            .unwrap();

        assert_eq!(
            requests.load(Ordering::SeqCst),
            1,
            "one request for the wave"
        );
        assert_eq!(texts.load(Ordering::SeqCst), words.len());
        for (word, created) in words.iter().zip(&created) {
            // The text's own vector, the source text plus a newline.
            let text = format!("{word}\n");
            assert_eq!(
                created.document.get("vector"),
                Some(&NormalValue::Float64Array(vec![
                    text.len() as f64,
                    f64::from(text.as_bytes()[0])
                ])),
                "{word}"
            );
        }
    })
    .unwrap()
}

#[test]
fn identical_texts_are_embedded_once() {
    workers::block_on(async {
        let (server, requests, texts) = embedding_server();
        let db = embedded_database(&server.url()).await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));

        let first = mutator
            .create_many(
                "Texts",
                vec![tagged_document("same", "1"), tagged_document("same", "2")],
            )
            .await
            .unwrap();
        let later = mutator
            .create("Texts", tagged_document("same", "3"))
            .await
            .unwrap();

        assert_eq!(texts.load(Ordering::SeqCst), 1, "one text, embedded once");
        assert_eq!(
            requests.load(Ordering::SeqCst),
            1,
            "the later create hits the cache"
        );
        let expected = NormalValue::Float64Array(vec![5.0, f64::from(b's')]);
        for created in first.iter().chain(std::iter::once(&later)) {
            assert_eq!(created.document.get("vector"), Some(&expected));
        }
    })
    .unwrap()
}

#[test]
fn an_update_and_a_delete_each_sign_in_one_wave() {
    workers::block_on(async {
        let db = database().await;
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator
            .create("Things", documents(1).remove(0))
            .await
            .unwrap();
        let signer = signer(Duration::from_millis(50));
        sign_with(&signer);

        let mut changed = created.document.clone();
        changed.set("a", "changed");
        let updated = mutator
            .update("Things", changed, ["a".to_string()].into_iter().collect())
            .await;
        let waves_after_update = signer.waves.load(Ordering::SeqCst);
        let signatures_after_update = signer.signatures.load(Ordering::SeqCst);
        let deleted = mutator.delete("Things", &created.doc_id).await;
        defra_core::signing::set_signing_config(None);

        updated.unwrap();
        assert!(deleted.unwrap().existed);
        // An update's field blocks are past priority 1 and go unsigned: its
        // composite is the one signature, built from heads read before the
        // transaction and signed once.
        assert_eq!(waves_after_update, 1);
        assert_eq!(signatures_after_update, 1);
        assert_eq!(
            signer.waves.load(Ordering::SeqCst),
            2,
            "the delete's one wave"
        );
        assert_eq!(signer.signatures.load(Ordering::SeqCst), 2);
    })
    .unwrap()
}

/// A store that lets one other write commit inside the next retried
/// transaction's first attempt, after that attempt's reads and before its
/// commit, so the attempt's commit loses and the engine retries it.
struct WinnerDuringFirstAttempt {
    inner: RegolithStore,
    winner: regolith::sync::SegQueue<MaybeBoxFuture<'static, ()>>,
}

impl storage::corekv::private::Sealed for WinnerDuringFirstAttempt {}

#[async_trait]
impl storage::corekv::Store for WinnerDuringFirstAttempt {
    async fn allocate(&self, key: &[u8], count: u64) -> storage::Result<std::ops::Range<u64>> {
        self.inner.allocate(key, count).await
    }

    async fn new_txn(&self, readonly: bool) -> storage::Result<Box<dyn storage::Txn>> {
        self.inner.new_txn(readonly).await
    }

    async fn transact_retry(
        &self,
        options: storage::corekv::RetryOptions,
        attempt: storage::corekv::RetryAttempt,
    ) -> Result<storage::corekv::RetryAttemptValue, storage::corekv::RetryAttemptError> {
        let attempt = match self.winner.pop() {
            Some(winner) => Box::new(WinnerAfterReads {
                attempt,
                winner: Some(winner),
            }),
            None => attempt,
        };
        self.inner.transact_retry(options, attempt).await
    }

    async fn close(&self) -> storage::Result<()> {
        self.inner.close().await
    }
}

/// Runs the winner's write after the first attempt's own work, before the
/// engine commits it.
struct WinnerAfterReads {
    attempt: storage::corekv::RetryAttempt,
    winner: Option<MaybeBoxFuture<'static, ()>>,
}

impl storage::corekv::RetryAttemptFn for WinnerAfterReads {
    fn run<'a>(
        &'a mut self,
        txn: &'a mut dyn storage::corekv::RetryTxn,
    ) -> storage::corekv::RetryAttemptFuture<'a> {
        Box::pin(async move {
            let value = self.attempt.run(txn).await?;
            if let Some(winner) = self.winner.take() {
                winner.await;
            }
            Ok(value)
        })
    }
}

#[test]
fn a_retry_after_a_conflict_builds_on_the_winner() {
    workers::block_on(async {
        let store = Arc::new(WinnerDuringFirstAttempt {
            inner: RegolithStore::in_memory().unwrap(),
            winner: regolith::sync::SegQueue::new(),
        });
        let db = Arc::new(DB::open_from_arc(store.clone()).await.unwrap());
        let mut fields = vec![FieldDescription::new("1", "_docID", FieldKind::doc_id())];
        for (index, name) in FIELDS.iter().enumerate() {
            fields.push(FieldDescription::new(
                (index + 2).to_string(),
                *name,
                FieldKind::string(),
            ));
        }
        db.create_collection(CollectionVersion::new("Things", "v1", "col-things", fields))
            .await
            .unwrap();
        let mutator = AutoCommitMutator::new(Arc::clone(&db));
        let created = mutator
            .create("Things", documents(1).remove(0))
            .await
            .unwrap();
        let field: rapidhash::RapidHashSet<String> = ["a".to_string()].into_iter().collect();

        // The winner writes field `a` while the loser's first attempt is open.
        let winner = Arc::new(regolith::sync::OnceCell::new());
        {
            let (winner, mutator, field) = (
                Arc::clone(&winner),
                AutoCommitMutator::new(Arc::clone(&db)),
                field.clone(),
            );
            let mut doc = created.document.clone();
            doc.set("a", "winner");
            store.winner.push(Box::pin(async move {
                let result = mutator.update("Things", doc, field).await.unwrap();
                let _ = winner.set(result);
            }));
        }
        let mut doc = created.document.clone();
        doc.set("a", "loser");
        let loser = mutator.update("Things", doc, field).await.unwrap();
        let winner = winner
            .get()
            .expect("the winner committed during the first attempt");

        // The loser retried on fresh heads: its composite is the winner's child,
        // one priority above it, not a sibling at the same priority.
        let winner_block = Block::from_dag_cbor(winner.commit_block.as_ref().unwrap()).unwrap();
        let loser_block = Block::from_dag_cbor(loser.commit_block.as_ref().unwrap()).unwrap();
        assert_eq!(loser_block.heads, Some(vec![winner.commit_cid.unwrap()]));
        assert_eq!(
            loser_block.delta.priority(),
            winner_block.delta.priority() + 1
        );
        // The last write's value wins.
        let stored = mutator
            .get_for_update("Things", &created.doc_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.get("a"), Some(&NormalValue::String("loser".into())));
    })
    .unwrap()
}

/// [`embedding_server`], whose threads are its own, so the test's thread
/// runs none of the server.
struct ServerApart {
    url: String,
    requests: Arc<AtomicUsize>,
    _server: HttpServer,
}

impl ServerApart {
    fn start() -> Self {
        let (server, requests, _) = embedding_server();
        Self {
            url: server.url(),
            requests,
            _server: server,
        }
    }
}

/// A schema-declared embedding endpoint works for a database the node gave
/// no network runtime, written from a thread that runs no runtime: the
/// request goes out on the process's on-demand network runtime.
#[test]
fn a_database_given_no_network_runtime_embeds_from_a_thread_without_one() {
    let server = ServerApart::start();
    let created = n0_future::future::block_on(async {
        let db = embedded_database(&server.url).await;
        AutoCommitMutator::new(db)
            .create("Texts", text_document("alpha"))
            .await
            .unwrap()
    });

    assert_eq!(server.requests.load(Ordering::SeqCst), 1);
    assert_eq!(
        created.document.get("vector"),
        Some(&NormalValue::Float64Array(vec![6.0, f64::from(b'a')])),
        "the vector of \"alpha\\n\""
    );
}

/// `set_embedding` and a hybrid search's query embedding both go through the
/// embedding service on the network runtime, never from the calling task: a
/// caller with no runtime gets its vectors.
#[test]
fn set_embedding_and_a_hybrid_query_embed_from_a_thread_without_a_runtime() {
    use db::{DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
    use query::QueryExecutor;

    let server = ServerApart::start();
    n0_future::future::block_on(async {
        let description = schema::VectorEmbeddingDescription {
            field_name: "vector".to_string(),
            fields: vec!["text".to_string()],
            model: "test-model".to_string(),
            provider: "openai".to_string(),
            template: String::new(),
            url: server.url.clone(),
        };
        let mut doc = text_document("bravo");
        let generated = db::search::set_embedding(
            &[description],
            &mut doc,
            true,
            None,
            &db::EmbeddingClientConfig::new(),
        )
        .await
        .unwrap();
        assert_eq!(generated, vec!["vector"]);
        assert_eq!(
            doc.get("vector"),
            Some(&NormalValue::Float64Array(vec![6.0, f64::from(b'b')]))
        );

        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        for schema in query::parse_sdl("type Docs { text: String vector: [Float!] }").unwrap() {
            db.create_collection(schema).await.unwrap();
        }
        let runner = query::QueryRunner::with_arc_registry_and_provider(
            LensedAutoCommitFetcher::new(db.clone()),
            DbCollectionProvider::new_arc(db.clone()),
            Arc::new(DbTransactionRegistry::new(db.clone())),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(db)));
        let created = runner
            .execute(query::QueryRequest::new(
                r#"mutation { add_Docs(input: {text: "question", vector: [1, 0]}) { _docID } }"#,
            ))
            .await;
        assert!(!created.has_errors(), "{:?}", created.errors);
        let request = db::DenseHybridSearchRequest::new("Docs", "question", "vector", ["text"])
            .with_embedding_url(&server.url)
            .with_embedding_model("test-model");
        let result = db::hybrid_search_dense(&runner, &db::EmbeddingClientConfig::new(), &request)
            .await
            .unwrap();
        assert_eq!(result.query_vector_dimensions, 2);
        assert_eq!(result.hits.len(), 1);
    });

    assert_eq!(
        server.requests.load(Ordering::SeqCst),
        2,
        "one request for the document, one for the query"
    );
}
