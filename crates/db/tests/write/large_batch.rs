//! One mutation that creates several hundred documents, each with an
//! embedding and KMS-keyed encryption, commits in one request: every input it
//! prepares is kept for every attempt, however many inputs the request has.

use crate::common::http::HttpServer;
use async_trait::async_trait;
use db::{AutoCommitMutator, DbCollectionProvider, LensedAutoCommitFetcher, DB};
use kovan_map::HopscotchMap;
use query::{QueryExecutor, QueryRequest};
use rapidhash::fast::RandomState;
use schema::{CollectionVersion, FieldDescription, FieldKind, VectorEmbeddingDescription};
use serde_json::{json, Value};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use storage::RegolithStore;

const DOCUMENTS: usize = 400;
const DIMENSIONS: usize = 384;
/// Texts one embedding request carries at most.
const EMBEDDING_BATCH: usize = 256;

/// Hands out a fresh key per call and serves the keys it handed out.
struct KeyingKms {
    issued: AtomicU64,
    keys: HopscotchMap<kms::EncryptionCid, [u8; 32], RandomState>,
}

#[async_trait]
impl kms::KmsService for KeyingKms {
    async fn generate_key(
        &self,
        _: &kms::RequestContext,
        _: kms::KeyScope,
    ) -> kms::Result<(kms::EncryptionCid, [u8; 32])> {
        let mut key = [0_u8; 32];
        key[..8].copy_from_slice(&self.issued.fetch_add(1, Ordering::SeqCst).to_be_bytes());
        let block = defra_core::Encryption { key: key.to_vec() };
        let cid =
            defra_core::block::generate_cid_from_bytes(&block.to_dag_cbor().unwrap()).unwrap();
        self.keys.insert(cid, key);
        Ok((cid, key))
    }

    async fn get_keys(
        &self,
        _: &kms::RequestContext,
        cids: &[kms::EncryptionCid],
    ) -> kms::Result<kms::KeyResults> {
        let (results, sender) = kms::KeyResults::new(cids.len());
        for cid in cids {
            let key = self.keys.get(cid).ok_or(kms::Error::KeyUnavailable)?;
            sender.send(Ok((*cid, key))).await.unwrap();
        }
        drop(sender);
        Ok(results)
    }

    async fn serve_request(
        &self,
        _: kms::PeerIdentity,
        _: kms::FetchEncryptionKeyRequest,
    ) -> kms::Result<kms::FetchEncryptionKeyReply> {
        Err(kms::Error::Unsupported("unexpected peer request"))
    }
}

#[test]
fn a_batch_of_hundreds_of_embedded_encrypted_creates_commits() {
    workers::block_on(async {
        let embedding_calls = Arc::new(AtomicUsize::new(0));
        let embedded_texts = Arc::new(AtomicUsize::new(0));
        let calls = Arc::clone(&embedding_calls);
        let texts = Arc::clone(&embedded_texts);
        // An OpenAI-compatible endpoint: `input` is one text or an array of
        // them, answered in order with their indices.
        let server = HttpServer::start(&["/embeddings"], move |request| {
            calls.fetch_add(1, Ordering::SeqCst);
            let count = match &request.body["input"] {
                Value::Array(inputs) => inputs.len(),
                _ => 1,
            };
            texts.fetch_add(count, Ordering::SeqCst);
            let data: Vec<Value> = (0..count)
                .map(|index| json!({"index": index, "embedding": vec![0.25_f64; DIMENSIONS]}))
                .collect();
            json!({ "data": data })
        });

        let db = Arc::new(DB::new(RegolithStore::in_memory().unwrap()).unwrap());
        let kms = Arc::new(KeyingKms {
            issued: AtomicU64::new(1),
            keys: HopscotchMap::with_hasher(RandomState::default()),
        });
        db.set_kms(kms.clone());
        let mut collection = CollectionVersion::new(
            "Texts",
            "texts-v1",
            "col-texts",
            vec![
                FieldDescription::new("1", "_docID", FieldKind::doc_id()),
                FieldDescription::new("2", "text", FieldKind::string()),
                FieldDescription::new("3", "vector", FieldKind::float64_array()),
            ],
        );
        collection
            .vector_embeddings
            .push(VectorEmbeddingDescription {
                field_name: "vector".to_string(),
                fields: vec!["text".to_string()],
                model: "test-model".to_string(),
                provider: "openai".to_string(),
                template: String::new(),
                url: server.url(),
            });
        db.create_collection(collection).await.unwrap();

        let runner = query::QueryRunner::with_provider(
            LensedAutoCommitFetcher::new(Arc::clone(&db)),
            DbCollectionProvider::new_arc(Arc::clone(&db)),
        )
        .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))));
        let inputs = (0..DOCUMENTS)
            .map(|index| format!(r#"{{text: "document number {index}"}}"#))
            .collect::<Vec<_>>()
            .join(", ");
        let response = runner
            .execute(QueryRequest::new(format!(
                "mutation {{ add_Texts(input: [{inputs}], encrypt: true) {{ _docID }} }}"
            )))
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let created = response.data.unwrap()["add_Texts"]
            .as_array()
            .unwrap()
            .len();
        assert_eq!(created, DOCUMENTS);
        // Each embedding is fetched once, before the transaction, whatever the
        // number of attempts, and the request's embeddings go out together: one
        // request per batch of texts, not one per document.
        assert_eq!(embedded_texts.load(Ordering::SeqCst), DOCUMENTS);
        assert_eq!(
            embedding_calls.load(Ordering::SeqCst),
            DOCUMENTS.div_ceil(EMBEDDING_BATCH)
        );

        let response = runner
            .execute(QueryRequest::new("{ Texts { _docID text vector } }"))
            .await;
        assert!(!response.has_errors(), "{:?}", response.errors);
        let stored = response.data.unwrap()["Texts"].as_array().unwrap().clone();
        assert_eq!(stored.len(), DOCUMENTS);
        assert!(stored.iter().all(|doc| doc["text"]
            .as_str()
            .unwrap()
            .starts_with("document number ")));
        // Every document has its own vector, whole.
        assert!(stored.iter().all(|doc| doc["vector"]
            .as_array()
            .is_some_and(|vector| vector.len() == DIMENSIONS
                && vector.iter().all(|value| value.as_f64() == Some(0.25)))));
    })
    .unwrap()
}
