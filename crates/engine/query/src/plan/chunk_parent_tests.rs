use std::sync::Arc;

use chunk_split::hash::text_hash as hash;
use chunk_split::split::{split, ChunkConfig};
use document::{DocID, Document, NormalValue};
use schema::{
    CType, ChunkSource, ChunkedField, CollectionBuilder, CollectionVersion, FieldDescription,
    FieldKind, ScalarKind,
};

use crate::fetcher::DocFetcher;
use crate::plan::chunk_gate::ChunkGate;
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

fn piece(text: &str) -> String {
    let config = ChunkConfig::new(defra_core::DEFAULT_SPLITTER_VERSION, 64, 0).unwrap();
    split(text, &config).remove(0).text
}

struct Store {
    fetcher: Arc<MockFetcher>,
    parent_id: String,
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
        let id = DocID::new_v0_from_seed(&format!("{text} {claimed}"));
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
