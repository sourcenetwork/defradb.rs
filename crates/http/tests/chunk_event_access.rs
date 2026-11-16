mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, HOST};
use axum::http::Request;
use defra_http::router::{AppStateBuilder, CollectionManagementOperations};
use defra_http::{MockDocumentAcpOperations, MockNodeAcpOperations};
use document::{DocID, Document, NormalValue};
use events::{Bus, ChannelBus, Message, Update};
use futures::StreamExt;
use query::doc_stream::{DocStream, VecStream};
use query::fetcher::{DocFetcher, FetchByIdsResult};
use query::QueryRunner;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

use common::{NodePermission, StatusCode, TEST_HOST};

const SENTINEL: &str = "sentinel";

/// Serves the given collections by ID.
struct Collections(Vec<schema::CollectionVersion>);

#[async_trait::async_trait]
impl CollectionManagementOperations for Collections {
    async fn list_actions(&self) -> Result<Vec<defra_core::ActionExecution>, String> {
        Ok(Vec::new())
    }

    async fn purge(&self) -> Result<(), String> {
        Ok(())
    }

    async fn get_collection_by_name(
        &self,
        name: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(self.0.iter().find(|c| c.name == name).cloned())
    }

    async fn has_collection(&self, name: &str) -> Result<bool, String> {
        Ok(self.0.iter().any(|c| c.name == name))
    }

    async fn find_collection_by_id(
        &self,
        collection_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(self
            .0
            .iter()
            .find(|c| c.collection_id == collection_id)
            .cloned())
    }

    async fn get_collection_by_version_id(
        &self,
        version_id: &str,
    ) -> Result<Option<schema::CollectionVersion>, String> {
        Ok(self.0.iter().find(|c| c.version_id == version_id).cloned())
    }

    async fn delete_collection_versions(&self, _version_ids: Vec<String>) -> Result<(), String> {
        Ok(())
    }

    async fn delete_collection(&self, _name: &str) -> Result<(), String> {
        Ok(())
    }

    async fn delete_collections(
        &self,
        _names: Vec<String>,
        _active_only: bool,
    ) -> Result<(), String> {
        Ok(())
    }
}

fn collections() -> Vec<schema::CollectionVersion> {
    let mut all = Vec::new();
    for sdl in [
        "type Users { body: String @chunk }",
        "type Notes { body: String @chunk }",
        "type Plain { body: String }",
    ] {
        all.extend(query::parse_sdl(sdl).unwrap());
    }
    for (index, collection) in all.iter_mut().enumerate() {
        collection.collection_id = format!("collection-{index}");
    }
    all
}

fn id_of(name: &str) -> String {
    collections()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap()
        .collection_id
}

/// Documents held in memory, enough for the chunk gate's reads.
#[derive(Default)]
struct Store(Vec<(String, Document)>);

impl Store {
    fn add_doc(&mut self, collection: &str, doc: Document) {
        self.0.push((collection.to_string(), doc));
    }

    fn docs(&self, collection: &str) -> Vec<Document> {
        self.0
            .iter()
            .filter(|(name, _)| name == collection)
            .map(|(_, doc)| doc.clone())
            .collect()
    }
}

#[async_trait::async_trait]
impl DocFetcher for Store {
    async fn get_all(&self, collection_name: &str) -> query::error::Result<Vec<Document>> {
        Ok(self.docs(collection_name))
    }

