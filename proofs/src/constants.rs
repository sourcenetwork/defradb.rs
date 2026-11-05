//! The P2P constants the models depend on, each stated once.
//!
//! Every row names the Rust constant that sets a value and the model
//! constants that stand for it. An `Exact` model site restates the value, in
//! the unit the Rust initializer uses; a `Scaled` site takes a smaller instance
//! of it, so only its declaration in a `CONSTANT`/`CONSTANTS` list is checked,
//! comments excluded. `every_constant_agrees` fails when the Rust initializer,
//! an exact model site or this table disagree, or a scaled model constant is no
//! longer declared: change all three together.

/// A value, in the unit its Rust initializer is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Value {
    Count(u64),
    Secs(u64),
    Millis(u64),
    List(&'static [u64]),
}

/// A model constant that stands for a Rust constant.
#[derive(Debug, Clone, Copy)]
pub enum Model {
    /// `file` defines `name` as exactly this value: a Lean `def`, a TLC
    /// config `name = v` or a TLA+ `name == v`.
    Exact(&'static str, &'static str),
    /// `file` declares `name` in a `CONSTANT`/`CONSTANTS` list and checks a
    /// smaller instance of the value.
    Scaled(&'static str, &'static str),
}

pub struct Constant {
    /// The Rust source file that defines the constant.
    pub file: &'static str,
    /// The Rust constant.
    pub name: &'static str,
    pub value: Value,
    pub models: &'static [Model],
}

use std::fs;
use std::path::Path;

use Model::{Exact, Scaled};
use Value::{Count, List, Millis, Secs};

const PENDING: &str = "crates/p2p/src/sync/manager/pending.rs";
const SYNC_CONFIG: &str = "crates/p2p/src/sync/manager/config.rs";
const DAG_RETRY: &str = "crates/p2p/src/sync/coordinator/dag_retry.rs";
const ADAPTER_RETRY: &str = "crates/p2p-adapter/src/retry.rs";
const DEPTH_CUT_CONFIGS: [&str; 3] = [
    "proofs/tla/MC_DepthCutFetch_Green.cfg",
    "proofs/tla/MC_DepthCutFetch_Red_NoWiden.cfg",
    "proofs/tla/MC_DepthCutFetch_Red_Unbounded.cfg",
];

