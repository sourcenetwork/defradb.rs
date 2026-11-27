use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use async_trait::async_trait;
use chunk_split::hash::text_hash as hash;
use chunk_split::split::{split, ChunkConfig};
use document::{DocID, Document, NormalValue};
use schema::{
    CType, ChunkSource, ChunkedField, CollectionBuilder, CollectionVersion, FieldDescription,
    FieldKind, IndexDescription, ScalarKind, CHUNK_HASH_FIELD, CHUNK_SOURCE_ID_FIELD,
};

use crate::doc_stream::DocStream;
use crate::fetcher::{DocFetcher, FetchByIdsResult, IndexScanEntry, IndexScanResult};
use crate::plan::chunk_gate::ChunkGate;
use crate::planner::IndexScanType;
use crate::test_utils::MockFetcher;

const PARENTS: &str = "Article";
const CHUNKS: &str = "Article__body_chunk";

fn chunked(splitter_version: u32) -> ChunkedField {
    ChunkedField {
        field_name: "body".into(),
        max_chars: 64,
        overlap: 0,
        splitter_version,
    }
}

fn parents_with(chunked: ChunkedField) -> CollectionVersion {
    let mut parents = collection(PARENTS, &["body"]);
    parents.chunked_fields = vec![chunked];
    parents
}

fn collection(name: &str, fields: &[&str]) -> CollectionVersion {
    fields
        .iter()
        .enumerate()
        .fold(
            CollectionBuilder::new(name, "bafytest"),
            |builder, (i, field)| {
                builder.field(FieldDescription::new(
                    i.to_string(),
                    *field,
                    FieldKind::Scalar(ScalarKind::None),
                    CType::LwwRegister,
                ))
            },
        )
        .build()
}

fn parents(splitter_version: u32) -> CollectionVersion {
    parents_with(chunked(splitter_version))
}

fn chunks() -> CollectionVersion {
    let mut chunks = collection(CHUNKS, &["_sourceID", "text", "hash"]);
    chunks.chunk_of = Some(ChunkSource {
        collection: PARENTS.into(),
        field_name: "body".into(),
    });
    chunks
}

fn indexed_chunks() -> CollectionVersion {
    let mut chunks = chunks();
    chunks.indexes.push(
        IndexDescription::new("source_hash")
            .with_field(CHUNK_SOURCE_ID_FIELD, false)
            .with_field(CHUNK_HASH_FIELD, false),
    );
    chunks
}

fn piece(text: &str) -> String {
    let config = ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).unwrap();
    split(text, &config).remove(0).text
}

struct Store {
    fetcher: Arc<MockFetcher>,
    parent_id: String,
}

struct IndexedStore {
    inner: Arc<MockFetcher>,
    entries: Vec<IndexScanEntry>,
    point_scans: AtomicUsize,
    prefix_scans: AtomicUsize,
}

impl IndexedStore {
    fn new(inner: Arc<MockFetcher>, entries: Vec<IndexScanEntry>) -> Self {
        Self {
            inner,
            entries,
            point_scans: AtomicUsize::new(0),
            prefix_scans: AtomicUsize::new(0),
        }
    }

    fn point_scans(&self) -> usize {
        self.point_scans.load(Ordering::Relaxed)
    }

