use std::sync::Arc;

use chunk_split::list::text_hash as hash;
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
    let mut parents = collection(PARENTS, &["body", "_body_chunks"]);
    parents.chunked_fields = vec![chunked(splitter_version)];
    parents
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
    fn new(body: Option<&str>, list: Vec<String>) -> Self {
        Self::with(body, list, false)
    }

    fn with(body: Option<&str>, list: Vec<String>, deleted: bool) -> Self {
        let fetcher = Arc::new(MockFetcher::new());
        let parent_id = DocID::new_v0_from_seed("parent").to_string();
        let mut parent = Document::new(parents(defra_core::DEFAULT_SPLITTER_VERSION));
        parent.set_id(DocID::from_string(&parent_id).unwrap());
        let body = body.map_or(NormalValue::Null, |body| NormalValue::String(body.into()));
        parent.set("body", body).unwrap();
        parent
            .set("_body_chunks", NormalValue::StringArray(list))
            .unwrap();
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
        let fetcher: Arc<dyn DocFetcher> = self.fetcher.clone();
        ChunkGate::new(
            Some(fetcher),
            &chunks(),
            Some(&parents(splitter_version)),
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
fn a_chunk_of_the_parents_text_shows() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::new(Some("Some text."), vec![hash("Some text."), hash(&text)]);
        let chunk = store.chunk(&text, &hash(&text));
        assert!(store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_of_old_text_hides_though_a_list_for_the_new_text_names_it() {
    workers::block_on(async {
        let old = piece("Old text.");
        let store = Store::new(Some("whole"), vec![hash("whole"), hash(&old)]);
        let chunk = store.chunk(&old, &hash(&old));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn the_whole_text_hash_does_not_list_a_chunk() {
    workers::block_on(async {
        let store = Store::new(Some("whole"), vec![hash("whole")]);
        let chunk = store.chunk("other", &hash("whole"));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_chunk_claiming_the_texts_hash_for_other_text_hides() {
    workers::block_on(async {
        let text = piece("Real text.");
        let store = Store::new(Some("Real text."), vec![hash("Real text."), hash(&text)]);
        let chunk = store.chunk("forged", &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_deleted_parent_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::with(Some("Some text."), Vec::new(), true);
        let chunk = store.chunk(&text, &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_list_of_other_text_is_ignored_for_the_chunks_of_the_text() {
    workers::block_on(async {
        let mine = piece("Mine is short.");
        let theirs = piece("Theirs is short.");
        let store = Store::new(Some("Mine is short."), vec![hash(&theirs), hash(&theirs)]);
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
        let store = Store::new(None, vec![hash("Some text."), hash(&text)]);
        let chunk = store.chunk(&text, &hash(&text));
        assert!(!store.shows(&chunk).await);
    })
    .unwrap()
}

#[test]
fn a_splitter_version_this_node_lacks_shows_no_chunk() {
    workers::block_on(async {
        let text = piece("Some text.");
        let store = Store::new(Some("Some text."), Vec::new());
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
        let store = Store::new(Some("Some text."), Vec::new());
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
