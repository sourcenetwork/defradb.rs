//! Trace validation for `proofs/tla/P2PDelivery.tla`.
//!
//! A defra binary built with `--features model-trace` logs one JSON event per
//! model action under the `model_trace` log target (`crates/p2p/src/model_trace.rs`).
//! [`read`] collects those events from each node's log and orders them by
//! time; [`Trace::abstracted`] names the nodes and the heads of one document
//! and keeps only what the model follows; [`Trace::module`] and
//! [`Trace::config`] write a TLA+ module and TLC configuration for
//! `Trace_P2PDelivery.tla`; [`replay`] runs TLC on them. A trace that replays
//! to its end is a behaviour of the model under the fixed design's knobs.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::{Map, Value};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("reading {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("{path}:{line}: malformed model_trace event: {source}")]
    Malformed {
        path: PathBuf,
        line: usize,
        source: serde_json::Error,
    },
    #[error("node {0} logged no Crash event: is the binary built with --features model-trace?")]
    NoStart(String),
    #[error("document {0} has no Commit event in the trace")]
    NoCommit(String),
    #[error("head {head} has parent {parent}, which no traced node committed")]
    UnknownParent { head: String, parent: String },
    #[error("running TLC: {0}")]
    Tlc(std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

/// The marker that opens an event's JSON in a log line.
const EVENT_START: &str = "{\"mt\":";

/// One node of a scenario: its name in the model and the log it writes.
#[derive(Debug, Clone)]
pub struct NodeLog {
    pub name: String,
    pub log: PathBuf,
}

/// One logged event, attributed to the node whose log holds it.
#[derive(Debug, Clone)]
pub struct Logged {
    pub mt: u64,
    pub node: String,
    pub fields: Map<String, Value>,
}

impl Logged {
    fn str(&self, key: &str) -> Option<&str> {
        self.fields.get(key).and_then(Value::as_str)
    }

    fn kind(&self) -> &str {
        self.str("ev").unwrap_or_default()
    }
}

/// Every event the nodes logged, in time order; ties keep log order.
pub fn read(nodes: &[NodeLog]) -> Result<Vec<Logged>> {
    let mut events = Vec::new();
    for node in nodes {
        let text = std::fs::read_to_string(&node.log).map_err(|source| Error::Io {
            path: node.log.clone(),
            source,
        })?;
        for (index, line) in text.lines().enumerate() {
            let Some(start) = line.find(EVENT_START) else {
                continue;
            };
            let mut fields: Map<String, Value> =
                serde_json::from_str(&line[start..]).map_err(|source| Error::Malformed {
                    path: node.log.clone(),
                    line: index + 1,
                    source,
                })?;
            let mt = fields.remove("mt").and_then(|mt| mt.as_u64()).unwrap_or(0);
            events.push(Logged {
                mt,
                node: node.name.clone(),
                fields,
            });
        }
    }
    events.sort_by_key(|event| event.mt);
    Ok(events)
}

/// The receiver's limits, as the node's defaults set them
/// (`crates/p2p/src/sync/manager/config.rs`).
#[derive(Debug, Clone, Copy)]
pub struct Bounds {
    /// `max_concurrent_dag_fetches`: fetch slots and global permits.
    pub fetches: usize,
    /// `max_pending_dags`: places in the in-memory pending map.
    pub pending: usize,
}

impl Default for Bounds {
    fn default() -> Self {
        Self {
            fetches: 4,
            pending: 1000,
        }
    }
}

/// One replayed event: the record `Trace_P2PDelivery.tla` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub ev: String,
    pub node: String,
    pub peer: String,
    pub head: String,
    pub outcome: String,
    pub sup: Vec<String>,
    pub mode: String,
}

/// A scenario's trace, abstracted to the model's names.
#[derive(Debug, Clone)]
pub struct Trace {
    pub nodes: Vec<String>,
    pub repl: Vec<(String, String)>,
    /// The document's heads in commit order, with their CIDs.
    pub heads: Vec<(String, String)>,
    pub author: BTreeMap<String, String>,
    pub parents: BTreeMap<String, BTreeSet<String>>,
    pub steps: Vec<Step>,
    pub bounds: Bounds,
}