    async fn stream_by_doc_short_ids(
        &self,
        collection_name: &str,
        doc_short_ids: &[u64],
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn DocStream>> {
        let all = self
            .get_all_with_deleted(collection_name, show_deleted)
            .await?;
        let picked = doc_short_ids
            .iter()
            .filter_map(|id| all.get(id.checked_sub(1)? as usize).cloned())
            .collect();
        Ok(Box::new(VecStream::new(picked)))
    }

    async fn stream_all_with_deleted(
        &self,
        collection_name: &str,
        show_deleted: bool,
    ) -> query::error::Result<Box<dyn DocStream>> {
        Ok(Box::new(VecStream::new(
            self.get_all_with_deleted(collection_name, show_deleted)
                .await?,
        )))
    }

    async fn get_by_ids(
        &self,
        collection_name: &str,
        doc_ids: &[String],
    ) -> query::error::Result<FetchByIdsResult> {
        let all = self.docs(collection_name);
        let (mut found, mut missing) = (Vec::new(), Vec::new());
        for id in doc_ids {
            match all
                .iter()
                .find(|doc| doc.id().is_some_and(|doc_id| doc_id.to_string() == *id))
            {
                Some(doc) => found.push(doc.clone()),
                None => missing.push(id.clone()),
            }
        }
        Ok(FetchByIdsResult::partial(found, missing))
    }

    async fn get_by_field_value(
        &self,
        collection_name: &str,
        field_name: &str,
        value: &str,
    ) -> query::error::Result<Vec<Document>> {
        Ok(self
            .docs(collection_name)
            .into_iter()
            .filter(|doc| doc.get(field_name).and_then(|v| v.as_str()) == Some(value))
            .collect())
    }
}

fn readable() -> String {
    DocID::new_v0_from_seed("chunk of a live parent").to_string()
}

fn hidden() -> String {
    DocID::new_v0_from_seed("chunk of a deleted parent").to_string()
}

fn doc(collection: &str, id: &str, fields: &[(&str, &str)]) -> Document {
    let version = collections()
        .into_iter()
        .find(|c| c.name == collection)
        .unwrap();
    let mut doc = Document::new(version);
    doc.set_id(DocID::from_string(id).unwrap());
    for (name, value) in fields {
        doc.set(*name, NormalValue::String((*value).into()))
            .unwrap();
    }
    doc
}

/// A real query runner over a store with one chunk of a live `Users` parent
/// and one of a deleted `Notes` parent, so the chunk gate itself decides.
fn executor() -> Arc<dyn query::QueryExecutor> {
    let mut fetcher = Store::default();
    let body = "Some text.";
    for (parents, chunk_id, deleted) in [("Users", readable(), false), ("Notes", hidden(), true)] {
        let version = collections()
            .into_iter()
            .find(|c| c.name == parents)
            .unwrap();
        let chunked = &version.chunked_fields[0];
        let config = chunk_split::split::ChunkConfig::new(
            chunked.splitter_version,
            chunked.max_chars as usize,
            chunked.overlap as usize,
        )
        .unwrap();
        let piece = chunk_split::split::split(body, &config).remove(0);
        let parent_id = DocID::new_v0_from_seed(parents).to_string();
        let mut parent = doc(parents, &parent_id, &[("body", body)]);
        parent.set_deleted(deleted);
        fetcher.add_doc(parents, parent);
        let chunks = format!("{parents}__body_chunk");
        fetcher.add_doc(
            &chunks,
            doc(
                &chunks,
                &chunk_id,
                &[
                    ("_sourceID", &parent_id),
                    ("text", &piece.text),
                    ("hash", &piece.hash),
                ],
            ),
        );
    }
    Arc::new(QueryRunner::new(fetcher, collections()))
}

fn update(collection: &str, doc_id: &str) -> Message {
    Message::update(Update::new(
        doc_id.to_string(),
        cid::Cid::default(),
        id_of(collection),
        Vec::new(),
        false,
        false,
    ))
}

/// The doc ID of the first event the stream delivers after `message` is
/// published, followed by an event nothing gates.
async fn first_delivered(acp_enabled: bool, message: Message) -> String {
    let (owner, _) = common::nac_owner();
    let (caller, token) = common::nac_owner();
    let mut nac = MockNodeAcpOperations::enabled_with_owner(owner);
    for permission in [
        NodePermission::DocumentRead,
        NodePermission::P2pPeerInfo,
        NodePermission::DacStatus,
    ] {
        nac = nac.with_grant(caller.clone(), permission);
    }
    let bus = Arc::new(ChannelBus::new());
    let mut builder = AppStateBuilder::new(executor(), edge::testing::edge())
        .with_nac(Arc::new(nac))
        .with_collection_mgmt(Arc::new(Collections(collections())))
        .with_event_bus(bus.clone());
    if acp_enabled {
        builder = builder.with_doc_acp(Arc::new(MockDocumentAcpOperations::new()));
    }
    let response = defra_http::create_router_with_state(builder.build())
        .oneshot(
            Request::builder()
                .uri("/api/v0/events?event=update")
                .header(HOST, TEST_HOST)
                .header(AUTHORIZATION, token)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    bus.publish(message);
    bus.publish(update("Plain", SENTINEL));
    let mut body = response.into_body().into_data_stream();
    let bytes = nuclei::time::timeout(Duration::from_secs(2), body.next())
        .await
        .expect("the ungated event must be delivered")
        .unwrap()
        .unwrap();
    let frame = std::str::from_utf8(&bytes).unwrap();
    let data = frame
        .lines()
        .find_map(|line| line.strip_prefix("data: "))
        .expect("SSE data line");
    let json: serde_json::Value = serde_json::from_str(data).unwrap();
    json["data"]["doc_id"].as_str().unwrap().to_string()
}

#[nuclei::test]
async fn a_chunk_event_reaches_only_those_who_can_read_it_with_acp_on() {
    assert_eq!(
        first_delivered(true, update("Users__body_chunk", &readable())).await,
        readable()
    );
    assert_eq!(
        first_delivered(true, update("Notes__body_chunk", &hidden())).await,
        SENTINEL
    );
}

#[nuclei::test]
async fn a_chunk_event_is_still_gated_with_acp_off() {
    assert_eq!(
        first_delivered(false, update("Users__body_chunk", &readable())).await,
        readable()
    );
    assert_eq!(
        first_delivered(false, update("Notes__body_chunk", &hidden())).await,
        SENTINEL
    );
}

#[nuclei::test]
async fn an_ungated_collection_event_needs_no_read_check() {
    for acp_enabled in [false, true] {
        assert_eq!(
            first_delivered(acp_enabled, update("Plain", "plain")).await,
            "plain"
        );
    }
}

#[nuclei::test]
async fn an_event_of_an_unknown_collection_is_not_delivered() {
    let unknown = Message::update(Update::new(
        "unknown".to_string(),
        cid::Cid::default(),
        "collection-unknown".to_string(),
        Vec::new(),
        false,
        false,
    ));
    for acp_enabled in [false, true] {
        assert_eq!(
            first_delivered(acp_enabled, unknown.clone()).await,
            SENTINEL
        );
    }
}
