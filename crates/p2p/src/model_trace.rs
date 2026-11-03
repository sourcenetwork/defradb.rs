//! Abstract delivery events for trace validation against
//! `proofs/tla/P2PDelivery.tla`.
//!
//! Each event witnesses one action of the model and is logged under the
//! [`TARGET`] log target as one JSON object. The node that logs an event is
//! the node that acts; peers are named by peer ID and heads by CID. Events are
//! recorded only when the crate is built with the `model-trace` feature;
//! without it [`model_trace!`](crate::model_trace!) compiles to nothing.
//! `proofs/tla/Trace_P2PDelivery.tla` replays a recorded run through TLC.

use serde::Serialize;

/// Whether this build records events.
pub const ENABLED: bool = cfg!(feature = "model-trace");

/// The log target every event is written under.
pub const TARGET: &str = "model_trace";

/// One model step, named after the `P2PDelivery` action it witnesses.
#[derive(Debug, Serialize)]
#[serde(tag = "ev")]
pub enum Event<'a> {
    /// A local write committed head `head` of `doc` on top of `parents`.
    Commit {
        doc: &'a str,
        head: String,
        parents: Vec<String>,
    },
    /// The sender's retry marker for `head` toward `receiver` is durable.
    Register {
        doc: &'a str,
        receiver: &'a str,
        head: String,
    },
    /// The recorder returned no marker; `dropped` are the peers whose
    /// registration the failure abandoned.
    RegFail {
        doc: &'a str,
        receiver: &'a str,
        head: String,
        dropped: Vec<&'a str>,
    },
    /// The receiver decided a pushed head.
    Admit {
        doc: &'a str,
        sender: Option<&'a str>,
        head: String,
        outcome: Outcome,
        superseded: Vec<String>,
    },
    /// The sender acted on the receiver's acknowledgement of `head`.
    Ack {
        doc: &'a str,
        receiver: &'a str,
        head: String,
    },
    /// The dispatch clock reserved a fetch slot for a due root.
    Schedule {
        doc: &'a str,
        head: String,
        mode: SlotMode,
    },
    /// A dispatched fetch took its global and per-provider permits.
    Acquire { head: String },
    /// A fetch window brought blocks without completing the DAG.
    Progress { head: String },
    /// The receiver merged `head`, which discharges its record and writes
    /// its forward intents.
    Merge { doc: &'a str, head: String },
    /// A fetch attempt ended with nothing.
    Stall { head: String, end: StallEnd },
    /// A dispatch that kept its slot started its next attempt.
    Retry { head: String },
    /// A dispatch that kept its slot ran out of attempts.
    GiveUp { head: String },
    /// A quantum cut a productive attempt.
    Preempt { head: String },
    /// A root's backoff ran out.
    BecomeDue { head: String },
    /// The resync sweep gave a durable record a place in memory.
    Resync { head: String },
    /// The pending TTL dropped a root from memory; `removed` says whether
    /// its durable record went too.
    Expire { head: String, removed: bool },
    /// An advisory pull fetch took permits.
    PullStart { provider: &'a str },
    /// An advisory pull fetch released its permits.
    PullEnd { provider: &'a str },
    /// The clock found the merge loop's channel full.
    FloodOn,
    /// The clock found room in the merge loop's channel again.
    FloodOff,
    /// A local merge continuation took a place in the pending map.
    StartCont,
    /// A local merge continuation left the pending map.
    EndCont,
    /// The node started; every earlier start of the same node ended.
    Crash { node: &'a str },
    /// The node restored its replicator list.
    RestoreReplicators,
    /// The last connection to `peer` closed.
    Disconnect { peer: String },
    /// A connection to `peer` came up.
    Reconnect { peer: String, mode: ConnMode },
}

/// How the receiver decided a pushed head.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Outcome {
    Merged,
    Covered,
    Registered,
    Nacked,
}

/// Whether a scheduled root's permits were free when its slot was reserved.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SlotMode {
    Run,
    Wait,
}

/// Whether a failed attempt kept its fetch slot.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum StallEnd {
    Hold,
    Backoff,
}

/// Which sides of a new connection serve requests.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ConnMode {
    /// Adopted on both sides.
    Full,
    /// Dialed by this node for its own requests and not adopted.
    Dialer,
}

/// The heads a block's DAG sits on, for [`Event::Commit`].
pub fn parents_of(block: &[u8]) -> Vec<String> {
    defra_core::Block::from_dag_cbor(block)
        .ok()
        .and_then(|block| block.heads)
        .unwrap_or_default()
        .iter()
        .map(ToString::to_string)
        .collect()
}

#[derive(Serialize)]
struct Record<'a> {
    mt: u64,
    #[serde(flatten)]
    event: Event<'a>,
}

/// Logs one event with the wall-clock time it happened at, which orders it
/// against the other nodes' events.
pub fn emit(event: Event<'_>) {
    let mt = web_time::SystemTime::now()
        .duration_since(web_time::UNIX_EPOCH)
        .map_or(0, |since| {
            u64::try_from(since.as_nanos()).unwrap_or(u64::MAX)
        });
    if let Ok(line) = serde_json::to_string(&Record { mt, event }) {
        tracing::info!(target: TARGET, "{line}");
    }
}

/// Records a [`Event`] variant when the `model-trace` feature is on, and
/// compiles to nothing otherwise.
#[macro_export]
macro_rules! model_trace {
    ($($event:tt)+) => {
        if $crate::model_trace::ENABLED {
            $crate::model_trace::emit($crate::model_trace::Event::$($event)+);
        }
    };
}