/// Neither peer nor head.
const NONE: &str = "none";

impl Trace {
    /// Follows document `doc` through the events: names each node by its
    /// role, each head of the document `h1`, `h2`, ... in commit order, and
    /// drops what concerns other documents.
    pub fn abstracted(
        nodes: &[NodeLog],
        repl: &[(&str, &str)],
        doc: &str,
        events: &[Logged],
        bounds: Bounds,
    ) -> Result<Self> {
        let mut peers = BTreeMap::new();
        for event in events.iter().filter(|event| event.kind() == "Crash") {
            if let Some(id) = event.str("node") {
                peers.insert(id.to_string(), event.node.clone());
            }
        }
        for node in nodes {
            if !peers.values().any(|name| *name == node.name) {
                return Err(Error::NoStart(node.name.clone()));
            }
        }

        let mut heads = Vec::new();
        let mut names = BTreeMap::new();
        let mut author = BTreeMap::new();
        let mut parent_cids = BTreeMap::new();
        for event in events.iter().filter(|event| event.kind() == "Commit") {
            let (Some(cid), Some(true)) = (event.str("head"), event.str("doc").map(|d| d == doc))
            else {
                continue;
            };
            if names.contains_key(cid) {
                continue;
            }
            let name = format!("h{}", heads.len() + 1);
            names.insert(cid.to_string(), name.clone());
            heads.push((name.clone(), cid.to_string()));
            author.insert(name.clone(), event.node.clone());
            parent_cids.insert(name, strings(event.fields.get("parents")));
        }
        if heads.is_empty() {
            return Err(Error::NoCommit(doc.to_string()));
        }
        let mut parents = BTreeMap::new();
        for (name, cids) in parent_cids {
            let mut set = BTreeSet::new();
            for cid in cids {
                let parent = names.get(&cid).ok_or_else(|| Error::UnknownParent {
                    head: name.clone(),
                    parent: cid.clone(),
                })?;
                set.insert(parent.clone());
            }
            parents.insert(name, set);
        }

        let steps = events
            .iter()
            .filter_map(|event| step(event, doc, &peers, &names))
            .collect();
        Ok(Self {
            nodes: nodes.iter().map(|node| node.name.clone()).collect(),
            repl: repl
                .iter()
                .map(|(from, to)| (from.to_string(), to.to_string()))
                .collect(),
            heads,
            author,
            parents,
            steps,
            bounds,
        })
    }

    /// How many events of kind `ev` the trace holds.
    pub fn count(&self, ev: &str) -> usize {
        self.steps.iter().filter(|step| step.ev == ev).count()
    }

    /// Each head's priority: one more than its highest parent's.
    fn priorities(&self) -> BTreeMap<String, usize> {
        let mut prio = BTreeMap::new();
        for (name, _) in &self.heads {
            let highest = self.parents[name]
                .iter()
                .filter_map(|parent| prio.get(parent).copied())
                .max()
                .unwrap_or(0);
            prio.insert(name.clone(), highest + 1);
        }
        prio
    }

    /// The heads in CID byte order.
    fn key_order(&self) -> Vec<String> {
        let mut keyed: Vec<_> = self
            .heads
            .iter()
            .map(|(name, cid)| (cid_bytes(cid), name.clone()))
            .collect();
        keyed.sort();
        keyed.into_iter().map(|(_, name)| name).collect()
    }

    /// The provider of the trace's advisory pull fetches.
    fn pull_peer(&self) -> &str {
        self.steps
            .iter()
            .find(|step| step.ev == "PullStart")
            .map_or(NONE, |step| step.peer.as_str())
    }