pub const CONSTANTS: &[Constant] = &[
    Constant {
        file: PENDING,
        name: "PENDING_DAG_TTL",
        value: Secs(300),
        models: &[],
    },
    Constant {
        file: PENDING,
        name: "PENDING_RETRY_BASE",
        value: Secs(2),
        models: &[],
    },
    Constant {
        file: PENDING,
        name: "PENDING_RETRY_CAP",
        value: Secs(60),
        models: &[],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_MAX_CONCURRENT_DAG_FETCHES",
        value: Count(4),
        models: &[Scaled("proofs/tla/SyncOwnership.tla", "FetchCap")],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_MAX_PENDING_DAGS",
        value: Count(1000),
        models: &[
            Scaled("proofs/tla/PushLogAdmission.tla", "Cap"),
            Scaled("proofs/tla/PendingDagRestart.tla", "Cap"),
            Scaled("proofs/tla/SyncOwnership.tla", "Cap"),
        ],
    },
    Constant {
        file: "crates/p2p/src/sync/manager/process/pending_dag.rs",
        name: "PENDING_DAG_PEER_CAPACITY_DIVISOR",
        value: Count(4),
        models: &[],
    },
    Constant {
        file: "crates/p2p/src/sync/manager/process/mod.rs",
        name: "PERSISTED_PENDING_CAP_FACTOR",
        value: Count(4),
        models: &[],
    },
    Constant {
        file: "crates/p2p/src/sync/manager/process/join.rs",
        name: "PUSHLOG_JOIN_DEADLINE",
        value: Millis(500),
        models: &[Scaled("proofs/tla/PushLogJoin.tla", "Deadline")],
    },
    Constant {
        file: "crates/p2p/src/sync/mod.rs",
        name: "PERSISTED_RETRY_SWEEP_INTERVAL",
        value: Secs(2),
        models: &[],
    },
    Constant {
        file: "crates/storage/src/stores/retry_info.rs",
        name: "RETRY_INTERVALS_SECS",
        value: List(&[30, 60, 120, 240, 480, 960, 1920]),
        models: &[Exact(
            "proofs/lean/RetrySchedule/Ladder.lean",
            "defaultLadder",
        )],
    },
    Constant {
        file: ADAPTER_RETRY,
        name: "REPLAY_TIMEOUT",
        value: Secs(15),
        models: &[],
    },
    Constant {
        file: ADAPTER_RETRY,
        name: "MAX_PEER_PASS",
        value: Secs(30),
        models: &[],
    },
    Constant {
        file: ADAPTER_RETRY,
        name: "MAX_MARKERS_PER_PEER_PASS",
        value: Count(64),
        models: &[],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_MAX_CONCURRENT_PUSH_TASKS",
        value: Count(8),
        models: &[Scaled("proofs/tla/PushBacklog.tla", "Workers")],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_MAX_ACTIVE_PUSHES_PER_PEER",
        value: Count(4),
        models: &[Scaled("proofs/tla/PushBacklog.tla", "PerPeerCap")],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_PUSH_QUEUE_CAPACITY",
        value: Count(1024),
        models: &[Scaled("proofs/tla/PushBacklog.tla", "QueueCap")],
    },
    Constant {
        file: SYNC_CONFIG,
        name: "DEFAULT_PUSH_SEND_TIMEOUT",
        value: Secs(30),
        models: &[],
    },
    Constant {
        file: DAG_RETRY,
        name: "MAX_FETCH_ATTEMPTS",
        value: Count(3),
        models: &[],
    },
    Constant {
        file: DAG_RETRY,
        name: "FAST_REFUSAL",
        value: Secs(3),
        models: &[Scaled("proofs/tla/DagFetchBudget.tla", "Fast")],
    },
    Constant {
        file: DAG_RETRY,
        name: "MAX_FREE_REFUSALS",
        value: Count(11),
        models: &[Scaled("proofs/tla/DagFetchBudget.tla", "MaxFree")],
    },
    Constant {
        file: "crates/p2p/src/sync/coordinator/dag_fetcher.rs",
        name: "BLOCK_SYNC_COMPLETION_WATCHDOG",
        value: Secs(35),
        models: &[Scaled("proofs/tla/DagFetchBudget.tla", "Window")],
    },
    Constant {
        file: "crates/p2p/src/sync/coordinator/provider_health.rs",
        name: "COOLDOWN_BASE",
        value: Secs(2),
        models: &[Exact("proofs/lean/ProviderHealth/Ranking.lean", "base")],
    },
    Constant {
        file: "crates/p2p/src/sync/coordinator/provider_health.rs",
        name: "COOLDOWN_MAX",
        value: Secs(30),
        models: &[Exact("proofs/lean/ProviderHealth/Ranking.lean", "ceiling")],
    },
    Constant {
        file: "crates/p2p/src/sync/coordinator/closure_depth.rs",
        name: "INITIAL_CLOSURE_DEPTH",
        value: Count(2),
        models: &[
            Exact(DEPTH_CUT_CONFIGS[0], "InitCut"),
            Exact(DEPTH_CUT_CONFIGS[1], "InitCut"),
            Exact(DEPTH_CUT_CONFIGS[2], "InitCut"),
        ],
    },
    Constant {
        file: "crates/p2p/src/sync/car.rs",
        name: "CAR_MAX_BLOCKS",
        value: Count(10_000),
        models: &[Scaled("proofs/tla/DepthCutFetch.tla", "Cap")],
    },
    Constant {
        file: "crates/p2p/src/sync/car.rs",
        name: "CAR_MAX_BYTES",
        value: Count(16 * 1024 * 1024),
        models: &[],
    },
    Constant {
        file: "crates/p2p/src/iroh/fetch_stats.rs",
        name: "SCALE",
        value: Count(1_000_000),
        models: &[Exact("proofs/lean/FetchStats/Rolling.lean", "scale")],
    },
    Constant {
        file: "crates/p2p/src/iroh/fetch_stats.rs",
        name: "WEIGHT",
        value: Count(8),
        models: &[Exact("proofs/lean/FetchStats/Rolling.lean", "weight")],
    },
    Constant {
        file: "crates/p2p/src/iroh/protocols.rs",
        name: "MAX_STREAM_TAG_LEN",
        value: Count(64),
        models: &[Exact("proofs/lean/GossipFrame/Framing.lean", "maxTagLen")],
    },
];