    fn prefix_scans(&self) -> usize {
        self.prefix_scans.load(Ordering::Relaxed)
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl DocFetcher for IndexedStore {
    async fn stream_by_doc_short_ids(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> crate::error::Result<Box<dyn DocStream>> {
        self.inner
            .stream_by_doc_short_ids(collection_name, doc_short_ids, show_deleted)
            .await
    }

    async fn get_all(&self, collection_name: &str) -> crate::error::Result<Vec<Document>> {
        self.inner.get_all(collection_name).await
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> crate::error::Result<Box<dyn DocStream>> {
        self.inner
            .stream_all_with_deleted(collection_name, show_deleted)
            .await
    }

    async fn get_by_ids(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> crate::error::Result<FetchByIdsResult> {
        self.inner.get_by_ids(collection_name, doc_ids).await
    }

    async fn get_by_ids_projected(
        &self,
        collection_name: &str,
        doc_ids: &[String],
        _fields: &rapidhash::RapidHashSet<String>,
    ) -> crate::error::Result<Option<FetchByIdsResult>> {
        Ok(Some(self.inner.get_by_ids(collection_name, doc_ids).await?))
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> crate::error::Result<Vec<Document>> {
        self.inner
            .get_by_field_value(collection_name, field_name, value)
            .await
    }

    async fn get_by_index_scan(
        &self,
        _collection_name: &str,
        params: &crate::planner::IndexScanParams,
    ) -> crate::error::Result<IndexScanResult> {
        self.point_scans.fetch_add(1, Ordering::Relaxed);
        let IndexScanType::PrefixScan { prefix_values, .. } = &params.scan_type else {
            panic!("expected a prefix scan");
        };
        let ids = self
            .entries
            .iter()
            .filter(|entry| entry.values.starts_with(prefix_values))
            .map(|entry| entry.doc_id.clone())
            .collect();
        Ok(IndexScanResult::new(ids))
    }

    async fn get_by_index_prefix_with_values(
        &self,
        _collection_name: &str,
        _index_name: &str,
        prefix_values: &[NormalValue],
    ) -> crate::error::Result<Option<Vec<IndexScanEntry>>> {
        self.prefix_scans.fetch_add(1, Ordering::Relaxed);
        Ok(Some(
            self.entries
                .iter()
                .filter(|entry| entry.values.starts_with(prefix_values))
                .cloned()
                .collect(),
        ))
    }

    fn supports_index_queries(&self) -> bool {
        true
    }
}

impl Store {
    fn new(body: Option<&str>) -> Self {
        Self::with(body, false)
    }

    fn with(body: Option<&str>, deleted: bool) -> Self {
        let fetcher = Arc::new(MockFetcher::new());
        let parent_id = DocID::new_v0_from_seed("parent").to_string();
        let mut parent = Document::new(parents(defra_core::DEFAULT_SPLITTER_VERSION));
        parent.set_id(DocID::from_string(&parent_id).unwrap());
        let body = body.map_or(NormalValue::Null, |body| NormalValue::String(body.into()));
        parent.set("body", body).unwrap();
        parent.set_deleted(deleted);
        fetcher.add_doc(PARENTS, parent);
        Self { fetcher, parent_id }
    }

    /// Stores a chunk of the parent and returns its DocID.
    fn chunk(&self, text: &str, claimed: &str) -> String {
        self.copy(text, claimed, "")
    }

    /// Stores a copy of a chunk under an ID of its own, as a peer that
    /// encrypts it with its own key does, and returns its DocID.
    fn copy(&self, text: &str, claimed: &str, writer: &str) -> String {
        let id = DocID::new_v0_from_seed(&format!("{text} {claimed}{writer}"));
        let mut doc = Document::new(chunks());
        doc.set_id(id.clone());
        doc.set("_sourceID", NormalValue::String(self.parent_id.clone()))
            .unwrap();
        doc.set("text", NormalValue::String(text.into())).unwrap();
        doc.set("hash", NormalValue::String(claimed.into()))
            .unwrap();
        self.fetcher.add_doc(CHUNKS, doc);
        id.to_string()
    }

    fn gate(&self, splitter_version: u32) -> ChunkGate {
        self.gate_for(chunked(splitter_version))
    }

    fn gate_for(&self, chunked: ChunkedField) -> ChunkGate {
        let fetcher: Arc<dyn DocFetcher> = self.fetcher.clone();
        ChunkGate::new(
            Some(fetcher),
            &chunks(),
            Some(&parents_with(chunked)),
            None,
            None,
        )
    }

    async fn shows(&self, chunk_id: &str) -> bool {
        self.gate(defra_core::DEFAULT_SPLITTER_VERSION)
            .allows(chunk_id, None)
            .await
            .unwrap()
    }
}

#[test]
fn a_chunk_claiming_the_texts_hash_for_other_text_hides() {
    workers::block_on(async {
        let text = piece("Real text.");
        let store = Store::new(Some("Real text."));
        let chunk = store.chunk("forged", &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::with(Some("Some text."), true);
        let chunk = store.chunk(&text, &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_of_other_text_hides_and_the_texts_own_chunk_shows() {
    workers::block_on(async {
        let mine = piece("Mine is short.");
        let theirs = piece("Theirs is short.");
        let store = Store::new(Some("Mine is short."));
        let shown = store.chunk(&mine, &hash(&mine));
        let hidden = store.chunk(&theirs, &hash(&theirs));
        assert!(store.shows(&shown).await);
        assert!(!store.shows(&hidden).await);
    })
    .unwrap()
}

#[test]
fn a_parent_without_text_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::new(None);
        let chunk = store.chunk(&text, &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_splitter_version_this_node_lacks_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::new(Some("Some text."));
        let chunk = store.chunk(&text, &hash(&text));
        let shown = store.gate(u32::MAX).allows(&chunk, None).await.unwrap();
        assert!(!shown);
    })
    .unwrap()
}

#[test]
fn a_gate_without_a_fetcher_or_parent_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::new(Some("Some text."));
        let chunk = store.chunk(&text, &hash(&text));
        let fetcher: Arc<dyn DocFetcher> = store.fetcher.clone();
        let no_parent = ChunkGate::new(Some(fetcher), &chunks(), None, None, None);
        let no_fetcher = ChunkGate::new(
            None,
            &chunks(),
            Some(&parents(defra_core::DEFAULT_SPLITTER_VERSION)),
            None,
            None,
        );
        assert!(!no_parent.allows(&chunk, None).await.unwrap());
        assert!(!no_fetcher.allows(&chunk, None).await.unwrap());
    })
    .unwrap()
}

#[test]
fn a_later_chunk_of_a_long_text_split_with_overlap_shows() {
    workers::block_on(async {
        // The repeat is capped at a share of maxChars, so the sentences stay short.
        let body = "Aa a. Bb b. Cc c. Dd d. Ee ee e.";
        let version = defra_core::DEFAULT_SPLITTER_VERSION;
        let pieces = split(body, &ChunkConfig::new(version, 24, 1).unwrap());
        let unshared = split(body, &ChunkConfig::new(version, 24, 0).unwrap());
        assert!(pieces.len() > 1 && pieces[1].hash != unshared[1].hash);
        let store = Store::new(Some(body));
        let later = store.chunk(&pieces[1].text, &pieces[1].hash);
        let settings = ChunkedField {
            max_chars: 24,
            overlap: 1,
            ..chunked(version)
        };

        assert!(store.gate_for(settings).allows(&later, None).await.unwrap());
    })
    .unwrap()
}

#[test]
fn two_copies_of_one_chunk_show_once_as_the_smallest_doc_id() {
    workers::block_on(async {
        let text = piece("Shared text.");
        let store = Store::new(Some("Shared text."));
        let first = store.copy(&text, &hash(&text), " by one peer");
        let second = store.copy(&text, &hash(&text), " by another");
        let forged = store.copy("forged", &hash(&text), "");
        let smallest = first.clone().min(second.clone());

        let gate = store.gate(defra_core::DEFAULT_SPLITTER_VERSION);
        assert!(gate.allows(&first, None).await.unwrap() == (first == smallest));
        assert!(gate.allows(&second, None).await.unwrap() == (second == smallest));
        assert!(!gate.allows(&forged, None).await.unwrap());
        assert!(store.shows(&smallest).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_listing_scans_its_parent_once_for_all_copies() {
    workers::block_on(async {
        let first = "The first paragraph is long enough to fill a chunk by itself.";
        let second = "The second paragraph is also long enough to fill its own chunk.";
        let body = format!("{first}\n\n{second}");
        let config = ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).unwrap();
        let pieces = split(&body, &config);
        assert_eq!(pieces.len(), 2);

        let store = Store::new(Some(&body));
        let first_shown = store.copy(&pieces[0].text, &pieces[0].hash, " readable");
        let second_shown = store.copy(&pieces[1].text, &pieces[1].hash, " readable");
        let first_hidden = (0..)
            .map(|i| store.copy("ciphertext", &pieces[0].hash, &format!(" hidden {i}")))
            .find(|id| id < &first_shown)
            .unwrap();
        let second_hidden = (0..)
            .map(|i| store.copy("ciphertext", &pieces[1].hash, &format!(" hidden {i}")))
            .find(|id| id < &second_shown)
            .unwrap();
        let entries = [
            (&first_shown, pieces[0].hash.as_str()),
            (&first_hidden, pieces[0].hash.as_str()),
            (&second_shown, pieces[1].hash.as_str()),
            (&second_hidden, pieces[1].hash.as_str()),
        ]
        .into_iter()
        .map(|(doc_id, hash)| IndexScanEntry {
            doc_id: doc_id.clone(),
            values: vec![
                NormalValue::String(store.parent_id.clone()),
                NormalValue::String(hash.to_owned()),
            ],
        })
        .collect();
        let fetcher = Arc::new(IndexedStore::new(store.fetcher.clone(), entries));
        let fetcher_trait: Arc<dyn DocFetcher> = fetcher.clone();
        let gate = ChunkGate::new(
            Some(fetcher_trait),
            &indexed_chunks(),
            Some(&parents(defra_core::DEFAULT_SPLITTER_VERSION)),
            None,
            None,
        );

        assert!(gate.allows(&first_shown, None).await.unwrap());
        assert!(gate.allows(&second_shown, None).await.unwrap());
        assert!(gate
            .allows(&first_hidden, None)
            .await
            .is_ok_and(|shown| !shown));
        assert_eq!(fetcher.point_scans(), 1);
        assert_eq!(fetcher.prefix_scans(), 1);
    })
    .unwrap();
}

#[test]
fn a_copy_whose_text_does_not_match_never_hides_the_real_chunk() {
    workers::block_on(async {
        let text = piece("Shared text.");
        let store = Store::new(Some("Shared text."));
        let real = store.chunk(&text, &hash(&text));
        let unreadable = (0..)
            .map(|i| store.copy("ciphertext", &hash(&text), &format!(" {i}")))
            .find(|id| id < &real)
            .unwrap();
        assert!(!store.shows(&unreadable).await);
        assert!(store.shows(&real).await);
    })
    .unwrap()
}
