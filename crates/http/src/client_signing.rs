use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use async_tungstenite::tungstenite::{handshake::derive_accept_key, protocol::Role, Message};
use async_tungstenite::WebSocketStream;
use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use base64::Engine;
use defra_core::signing::{SigningConfig, SigningKeyType};
use futures::{FutureExt, StreamExt};
use identity::Identity as _;
use query::executor::{QueryRequest, QueryResponse};
use serde::Deserialize;
use serde_json::json;

use crate::client_signer::{SessionSigner, Wave};
use crate::error::HttpError;
use crate::identity_extractor::{ExtractIdentity, ExtractTokenIdentity};
use crate::nac_guard::require_permission;
use crate::router::{AppState, NodePermission};

const MAX_MESSAGE: usize = 1024 * 1024;
const SESSION_TIMEOUT: Duration = Duration::from_secs(90);

type Socket = WebSocketStream<net::rt::FromHyper<hyper::upgrade::Upgraded>>;

pub(crate) fn session_lifetime(now: Duration, expiration: u64) -> Option<Duration> {
    let remaining = Duration::from_secs(expiration).checked_sub(now)?;
    (!remaining.is_zero()).then_some(remaining.min(SESSION_TIMEOUT))
}

fn current_lifetime(expiration: u64) -> Option<Duration> {
    session_lifetime(
        SystemTime::now().duration_since(UNIX_EPOCH).ok()?,
        expiration,
    )
}

pub(crate) fn try_session_slot(
    slots: &Arc<nuclei::sync::Semaphore>,
) -> Result<nuclei::sync::OwnedSemaphorePermit, nuclei::sync::TryAcquireError> {
    slots.clone().try_acquire_owned()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    query: String,
    #[serde(default, rename = "operationName")]
    operation_name: Option<String>,
    #[serde(default)]
    variables: Option<serde_json::Value>,
    #[serde(default)]
    txn_id: Option<String>,
    public_key_hex: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    signatures: Vec<String>,
}

fn has_token(value: Option<&axum::http::HeaderValue>, token: &str) -> bool {
    value.and_then(|v| v.to_str().ok()).is_some_and(|v| {
        v.split(',')
            .any(|part| part.trim().eq_ignore_ascii_case(token))
    })
}

pub(crate) async fn upgrade(
    State(state): State<AppState>,
    identity: ExtractIdentity,
    token: ExtractTokenIdentity,
    mut request: Request<Body>,
) -> Result<Response, HttpError> {
    if identity.did().is_none() {
        return Err(HttpError::Unauthorized(
            "client signing requires an authenticated DID".into(),
        ));
    }
    let token = token
        .identity()
        .ok_or_else(|| HttpError::Unauthorized("client signing requires a bearer".into()))?;
    if token.did().ok().as_ref() != identity.did() {
        return Err(HttpError::Unauthorized("bearer identity mismatch".into()));
    }
    let expiration = token.claims.exp;
    let lifetime = current_lifetime(expiration)
        .ok_or_else(|| HttpError::Unauthorized("bearer expired".into()))?;
    require_permission(&state, &identity, NodePermission::DocumentUpdate).await?;
    let headers = request.headers();
    if !has_token(headers.get(header::CONNECTION), "upgrade")
        || !has_token(headers.get(header::UPGRADE), "websocket")
        || headers
            .get(header::SEC_WEBSOCKET_VERSION)
            .and_then(|v| v.to_str().ok())
            != Some("13")
    {
        return Err(HttpError::BadRequest(
            "a WebSocket upgrade is required".into(),
        ));
    }
    let key = headers
        .get(header::SEC_WEBSOCKET_KEY)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| HttpError::BadRequest("missing WebSocket key".into()))?;
    if base64::engine::general_purpose::STANDARD
        .decode(key)
        .map_or(true, |key| key.len() != 16)
    {
        return Err(HttpError::BadRequest("invalid WebSocket key".into()));
    }
    let accept = derive_accept_key(key.as_bytes());
    let permit = try_session_slot(&state.signing_slots)
        .map_err(|_| HttpError::ServiceUnavailable("client signing capacity reached".into()))?;
    let upgraded = hyper::upgrade::on(&mut request);
    let edge = state.edge.clone();
    let _task = edge
        .try_spawn(async move {
            let _permit = permit;
            if let Ok(Ok(stream)) = nuclei::time::timeout(lifetime, upgraded).await {
                let mut config =
                    async_tungstenite::tungstenite::protocol::WebSocketConfig::default();
                config.max_message_size = Some(MAX_MESSAGE);
                config.max_frame_size = Some(MAX_MESSAGE);
                let socket = WebSocketStream::from_raw_socket(
                    net::rt::FromHyper::new(stream),
                    Role::Server,
                    Some(config),
                )
                .await;
                if let Some(remaining) = current_lifetime(expiration) {
                    let _ =
                        nuclei::time::timeout(remaining, run(socket, state, identity, expiration))
                            .await;
                }
            }
        })
        .map_err(|_| HttpError::ServiceUnavailable("network edge is shutting down".into()))?;
    Ok((
        StatusCode::SWITCHING_PROTOCOLS,
        [
            (header::CONNECTION, "upgrade"),
            (header::UPGRADE, "websocket"),
            (header::SEC_WEBSOCKET_ACCEPT, accept.as_str()),
        ],
    )
        .into_response())
}