/// What an initializer or a model definition states.
#[derive(Debug, PartialEq, Eq)]
enum Stated {
    Count(u64),
    Secs(u64),
    Millis(u64),
    List(Vec<u64>),
}

impl Value {
    fn stated(self) -> Stated {
        match self {
            Count(n) => Stated::Count(n),
            Secs(n) => Stated::Secs(n),
            Millis(n) => Stated::Millis(n),
            List(items) => Stated::List(items.to_vec()),
        }
    }

    /// A model states a number in the Rust unit, or the same list.
    fn agrees(self, model: &Stated) -> bool {
        match (self, model) {
            (List(ours), Stated::List(theirs)) => ours == theirs.as_slice(),
            (Count(n) | Secs(n) | Millis(n), Stated::Count(m)) => n == *m,
            _ => false,
        }
    }
}

/// An integer product, a `Duration::from_secs`/`from_millis` of one, or a
/// slice of integers.
fn rust_value(initializer: &str) -> Option<Stated> {
    let expr: String = initializer.chars().filter(|c| !c.is_whitespace()).collect();
    let call = |name: &str| {
        expr.strip_prefix(name)
            .and_then(|rest| rest.strip_suffix(')'))
            .and_then(product)
    };
    if let Some(secs) = call("Duration::from_secs(") {
        return Some(Stated::Secs(secs));
    }
    if let Some(millis) = call("Duration::from_millis(") {
        return Some(Stated::Millis(millis));
    }
    model_value(expr.trim_start_matches('&'))
}

/// An integer product or a list of them.
fn model_value(definition: &str) -> Option<Stated> {
    let expr: String = definition.chars().filter(|c| !c.is_whitespace()).collect();
    match expr
        .strip_prefix('[')
        .and_then(|list| list.strip_suffix(']'))
    {
        Some(items) => items
            .split(',')
            .filter(|item| !item.is_empty())
            .map(product)
            .collect::<Option<Vec<_>>>()
            .map(Stated::List),
        None => product(&expr).map(Stated::Count),
    }
}

fn product(expr: &str) -> Option<u64> {
    expr.split('*').try_fold(1u64, |acc, factor| {
        acc.checked_mul(factor.replace('_', "").parse().ok()?)
    })
}

/// The initializer of `const name: T = ...;` in Rust source.
fn rust_initializer<'a>(source: &'a str, name: &str) -> Option<&'a str> {
    let needle = format!("const {name}:");
    let rest = &source[source.find(&needle)? + needle.len()..];
    rest.get(rest.find('=')? + 1..rest.find(';')?)
}

/// The definition of `name` in a Lean file, a TLC config or a TLA+ module.
fn model_definition<'a>(source: &'a str, file: &str, name: &str) -> Option<&'a str> {
    let lean = file.ends_with(".lean");
    let separator = if lean {
        ":="
    } else if file.ends_with(".cfg") {
        "="
    } else {
        "=="
    };
    source.lines().map(str::trim).find_map(|line| {
        let head = if lean {
            line.strip_prefix("def ")?.split_whitespace().next()?
        } else {
            line.split(|c: char| c.is_whitespace() || c == '=').next()?
        };
        if head != name {
            return None;
        }
        line.split_once(separator).map(|(_, value)| value)
    })
}

/// TLA+ source with `\*` line comments and `(* *)` block comments removed.
fn without_comments(source: &str) -> String {
    let mut text = String::with_capacity(source.len());
    let mut depth = 0usize;
    let mut chars = source.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('(', Some('*')) => {
                chars.next();
                depth += 1;
            }
            ('*', Some(')')) if depth > 0 => {
                chars.next();
                depth -= 1;
            }
            ('\\', Some('*')) if depth == 0 => {
                for c in chars.by_ref() {
                    if c == '\n' {
                        text.push(c);
                        break;
                    }
                }
            }
            _ if depth == 0 => text.push(c),
            _ => {}
        }
    }
    text
}