    /// The TLA+ module `name` that defines this trace's constants.
    pub fn module(&self, name: &str, story: &str) -> String {
        let mut out = String::new();
        let bar = "-".repeat(24);
        let _ = writeln!(out, "{bar} MODULE {name} {bar}");
        for line in story.lines() {
            let _ = writeln!(out, "\\* {line}");
        }
        out.push_str(
            "\\* Written by conformance::model_trace from the nodes' model_trace\n\
             \\* events. Regenerate it rather than edit it.\n\
             EXTENDS Trace_P2PDelivery\n\n",
        );
        let nodes = set(self.nodes.iter());
        let _ = writeln!(
            out,
            "\\* The nodes, by their role in the scenario.\ntNodes == {nodes}"
        );
        let edges = self
            .repl
            .iter()
            .map(|(from, to)| format!("<<\"{from}\", \"{to}\">>"))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "\\* The replicator edges the scenario set.\ntRepl == {{{edges}}}"
        );
        let _ = writeln!(out, "\\* The document's heads, in commit order:");
        for (name, cid) in &self.heads {
            let _ = writeln!(out, "\\*   {name} = {cid}");
        }
        let _ = writeln!(
            out,
            "tHeads == {}",
            set(self.heads.iter().map(|(name, _)| name))
        );
        let _ = writeln!(
            out,
            "\\* The node that committed each head.\ntAuthor == {}",
            by_head(&self.heads, |name| format!("\"{}\"", self.author[name]))
        );
        let _ = writeln!(
            out,
            "\\* The heads each head was written on top of.\ntParent == {}",
            by_head(&self.heads, |name| set(self.parents[name].iter()))
        );
        let prio = self.priorities();
        let _ = writeln!(
            out,
            "\\* Each head's priority, one more than its highest parent's.\ntPrio == {}",
            by_head(&self.heads, |name| prio[name].to_string())
        );
        let keys = self
            .key_order()
            .iter()
            .map(|name| format!("\"{name}\""))
            .collect::<Vec<_>>()
            .join(", ");
        let _ = writeln!(
            out,
            "\\* The heads in CID byte order.\ntKeyOrder == <<{keys}>>"
        );
        let _ = writeln!(
            out,
            "\\* No node starts with a durable record.\ntBacklogs == {{[n \\in tNodes |-> {{}}]}}"
        );
        let _ = writeln!(
            out,
            "\\* No shared base, trickling provider or dead node: every head is written in the run.\ntNoHeads == {{}}"
        );
        let _ = writeln!(
            out,
            "\\* The provider of the advisory pull fetches, if any ran.\ntPullPeer == \"{}\"",
            self.pull_peer()
        );
        let _ = writeln!(out, "\\* The recorded events, in time order.\ntTrace == <<");
        let last = self.steps.len().saturating_sub(1);
        for (index, step) in self.steps.iter().enumerate() {
            let comma = if index == last { "" } else { "," };
            let _ = writeln!(
                out,
                "  [ev |-> \"{}\", node |-> \"{}\", peer |-> \"{}\", head |-> \"{}\", outcome |-> \"{}\", sup |-> {}, mode |-> \"{}\"]{comma} \\* {}: {}",
                step.ev,
                step.node,
                step.peer,
                step.head,
                step.outcome,
                set(step.sup.iter()),
                step.mode,
                index + 1,
                describe(step),
            );
        }
        out.push_str(
            ">>\n=============================================================================\n",
        );
        out
    }

    /// The TLC configuration that replays module `name` under the fixed
    /// design's knobs.
    pub fn config(&self, name: &str) -> String {
        let bounds = self.bounds;
        format!(
            "\\* Replays the trace in {name} through P2PDelivery under the fixed\n\
             \\* design's knobs. TLC finds no error if every event is a model step\n\
             \\* and the run settles; a deadlock names the first event that is not.\n\
             SPECIFICATION TraceSpec\n\
             CONSTANTS\n\
             \x20 Nodes <- tNodes\n\
             \x20 Repl <- tRepl\n\
             \x20 Heads <- tHeads\n\
             \x20 Author <- tAuthor\n\
             \x20 Parent <- tParent\n\
             \x20 Prio <- tPrio\n\
             \x20 KeyOrder <- tKeyOrder\n\
             \x20 Base <- tNoHeads\n\
             \x20 Fresh <- tHeads\n\
             \x20 Backlogs <- tBacklogs\n\
             \x20 Trickle <- tNoHeads\n\
             \x20 Dead <- tNoHeads\n\
             \x20 PullPeer <- tPullPeer\n\
             \x20 PullAt <- tNodes\n\
             \x20 Storm <- tNodes\n\
             \x20 Crashable <- tNodes\n\
             \x20 ConnectedAtStart = \"down\"\n\
             \x20 Trace <- tTrace\n\
             \x20 K = {fetches}\n\
             \x20 Permits = {fetches}\n\
             \x20 PeerPermits = {peer_permits}\n\
             \x20 MemCap = {pending}\n\
             \x20 PeerQuota = {quota}\n\
             \x20 DurCap = {durable}\n\
             \x20 MaxCrash = {crashes}\n\
             \x20 MaxRegFail = {reg_fails}\n\
             \x20 MaxDisc = {discs}\n\
             \x20 MaxCont = {conts}\n\
             \x20 CoverRule = \"Ancestry\"\n\
             \x20 ShutdownOrder = \"DrainFirst\"\n\
             \x20 ForwardMode = \"BeforeMark\"\n\
             \x20 StartupOrder = \"RestoreFirst\"\n\
             \x20 FailedIntent = \"Kept\"\n\
             \x20 FanoutMode = \"PerPeer\"\n\
             \x20 SlotBeforePermit = FALSE\n\
             \x20 DispatchPath = \"Direct\"\n\
             \x20 ConnMode = \"Served\"\n\
             INVARIANT INV_TypeOK\n\
             INVARIANT INV_Bounded\n\
             INVARIANT INV_ObligationOwned\n\
             INVARIANT INV_TraceIndex\n",
            fetches = bounds.fetches,
            peer_permits = bounds.fetches.saturating_sub(1).max(1),
            pending = bounds.pending,
            quota = (bounds.pending / 4).max(1),
            durable = bounds.pending * 4,
            crashes = self.count("Crash"),
            reg_fails = self.count("RegFail"),
            discs = self.count("Disconnect"),
            conts = self.count("StartCont"),
        )
    }
}

