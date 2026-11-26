#![cfg(feature = "p2p")]

//! A write validator answers once per mutation: request preparation keeps its
//! approval, and the mutation's first attempt finds it instead of ending on a
//! miss and asking again (#3466).

use std::net::{IpAddr, Ipv4Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[path = "support/outcome.rs"]
mod outcome;

use async_trait::async_trait;
use defra_node::{AccessHooks, Node, NodeBuilder, P2PConfig};
use outcome::Result;
use query::access_hooks::{WriteRequest, WriteValidator};
use serde_json::Value as JsonValue;

const SDL: &str = "type Note { body: String }";

#[derive(Default)]
struct CountingValidator {
    calls: AtomicUsize,
}

#[async_trait]
impl WriteValidator for CountingValidator {
    async fn validate_write(&self, request: &WriteRequest<'_>) -> Result<(), String> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let empty = request
            .create_input
            .iter()
            .chain(std::iter::once(request.update_input))
            .any(|input| input.get("body") == Some(&JsonValue::String(String::new())));
        if empty {
            return Err("a note needs a body".to_string());
        }
        Ok(())
    }
}

/// Validator calls and input waves (attempts that ended on a miss) one
/// mutation cost.
async fn cost(
    node: &Node,
    validator: &CountingValidator,
    mutation: &str,
) -> (usize, u64, JsonValue) {
    let waves = &query::prepared::input_wait_stats().waves;
    let (calls_before, waves_before) = (
        validator.calls.load(Ordering::SeqCst),
        waves.load(Ordering::SeqCst),
    );
    let response = node.execute(mutation).await;
    let outcome = match response.data {
        Some(data) if response.errors.is_empty() => data,
        _ => JsonValue::String(format!("{:?}", response.errors)),
    };
    (
        validator.calls.load(Ordering::SeqCst) - calls_before,
        waves.load(Ordering::SeqCst) - waves_before,
        outcome,
    )
}

#[test]
#[mantra_macros::req_test("defra.write_validator.once")]
fn a_write_validator_is_asked_once_per_mutation() -> Result<()> {
    edge::testing::edge()
        .block_on(async {
            let validator = Arc::new(CountingValidator::default());
            let node = NodeBuilder::default()
                .with_p2p(iroh_config())
                .with_access_hooks(
                    AccessHooks::new(Vec::<String>::new()).with_write_validator(validator.clone()),
                )
                .build()
                .await?;
            node.add_collection(&defra_core::RequestContext::ambient(), SDL)
                .await?;

            let mut costs = Vec::new();
            let (calls, waves, created) = cost(
                &node,
                &validator,
                r#"mutation { add_Note(input: {body: "first"}) { _docID } }"#,
            )
            .await;
            costs.push(("add", calls, waves));
            let doc_id = created["add_Note"][0]["_docID"]
                .as_str()
                .unwrap_or_else(|| panic!("add: {created}"))
                .to_string();

            let mutations = [
                (
                    "update by DocID",
                    format!(
                        r#"mutation {{ update_Note(docID: "{doc_id}", input: {{body: "second"}}) {{ body }} }}"#
                    ),
                ),
                (
                    "update by filter",
                    r#"mutation { update_Note(filter: {body: {_eq: "second"}}, input: {body: "third"}) { body } }"#
                        .to_string(),
                ),
                (
                    "delete",
                    format!(r#"mutation {{ delete_Note(docID: "{doc_id}") {{ _docID }} }}"#),
                ),
            ];
            for (name, mutation) in &mutations {
                let (calls, waves, outcome) = cost(&node, &validator, mutation).await;
                assert!(!outcome.is_string(), "{name}: {outcome}");
                costs.push((name, calls, waves));
            }

            let (calls, waves, refused) = cost(
                &node,
                &validator,
                r#"mutation { add_Note(input: {body: ""}) { _docID } }"#,
            )
            .await;
            assert!(refused.to_string().contains("a note needs a body"), "{refused}");
            costs.push(("refused add", calls, waves));

            eprintln!("(mutation, validator calls, input waves): {costs:?}");
            assert!(
                costs.iter().all(|(_, calls, waves)| (*calls, *waves) == (1, 0)),
                "{costs:?}"
            );

            node.shutdown().await.unwrap();
            Ok(())
        })
        .unwrap()
}

fn iroh_config() -> P2PConfig {
    P2PConfig {
        peer_key: defra_node::PeerKeySource::Store,
        bind_addr: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
        port: Some(0),
        relay_mode: p2p::iroh::IrohRelayModeConfig::Disabled,
        discovery: p2p::iroh::IrohDiscoveryConfig::Disabled,
        ..Default::default()
    }
}