/// Whether a TLA+ module lists `name` in a `CONSTANT` or `CONSTANTS`
/// declaration.
fn declares_constant(source: &str, name: &str) -> bool {
    let text = without_comments(source);
    let mut tokens = Vec::new();
    let mut word = String::new();
    for c in text.chars().chain(['\n']) {
        if c.is_ascii_alphanumeric() || c == '_' {
            word.push(c);
            continue;
        }
        if !word.is_empty() {
            tokens.push(std::mem::take(&mut word));
        }
        if !c.is_whitespace() {
            tokens.push(c.to_string());
        }
    }
    let mut at = 0;
    while at < tokens.len() {
        let keyword = matches!(tokens[at].as_str(), "CONSTANT" | "CONSTANTS");
        at += 1;
        if !keyword {
            continue;
        }
        while let Some(declared) = tokens.get(at) {
            if declared == name {
                return true;
            }
            if tokens.get(at + 1).map(String::as_str) != Some(",") {
                break;
            }
            at += 2;
        }
    }
    false
}

/// Every way the code, a restating model or this table disagree; empty when
/// they agree.
pub fn disagreements(root: &Path) -> Vec<String> {
    let read = |file: &str| fs::read_to_string(root.join(file)).unwrap_or_default();
    let mut drift = Vec::new();
    for constant in CONSTANTS {
        let source = read(constant.file);
        match rust_initializer(&source, constant.name) {
            Some(init) if rust_value(init) == Some(constant.value.stated()) => {}
            Some(init) => drift.push(format!(
                "{} in {} is `{}`, the table says {:?}",
                constant.name,
                constant.file,
                init.trim(),
                constant.value
            )),
            None => drift.push(format!(
                "{} is not defined in {}",
                constant.name, constant.file
            )),
        }
        for model in constant.models {
            match *model {
                Exact(file, name) => {
                    let source = read(file);
                    let found = model_definition(&source, file, name);
                    let stated = found.and_then(model_value);
                    if !stated.is_some_and(|stated| constant.value.agrees(&stated)) {
                        drift.push(format!(
                            "{name} in {file} is `{}`, {} is {:?}",
                            found.map_or("undefined", str::trim),
                            constant.name,
                            constant.value
                        ));
                    }
                }
                Scaled(file, name) => {
                    if !declares_constant(&read(file), name) {
                        drift.push(format!(
                            "{name}, scaling {}, is not a CONSTANT of {file}",
                            constant.name
                        ));
                    }
                }
            }
        }
    }
    drift
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::anchor::repository_root;

    #[test]
    fn every_constant_agrees() {
        let drift = disagreements(&repository_root());
        assert!(
            drift.is_empty(),
            "P2P constants out of agreement:\n{}",
            drift.join("\n")
        );
    }

    #[test]
    fn initializers_and_definitions_parse() {
        assert_eq!(
            rust_value(" Duration::from_secs(60 * 60)"),
            Some(Stated::Secs(3600))
        );
        assert_eq!(
            rust_value(" Duration::from_millis(500)"),
            Some(Stated::Millis(500))
        );
        assert_eq!(
            rust_value(" 16 * 1024 * 1024"),
            Some(Stated::Count(16 << 20))
        );
        assert_eq!(
            rust_value(" &[\n    30, 60,\n]"),
            Some(Stated::List(vec![30, 60]))
        );
        assert_eq!(rust_value(" CAR_MAX_BLOCKS as u32"), None);
        let lean = "-- the base\ndef base : Nat := 2\n";
        assert_eq!(model_definition(lean, "x.lean", "base"), Some(" 2"));
        let cfg = "CONSTANTS\n  N = 24\n  InitCut = 2\n";
        assert_eq!(model_definition(cfg, "x.cfg", "InitCut"), Some(" 2"));
        let tla = "Window == 3\n";
        assert_eq!(model_definition(tla, "x.tla", "Window"), Some(" 3"));
    }

    #[test]
    fn a_scaled_constant_must_be_declared() {
        let declared = "CONSTANTS\n  Window,  \\* the window\n  Fast\nASSUME Fast < Window\n";
        assert!(declares_constant(declared, "Window"));
        assert!(declares_constant(declared, "Fast"));
        assert!(declares_constant("CONSTANT Deadline\n", "Deadline"));
        assert!(declares_constant("CONSTANTS N , Cap\n", "Cap"));
        for refused in [
            "CONSTANTS FastCost\n\\* Fast is FAST_REFUSAL\nASSUME FastCost > 0\n",
            "CONSTANTS (* Fast, *) FastCost\n",
            "CONSTANTS Window\nFast == 1\n",
            "VARIABLES Fast\n",
        ] {
            assert!(!declares_constant(refused, "Fast"), "{refused}");
        }
    }
}
