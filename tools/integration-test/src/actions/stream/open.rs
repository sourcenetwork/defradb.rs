use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use super::{address, in_process, peer_id, KeptStream, ProtocolSpec, STARTED};
use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// How long an opener waits on a stream before calling it stuck.
const STREAM_TIMEOUT: Duration = Duration::from_secs(10);

/// What the opener does once its request is written.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadMode {
    /// Finish the request and read the whole reply.
    #[default]
    All,
    /// Read until the handler says `started`, then keep the stream open.
    Started,
    /// Keep writing until the serving node stops the stream, and fail with why.
    UntilStopped,
}

/// Opens a stream protocol from this node to `target`, writes the request,
/// and reads as `read` says. Answers `{"reply": …}`: the reply as JSON when
/// it parses, else as text, with the opener's own peer ID shown as
/// `"opener"`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenStream {
    pub target: String,
    pub protocol: ProtocolSpec,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub request: String,
    /// Send this many bytes as the request instead, sized by the protocol's limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_size: Option<super::Size>,
    /// Dial `target` by its peer ID alone, never its address: the stream
    /// can only ride a connection the two nodes already share.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub by_id: bool,
    #[serde(default)]
    pub read: ReadMode,
    /// Keep the stream open under this name (with `read: started`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep: Option<String>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for OpenStream {
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
        let opener = in_process(state, self.common.node(), Self::name())?;
        let target = in_process(state, &self.target, Self::name())?;
        let protocol = self.protocol.protocol()?;
        let peer = match self.by_id {
            true => peer_id(state, &target)?,
            false => address(state, &target)?,
        };
        let request = match self.request_size {
            None => self.request.clone().into_bytes(),
            Some(super::Size::Bytes(n)) => vec![7; n],
            Some(super::Size::Share) => vec![7; protocol.max_message],
            Some(super::Size::AboveShare) => vec![7; protocol.max_message + 1],
        };
        let failed = |e: String| Failure::operation(format!("stream failed: {e}"));
        let (mut send, mut recv) = state
            .block_on(workers::timeout(
                STREAM_TIMEOUT,
                opener.open_p2p_stream(&peer, &protocol, request),
            ))
            .map_err(|_| failed("timed out opening".into()))?
            .map_err(|e| failed(e.to_string()))?;
        let reply = match self.read {
            ReadMode::All => {
                send.finish().map_err(|e| failed(e.to_string()))?;
                state
                    .block_on(workers::timeout(STREAM_TIMEOUT, recv.read_to_end(1 << 20)))
                    .map_err(|_| failed("timed out reading".into()))?
                    .map_err(|e| failed(e.to_string()))?
            }
            ReadMode::Started => {
                let mut started = vec![0; STARTED.len()];
                let mut filled = 0;
                while filled < started.len() {
                    let read = state
                        .block_on(workers::timeout(
                            STREAM_TIMEOUT,
                            recv.read(&mut started[filled..]),
                        ))
                        .map_err(|_| failed("timed out waiting to start".into()))?
                        .map_err(|e| failed(e.to_string()))?;
                    match read {
                        Some(n) => filled += n,
                        None => return Err(failed("ended before it started".into())),
                    }
                }
                if let Some(name) = &self.keep {
                    state.streams.keep(name, KeptStream { send, recv });
                }
                started
            }
            ReadMode::UntilStopped => {
                let deadline = std::time::Instant::now() + STREAM_TIMEOUT;
                loop {
                    if let Err(e) = state.block_on(send.write_all(b"more")) {
                        return Err(failed(e.to_string()));
                    }
                    if std::time::Instant::now() > deadline {
                        return Err(failed("never stopped".into()));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
            }
        };
        let own = peer_id(state, &opener)?;
        let text = String::from_utf8_lossy(&reply).replace(&own, "opener");
        let reply = serde_json::from_str(&text).unwrap_or(Value::String(text));
        Ok(json!({ "reply": reply }))
    }
}

impl Action for OpenStream {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
