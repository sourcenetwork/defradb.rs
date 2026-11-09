mod common;

use axum::body::Body;
use axum::http::header::{AUTHORIZATION, HOST};
use axum::http::Request;
use defra_http::router::{AppStateBuilder, CollectionManagementOperations};
use defra_http::{MockDocumentAcpOperations, MockNodeAcpOperations, MockQueryExecutor};
use events::{Bus, ChannelBus, Message, Update};
use futures::StreamExt;
use std::sync::Arc;
use std::time::Duration;
use tower::ServiceExt;

use common::{NodePermission, StatusCode, TEST_HOST};

const SENTINEL: &str = "sentinel";

/// Serves the given collections by ID. The mock executor answers a query only
/// when its text mentions `users`, so a `Users` chunk collection is readable
/// and a `Notes` one is not.
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
    let mut builder =
        AppStateBuilder::new(Arc::new(MockQueryExecutor::new()), edge::testing::edge())
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
        first_delivered(true, update("Users__body_chunk", "readable")).await,
        "readable"
    );
    assert_eq!(
        first_delivered(true, update("Notes__body_chunk", "hidden")).await,
        SENTINEL
    );
}

#[nuclei::test]
async fn a_chunk_event_is_still_gated_with_acp_off() {
    assert_eq!(
        first_delivered(false, update("Users__body_chunk", "readable")).await,
        "readable"
    );
    assert_eq!(
        first_delivered(false, update("Notes__body_chunk", "hidden")).await,
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
