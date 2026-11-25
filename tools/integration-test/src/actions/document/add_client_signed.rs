use std::time::Duration;

use crate::client_type::{network_edge, ClientType};
use crate::clients::{CliRun, HttpRun};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use ::identity::{Identity as _, KeyType, RawIdentity};
use db::block::builder::{ClientCreateSignatures, VerifiedClientCreate};
use defra_core::signing::{SigningConfig, SigningKeyType};
use defra_node::{QueryRequest, QueryResponse, SignerCredential};
use schema::CollectionVersion;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Adds one document that the client signs itself, with an Ed25519 key of its
/// own, over the create graph it builds from the collection's schema and the
/// document (`ClientCreateSignatures`). Its result is the committed `docID`
/// and the `precomputedDocID` the graph named before anything was sent.
/// In-process it stages the verified graph in a node-owned transaction and
/// commits it; over rust-http it runs the client-signing conversation. A Go
/// node and the CLI have no client-signed create.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddDocClientSigned {
    pub collection: String,
    #[serde(rename = "document")]
    pub doc_map: Value,
    /// Names the signer's Ed25519 key: the same seed is the same key.
    #[serde(default)]
    pub signer_seed: u64,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    /// The stable code the error must carry; see [`Op::expected_error_code`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error_code: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for AddDocClientSigned {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn expected_error_code(&self) -> Option<&str> {
        self.expected_error_code.as_deref()
    }

    fn run(&self, state: &State) -> Result<Value, Failure> {
        let node = state.node_on(self.common.node())?;
        let signer = Signer::new(self.signer_seed)?;
        let schema = self.schema(&node)?;
        let graph = self.graph(&schema, &signer)?;
        let precomputed = graph.doc_id().to_owned();
        let crdts = crdt::Crdts::default();
        let verified =
            VerifiedClientCreate::verify(&crdts, &schema, &self.doc_map, graph.evidence())
                .map_err(|e| Failure::operation(e.to_string()))?;
        let request = self.request().with_identity(Some(signer.did()?));
        let response = state.block_on(async {
            let txn = node
                .begin_transaction(false)
                .await
                .map_err(|e| Failure::operation(e.to_string()))?;
            let response = node
                .execute_verified_create_in_txn(request, &txn, &verified, SignerCredential::None)
                .await;
            if response.has_errors() {
                let _ = node.discard_transaction(&txn).await;
                return Ok(response);
            }
            node.commit_transaction(&txn)
                .await
                .map_err(|e| Failure::operation(e.to_string()))?;
            Ok::<_, Failure>(response)
        })?;
        self.result(response, precomputed)
    }
}

impl AddDocClientSigned {
    /// The collection's schema, which the client trusts as its own.
    fn schema(&self, node: &defra_node::Node) -> Result<CollectionVersion, Failure> {
        node.get_collection(&self.collection)
            .map_err(|e| Failure::operation(e.to_string()))?
            .ok_or_else(|| Failure::operation(format!("no collection {}", self.collection)))
    }

    fn graph(
        &self,
        schema: &CollectionVersion,
        signer: &Signer,
    ) -> Result<ClientCreateSignatures, Failure> {
        ClientCreateSignatures::from_input(
            &crdt::Crdts::default(),
            schema,
            &self.doc_map,
            &signer.config,
        )
        .map_err(|e| Failure::operation(e.to_string()))
    }

    /// The create the graph signs, selecting the unaliased `_docID`.
    fn request(&self) -> QueryRequest {
        let name = &self.collection;
        QueryRequest::new(format!(
            "mutation($input: {name}MutationInputArg!) {{ create_{name}(input: $input) {{ _docID }} }}"
        ))
        .with_variables(json!({ "input": self.doc_map }))
    }