async fn next_json<T>(socket: &mut Socket) -> Option<T>
where
    T: serde::de::DeserializeOwned,
{
    let message = socket.next().await?.ok()?;
    let Message::Text(text) = message else {
        return None;
    };
    serde_json::from_str(&text).ok()
}

fn accepted_mutation(start: &Start, limits: query::QueryLimits) -> bool {
    let variables = match &start.variables {
        Some(serde_json::Value::Object(map)) => {
            Some(map.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
        }
        None => None,
        _ => return false,
    };
    let Ok(query::query_parse::ParsedOperation::Mutation {
        mutations,
        explain: None,
    }) = query::query_parse::parse_request_with_limits(
        &start.query,
        variables.as_ref(),
        start.operation_name.as_deref(),
        limits,
    )
    else {
        return false;
    };
    mutations.len() == 1
        && mutations[0].encryption().is_none()
        && match mutations[0].mutation_type {
            query::MutationType::Add => mutations[0].create_input.len() == 1,
            query::MutationType::Update => {
                crate::client_signed_update::single_document_target(&mutations[0]).is_some()
                    && !mutations[0].update_input.is_empty()
            }
            query::MutationType::Delete => {
                crate::client_signed_delete::single_document_target(&mutations[0]).is_some()
            }
            _ => false,
        }
}

async fn run(mut socket: Socket, state: AppState, identity: ExtractIdentity, expiration: u64) {
    let Some(start) = next_json::<Start>(&mut socket).await else {
        return;
    };
    if current_lifetime(expiration).is_none() || !accepted_mutation(&start, state.query_limits) {
        return;
    }
    let Ok(public) =
        crypto::public_key_from_string(crypto::KeyType::Ed25519, &start.public_key_hex)
    else {
        return;
    };
    if public.did().ok().as_deref() != identity.did().map(|did| did.as_str()) {
        return;
    }
    let txn = match start.txn_id.as_deref().map(str::parse).transpose() {
        Ok(txn) => txn,
        Err(_) => return,
    };
    let Ok((signer, mut waves)) = SessionSigner::new(&start.public_key_hex) else {
        return;
    };
    let signing = SigningConfig {
        key_type: SigningKeyType::Ed25519,
        private_key_bytes: Vec::new(),
        public_key_bytes: public.raw_owned(),
        public_key_hex: start.public_key_hex,
        remote_signer: Some(Arc::new(signer)),
        signing_authorization: None,
    };
    let mut request = QueryRequest::new(start.query);
    request.operation_name = start.operation_name;
    request.variables = start.variables;
    request.identity = identity.did().cloned();
    let execution = crate::query_context::execute_with_client_signer(
        &state, &identity, request, signing, txn, expiration,
    )
    .fuse();
    futures::pin_mut!(execution);
    enum Event {
        Done(QueryResponse),
        Wave(Option<Wave>),
        Closed,
    }
    loop {
        if current_lifetime(expiration).is_none() {
            return;
        }
        let event = {
            let wave = waves.recv().fuse();
            let incoming = socket.next().fuse();
            futures::pin_mut!(wave, incoming);
            futures::select! {
                result = execution => Event::Done(result),
                request = wave => Event::Wave(request),
                _message = incoming => Event::Closed,
            }
        };
        match event {
            Event::Done(result) => {
                let _ = socket
                    .send(Message::Text(
                        json!({"response": result}).to_string().into(),
                    ))
                    .await;
                return;
            }
            Event::Wave(Some(wave)) => {
                if current_lifetime(expiration).is_none() {
                    return;
                }
                let payloads = wave.payloads.iter().map(hex::encode).collect::<Vec<_>>();
                if socket
                    .send(Message::Text(
                        json!({"blocks": payloads}).to_string().into(),
                    ))
                    .await
                    .is_err()
                {
                    return;
                }
                let Some(answer) = next_json::<Answer>(&mut socket).await else {
                    return;
                };
                let Ok(signatures) = answer
                    .signatures
                    .iter()
                    .map(hex::decode)
                    .collect::<Result<Vec<_>, _>>()
                else {
                    return;
                };
                if current_lifetime(expiration).is_none() || wave.answer.send(signatures).is_err() {
                    return;
                }
            }
            Event::Wave(None) | Event::Closed => return,
        }
    }
}
