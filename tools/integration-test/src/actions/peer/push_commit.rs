use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use testo::prelude::*;

use crate::actions::peer::support::addresses_of;
use crate::hostile::blocks::{author, genesis, tampered_field, Fields, Signing};
use crate::hostile::peer::HostilePeer;
use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use document::NormalValue;

/// One integer field's value swapped after the commit's CIDs are taken, so
/// that field's block no longer hashes to the CID it is pushed under.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tamper {
    pub field: String,
    pub value: i64,
}

/// Pushes a new document to `target` the way a replicator does, from a peer
/// that is not a defra node: an in-process iroh peer with its own key builds
/// a genesis of `fields` for the collection `collection_id` at version
/// `version_id`, signs its composite, and sends each block in its own
/// PushLog, children first. The target must be an iroh node. Fails with the
/// first refusal the target answers.
///
/// With `rootOnly` it sends the signed composite alone, so the target holds a
/// root whose links it lacks and keeps a pending fetch for the rest. The
/// result names the pusher's own peer ID as `peerID`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PushCommit {
    pub target: String,
    pub collection_id: Text,
    pub version_id: Text,
    /// String and integer field values, by field name.
    pub fields: Map<String, Value>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub root_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tamper: Option<Tamper>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl PushCommit {
    fn fields(&self) -> Result<Fields, Failure> {
        let mut fields = Vec::with_capacity(self.fields.len());
        for (name, value) in &self.fields {
            let value = match value {
                Value::String(text) => NormalValue::String(text.clone()),
                Value::Number(number) => match number.as_i64() {
                    Some(int) => NormalValue::Int(int),
                    None => return Err(unsupported_value(name, value)),
                },
                _ => return Err(unsupported_value(name, value)),
            };
            fields.push((name.clone(), value));
        }
        Ok(Fields::new(fields))
    }
}

fn unsupported_value(name: &str, value: &Value) -> Failure {
    Failure::unsupported(format!("PushCommit field {name} value {value}"))
}

impl Op for PushCommit {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let addresses = addresses_of(state, &[self.target.as_str()])?;
        let mut fragment = genesis(
            &self.fields()?,
            self.version_id.as_str(),
            &author(0x11),
            Signing::CompositeOnly,
        );
        if let Some(tamper) = &self.tamper {
            if !self.fields.contains_key(&tamper.field) {
                return Err(Failure::unsupported(format!(
                    "PushCommit tamper field {} is not one of its fields",
                    tamper.field
                )));
            }
            fragment = tampered_field(&fragment, &tamper.field, tamper.value).0;
        }
        let collection_id = self.collection_id.as_str().to_owned();
        let root_only = self.root_only;
        let edge = edge::testing::edge();
        let (peer_id, refusals) = edge
            .block_on(edge::compat(async move {
                let peer = HostilePeer::dial(&addresses).await;
                let peer_id = peer.peer_id();
                let refusals: Vec<Option<String>> = match root_only {
                    true => vec![peer.push_root(&fragment, &collection_id).await.err_message],
                    false => peer
                        .push(&fragment, &collection_id)
                        .await
                        .into_iter()
                        .map(|(_, reply)| reply.err_message)
                        .collect(),
                };
                (peer_id, refusals)
            }))
            .map_err(|e| Failure::operation(format!("push did not run: {e}")))?;
        match refusals.into_iter().flatten().next() {
            Some(refusal) => Err(Failure::operation(refusal)),
            None => Ok(json!({ "peerID": peer_id })),
        }
    }
}

impl Action for PushCommit {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