    /// The committed `_docID` beside the precomputed one, or the response's
    /// first error with its code.
    fn result(&self, response: QueryResponse, precomputed: String) -> Result<Value, Failure> {
        if let Some(error) = response.errors.first() {
            return Err(Failure::coded(error.message.clone(), error.code()));
        }
        let committed = response
            .data
            .as_ref()
            .and_then(|data| data[format!("add_{}", self.collection)][0]["_docID"].as_str())
            .map(str::to_owned)
            .ok_or_else(|| Failure::operation(format!("no _docID in {:?}", response.data)))?;
        Ok(json!({ "docID": committed, "precomputedDocID": precomputed }))
    }
}

/// The client's own Ed25519 key, named by its seed.
struct Signer {
    identity: RawIdentity,
    config: SigningConfig,
}

impl Signer {
    fn new(seed: u64) -> Result<Self, Failure> {
        let mut key = [0x5c; 32];
        key[24..].copy_from_slice(&seed.to_be_bytes());
        let key = crypto::keys::ed25519::ed25519_key_from_seed(&key)
            .map_err(|e| Failure::operation(e.to_string()))?;
        let identity = RawIdentity::from_bytes(KeyType::Ed25519, &key)
            .map_err(|e| Failure::operation(e.to_string()))?;
        let public = identity.public_key_bytes();
        let config = SigningConfig {
            key_type: SigningKeyType::Ed25519,
            private_key_bytes: identity.private_key_bytes(),
            public_key_hex: hex::encode(&public),
            public_key_bytes: public,
            remote_signer: None,
            signing_authorization: None,
        };
        Ok(Self { identity, config })
    }

    fn did(&self) -> Result<::identity::Did, Failure> {
        self.identity
            .did()
            .map_err(|e| Failure::operation(e.to_string()))
    }

    /// A bearer token for the node at `address` (`host:port`).
    fn authorization(&self, address: &str) -> Result<String, Failure> {
        let token = ::identity::new_token(
            &self.identity,
            Duration::from_secs(300),
            Some(address.to_owned()),
            None,
        )
        .map_err(|e| Failure::operation(e.to_string()))?;
        let token = String::from_utf8(token).map_err(|e| Failure::operation(e.to_string()))?;
        Ok(format!("Bearer {token}"))
    }
}

impl Action for AddDocClientSigned {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

impl HttpRun for AddDocClientSigned {
    fn execute_http(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |action, state| {
            if state.client_type() == ClientType::GoHttp {
                return Err(Failure::unsupported(format!(
                    "{} on a Go node: Go has no client-signed create",
                    Self::name()
                )));
            }
            let name = action.common.node();
            let node = state
                .node(name)?
                .embedded()
                .cloned()
                .ok_or_else(|| Failure::operation(format!("node {name} is not a Rust node")))?;
            let signer = Signer::new(action.signer_seed)?;
            let schema = action.schema(&node)?;
            let mut graph = action.graph(&schema, &signer)?;
            let precomputed = graph.doc_id().to_owned();
            let url = state.url_on(name)?;
            let address = url.trim_start_matches("http://").to_owned();
            let authorization = signer.authorization(&address)?;
            let socket = format!("ws://{address}/api/v0/graphql/client-sign");
            let request = action.request();
            let outcome = state
                .block_on(network_edge().spawn(async move {
                    defra_http::client_signed_create::execute(
                        &socket,
                        &authorization,
                        request,
                        None,
                        &mut graph,
                    )
                    .await
                    .map_err(|e| e.to_string())
                }))
                .map_err(|e| Failure::operation(format!("run on the network edge: {e:?}")))?;
            let response = outcome.map_err(Failure::operation)?;
            action.result(response, precomputed)
        });
    }
}

impl CliRun for AddDocClientSigned {
    fn execute_cli(&self, state: &mut State) {
        crate::scenario::execute_with(self, state, |_, _| {
            Err(Failure::unsupported(format!(
                "{}: the CLI has no client-signed create",
                Self::name()
            )))
        });
    }
}