/// The record for one logged event, or `None` when the model does not
/// follow it: a head of another document, or an event of another document.
fn step(
    event: &Logged,
    doc: &str,
    peers: &BTreeMap<String, String>,
    heads: &BTreeMap<String, String>,
) -> Option<Step> {
    if event.str("doc").is_some_and(|other| other != doc) {
        return None;
    }
    let head = match event.str("head") {
        Some(cid) => heads.get(cid)?.clone(),
        None => NONE.to_string(),
    };
    let peer = ["receiver", "sender", "provider", "peer"]
        .iter()
        .find_map(|key| event.str(key))
        .map_or_else(
            || NONE.to_string(),
            |id| {
                peers
                    .get(id)
                    .cloned()
                    .unwrap_or_else(|| "unknown".to_string())
            },
        );
    let mode = match event.fields.get("removed").and_then(Value::as_bool) {
        Some(true) => "removed".to_string(),
        Some(false) => "kept".to_string(),
        None => event
            .str("mode")
            .or_else(|| event.str("end"))
            .unwrap_or(NONE)
            .to_string(),
    };
    let sup = strings(event.fields.get("superseded"))
        .iter()
        .filter_map(|cid| heads.get(cid).cloned())
        .collect();
    Some(Step {
        ev: event.kind().to_string(),
        node: event.node.clone(),
        peer,
        head,
        outcome: event.str("outcome").unwrap_or(NONE).to_string(),
        sup,
        mode,
    })
}

/// One line on what a step means, for the module's comments.
fn describe(step: &Step) -> String {
    let Step {
        ev,
        node,
        peer,
        head,
        outcome,
        mode,
        ..
    } = step;
    match ev.as_str() {
        "Crash" => format!("{node} starts"),
        "RestoreReplicators" => format!("{node} restores its replicators"),
        "Reconnect" => format!("{node} connects to {peer} ({mode})"),
        "Disconnect" => format!("{node} loses {peer}"),
        "Commit" => format!("{node} commits {head}"),
        "Register" => format!("{node} registers {head} for {peer}"),
        "RegFail" => format!("{node} fails to register {head} for {peer}"),
        "Admit" => format!("{node} admits {head} from {peer}: {outcome}"),
        "Ack" => format!("{node} has {peer}'s ack for {head}"),
        "Schedule" => format!("{node} dispatches {head} ({mode})"),
        "Acquire" => format!("{node} takes permits for {head}"),
        "Progress" => format!("{node} gets part of {head}"),
        "Merge" => format!("{node} merges {head}"),
        "Stall" => format!("{node} gets nothing for {head} ({mode})"),
        "Retry" => format!("{node} retries {head}"),
        "GiveUp" => format!("{node} gives {head} up"),
        "Preempt" => format!("{node} cuts {head}"),
        "BecomeDue" => format!("{head} is due on {node} again"),
        "Resync" => format!("{node} re-admits {head}"),
        "Expire" => format!("{node} expires {head} ({mode})"),
        "PullStart" => format!("{node} pulls from {peer}"),
        "PullEnd" => format!("{node} ends a pull"),
        "FloodOn" => format!("{node}'s merge channel is full"),
        "FloodOff" => format!("{node}'s merge channel has room"),
        "StartCont" => format!("{node} starts a continuation"),
        "EndCont" => format!("{node} ends a continuation"),
        other => format!("{node}: {other}"),
    }
}

