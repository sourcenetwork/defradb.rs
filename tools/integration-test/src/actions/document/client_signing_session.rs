use std::time::Duration;

use async_tungstenite::tungstenite::Message;
use crypto::{Key as _, PrivateKey as _};
use futures::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use crate::client_type::{network_edge, ClientType};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// How the client breaks the signing conversation, if it does.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SigningMisstep {
    /// Answers the first wave, then sends the same answer again.
    AnswerTwice,
    /// Sends an answer for wave 9 before the server asks for any wave.
    UnrequestedWave,
    /// Answers the first wave with a signature over a block it was not asked to sign.
    ForeignBlock,
}

/// Creates one document over the HTTP client-signing WebSocket, signing every
/// block the server asks for with a fresh Ed25519 identity, and breaking the
/// conversation as `misstep` says. Only the Rust HTTP server serves the route.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientSigningSession {
    pub request: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub misstep: Option<SigningMisstep>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for ClientSigningSession {
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
        if state.client_type() != ClientType::RustHttp {
            return Err(Failure::unsupported(format!(
                "{} under {:?}",
                Self::name(),
                state.client_type()
            )));
        }
        let url = state.url_on(self.common.node())?;
        let request = self.request.clone();
        let misstep = self.misstep;
        let response = state
            .block_on(network_edge().spawn(converse(url, request, misstep)))
            .map_err(|error| Failure::operation(error.to_string()))?
            .map_err(Failure::operation)?;
        let errors = response["errors"].as_array().cloned().unwrap_or_default();
        if !errors.is_empty() {
            let messages = errors
                .iter()
                .map(|error| error["message"].as_str().unwrap_or_default())
                .collect::<Vec<_>>();
            return Err(Failure::operation(messages.join("; ")));
        }
        Ok(response["data"].clone())
    }
}

async fn converse(
    url: String,
    request: String,
    misstep: Option<SigningMisstep>,
) -> Result<Value, String> {
    let private = crypto::generate_ed25519().map_err(|error| error.to_string())?;
    let public_key_hex = hex::encode(private.public_key().raw());
    let identity = identity::RawIdentity::from_bytes(crypto::KeyType::Ed25519, private.raw())
        .map_err(|error| error.to_string())?;
    let audience = url
        .strip_prefix("http://")
        .unwrap_or(&url)
        .trim_end_matches('/')
        .to_owned();
    let token = identity::new_token(&identity, Duration::from_secs(60), Some(audience), None)
        .map_err(|error| error.to_string())?;
    let token = String::from_utf8(token).map_err(|error| error.to_string())?;
    let endpoint = format!(
        "ws://{}/api/v0/graphql/client-sign",
        url.strip_prefix("http://").unwrap_or(&url)
    );
    let handshake =
        defra_http::client_signed_create::websocket_request(&endpoint, &format!("Bearer {token}"))
            .map_err(|error| error.to_string())?;
    let stream = net::connect(handshake.uri())
        .await
        .map_err(|error| error.to_string())?;
    let config = defra_http::client_signed_create::websocket_config();
    let (mut socket, _) =
        async_tungstenite::client_async_with_config(handshake, stream, Some(config))
            .await
            .map_err(|error| error.to_string())?;
    let mut opening = vec![json!({"query": request, "public_key_hex": public_key_hex})];
    if misstep == Some(SigningMisstep::UnrequestedWave) {
        opening.push(json!({"wave": 9, "signatures": []}));
    }
    for value in opening {
        socket
            .send(Message::Text(value.to_string().into()))
            .await
            .map_err(|error| error.to_string())?;
    }
    loop {
        let message = nuclei::time::timeout(Duration::from_secs(30), socket.next())
            .await
            .map_err(|_| "the signing session went silent".to_owned())?
            .ok_or_else(|| "the server closed the signing session".to_owned())?
            .map_err(|error| error.to_string())?;
        let Message::Text(text) = message else {
            continue;
        };
        let value: Value = serde_json::from_str(&text).map_err(|error| error.to_string())?;
        if let Some(response) = value.get("response") {
            return Ok(response.clone());
        }
        let wave = value["wave"]
            .as_u64()
            .ok_or_else(|| format!("unexpected signing message {value}"))?;
        let signatures = value["blocks"]
            .as_array()
            .ok_or_else(|| format!("unexpected signing message {value}"))?
            .iter()
            .map(|block| {
                let block = hex::decode(block.as_str().unwrap_or_default())
                    .map_err(|error| error.to_string())?;
                let signed = match misstep {
                    Some(SigningMisstep::ForeignBlock) if wave == 1 => {
                        b"a block outside the approved graph".to_vec()
                    }
                    _ => block,
                };
                private
                    .sign(&signed)
                    .map(hex::encode)
                    .map_err(|error| error.to_string())
            })
            .collect::<Result<Vec<_>, String>>()?;
        let answer = json!({"wave": wave, "signatures": signatures}).to_string();
        let times = match misstep {
            Some(SigningMisstep::AnswerTwice) if wave == 1 => 2,
            _ => 1,
        };
        for _ in 0..times {
            socket
                .send(Message::Text(answer.clone().into()))
                .await
                .map_err(|error| error.to_string())?;
        }
    }
}

impl Action for ClientSigningSession {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