/// A JSON array of strings, or nothing.
fn strings(value: Option<&Value>) -> Vec<String> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// A TLA+ set of strings.
fn set<'a, I>(items: I) -> String
where
    I: Iterator<Item = &'a String>,
{
    let items: Vec<_> = items.map(|item| format!("\"{item}\"")).collect();
    format!("{{{}}}", items.join(", "))
}

/// A TLA+ function over the heads, from each head's value.
fn by_head<F>(heads: &[(String, String)], value: F) -> String
where
    F: Fn(&str) -> String,
{
    let arms = heads
        .iter()
        .map(|(name, _)| format!("hd = \"{name}\" -> {}", value(name)))
        .collect::<Vec<_>>()
        .join(" [] ");
    format!("[hd \\in tHeads |-> CASE {arms}]")
}

/// A CID's bytes, from its base32 multibase string; other encodings sort
/// as their text.
fn cid_bytes(cid: &str) -> Vec<u8> {
    let Some(body) = cid.strip_prefix('b') else {
        return cid.as_bytes().to_vec();
    };
    let mut bytes = Vec::new();
    let (mut buffer, mut bits) = (0u32, 0u32);
    for symbol in body.bytes() {
        let value = match symbol {
            b'a'..=b'z' => symbol - b'a',
            b'2'..=b'7' => symbol - b'2' + 26,
            _ => return cid.as_bytes().to_vec(),
        };
        buffer = (buffer << 5) | u32::from(value);
        bits += 5;
        if bits >= 8 {
            bits -= 8;
            bytes.push((buffer >> bits) as u8);
            buffer &= (1 << bits) - 1;
        }
    }
    bytes
}

/// What TLC said about a replay.
#[derive(Debug)]
pub struct Verdict {
    pub accepted: bool,
    pub output: String,
}

/// Replays module `name` (written with its configuration into `work`)
/// through TLC, using the models and the pinned TLC wrapper in `tla_dir`.
pub fn replay(
    tla_dir: &Path,
    work: &Path,
    name: &str,
    trace: &Trace,
    story: &str,
) -> Result<Verdict> {
    let io = |path: &Path| {
        let path = path.to_path_buf();
        move |source| Error::Io { path, source }
    };
    std::fs::create_dir_all(work.join("java")).map_err(io(work))?;
    for model in ["P2PDelivery.tla", "Trace_P2PDelivery.tla"] {
        std::fs::copy(tla_dir.join(model), work.join(model)).map_err(io(&tla_dir.join(model)))?;
    }
    let module = work.join(format!("{name}.tla"));
    std::fs::write(&module, trace.module(name, story)).map_err(io(&module))?;
    let config = work.join(format!("{name}.cfg"));
    std::fs::write(&config, trace.config(name)).map_err(io(&config))?;
    let run = Command::new(tla_dir.join("tools/tlc"))
        .current_dir(work)
        .env("TLA_JAVA_TMPDIR", work.join("java"))
        .args(["-workers", "1", "-metadir"])
        .arg(work.join("states"))
        .arg("-config")
        .arg(&config)
        .arg(&module)
        .output()
        .map_err(Error::Tlc)?;
    let output = format!(
        "{}{}",
        String::from_utf8_lossy(&run.stdout),
        String::from_utf8_lossy(&run.stderr)
    );
    let outcome = work.join("tlc.out");
    std::fs::write(&outcome, &output).map_err(io(&outcome))?;
    Ok(Verdict {
        accepted: run.status.success() && output.contains("No error has been found"),
        output,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(node: &str, json: &str) -> Logged {
        let mut fields: Map<String, Value> = serde_json::from_str(json).expect("json");
        let mt = fields.remove("mt").and_then(|mt| mt.as_u64()).unwrap_or(0);
        Logged {
            mt,
            node: node.to_string(),
            fields,
        }
    }

    fn two_node() -> Vec<Logged> {
        vec![
            event("a", r#"{"mt":1,"ev":"Crash","node":"A"}"#),
            event("b", r#"{"mt":2,"ev":"Crash","node":"B"}"#),
            event(
                "a",
                r#"{"mt":3,"ev":"Commit","doc":"d","head":"bx","parents":[]}"#,
            ),
            event(
                "a",
                r#"{"mt":4,"ev":"Commit","doc":"other","head":"by","parents":[]}"#,
            ),
            event(
                "a",
                r#"{"mt":5,"ev":"Register","doc":"d","receiver":"B","head":"bx"}"#,
            ),
            event(
                "b",
                r#"{"mt":6,"ev":"Admit","doc":"d","sender":"A","head":"bx","outcome":"registered","superseded":[]}"#,
            ),
            event("b", r#"{"mt":7,"ev":"Acquire","head":"by"}"#),
            event("b", r#"{"mt":8,"ev":"Stall","head":"bx","end":"hold"}"#),
            event("b", r#"{"mt":9,"ev":"Expire","head":"bx","removed":false}"#),
        ]
    }

    fn nodes() -> Vec<NodeLog> {
        ["a", "b"]
            .iter()
            .map(|name| NodeLog {
                name: name.to_string(),
                log: PathBuf::new(),
            })
            .collect()
    }

    #[test]
    fn abstraction_names_peers_and_heads_and_drops_other_documents() {
        let trace = Trace::abstracted(&nodes(), &[("a", "b")], "d", &two_node(), Bounds::default())
            .expect("trace");
        assert_eq!(trace.heads, vec![("h1".to_string(), "bx".to_string())]);
        let kinds: Vec<_> = trace.steps.iter().map(|step| step.ev.as_str()).collect();
        assert_eq!(
            kinds,
            ["Crash", "Crash", "Commit", "Register", "Admit", "Stall", "Expire"]
        );
        assert_eq!(trace.steps[3].peer, "b");
        assert_eq!(trace.steps[4].peer, "a");
        assert_eq!(trace.steps[4].outcome, "registered");
        assert_eq!(trace.steps[5].mode, "hold");
        assert_eq!(trace.steps[6].mode, "kept");
    }

    #[test]
    fn a_node_without_a_start_event_is_reported() {
        let events: Vec<_> = two_node().into_iter().skip(1).collect();
        let error = Trace::abstracted(&nodes(), &[("a", "b")], "d", &events, Bounds::default())
            .expect_err("a has no start");
        assert!(matches!(error, Error::NoStart(name) if name == "a"));
    }

    #[test]
    fn module_and_config_name_every_constant_the_trace_spec_needs() {
        let trace = Trace::abstracted(&nodes(), &[("a", "b")], "d", &two_node(), Bounds::default())
            .expect("trace");
        let module = trace.module("MC_Example", "An example.");
        for definition in [
            "tNodes ==",
            "tRepl ==",
            "tHeads ==",
            "tAuthor ==",
            "tParent ==",
            "tPrio ==",
            "tKeyOrder ==",
            "tBacklogs ==",
            "tNoHeads ==",
            "tPullPeer ==",
            "tTrace ==",
        ] {
            assert!(
                module.contains(definition),
                "{definition} missing:\n{module}"
            );
        }
        let config = trace.config("MC_Example");
        assert!(config.contains("MaxCrash = 2"), "{config}");
        assert!(config.contains("SPECIFICATION TraceSpec"), "{config}");
    }

    #[test]
    fn base32_cids_sort_by_their_bytes() {
        assert_eq!(cid_bytes("bae"), vec![0x01]);
        assert!(cid_bytes("ba7") > cid_bytes("baz"));
    }
}
