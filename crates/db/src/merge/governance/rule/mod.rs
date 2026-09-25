//! Merge rules carried as code: a wasm module named by CID, run under a fuel
//! budget, judging a composite or a definition through a step protocol.
//!
//! The version ID commits to the rule tag (`@governed(rule:)`). Here the tag
//! is the CID of the module's bytes, so every replica judging a write under
//! a version runs the same code over the same inputs: the rule is part of
//! what replicas agree on, not a deployment detail.
//!
//! # The step protocol
//!
//! The module is a pure function of one request: the candidate, and the
//! inputs fetched so far. It answers with a verdict, or with the keys it
//! needs next. The host fetches each key through the merge view and runs
//! the module again with the inputs extended, up to [`RuleBudget::steps`]
//! times. A key the host cannot satisfy is a defer naming it, in the same
//! vocabulary the deferral index re-drives on. No host call happens inside
//! the module, so the module needs no imports, its execution is bounded by
//! fuel per step and steps per verdict, and the inputs a verdict consumed
//! are the closure an audit would replay it over.
//!
//! # The guest ABI
//!
//! Exports: `memory`, `alloc(len: i32) -> ptr: i32`, and
//! `judge(ptr: i32, len: i32) -> ptr: i32`. The request is CBOR at
//! `(ptr, len)`; the response is CBOR at the returned pointer, preceded by
//! its length as a little-endian `u32`.
//!
//! Request:
//!
//! ```text
//! { "step": u, "kind": "composite" | "definition", "candidate": {...},
//!   "inputs": { key: value, ... } }
//! ```
//!
//! `step` is the first key so a module can read it at a fixed offset.
//! A composite candidate: `cid`, `doc_id`, `collection`, `collection_id`,
//! `version_id`, `rule`, `is_genesis`, `signature` (`"verified"`,
//! `"invalid"`, `"unsigned"`, `"not_held"`), `signer` (the DID, when
//! verified), and `fields`, the composite's own linked fields as
//! `[[name, value], ...]` where a value is `{"value": v}`, `"encrypted"`,
//! `{"not_held": cid}` or `"undecodable"`. A definition candidate: `cid`,
//! `collection`, `collection_id`, `version_id`, `rule`, `is_initial`,
//! `previous_version_id`, `previous_rule`.
//!
//! Keys a module may ask for, and what the host puts under them:
//!
//! | key | value | if not held |
//! |---|---|---|
//! | `fields:<cid>` | that composite's fields, as above | defer awaiting the composite |
//! | `genesis:<cid>` | the genesis CID as a string | defer, unnamed |
//! | `find:<collection>:<field>:<hex of the CBOR value>` | the matching document ids, sorted | never absent: an empty list |
//! | `immutable:<collection>:<doc_id>` | `[[field, value], ...]`, or `null` when the document is not held | the module decides |
//!
//! Response:
//!
//! ```text
//! { "verdict": "accept" }
//! { "verdict": "reject", "reason": s }
//! { "verdict": "defer", "reason": s, "awaiting": [ {"composite": cid} | {"immutable_field": {"collection": s, "field": s, "value": v}} ] }
//! { "verdict": "need", "keys": [ key, ... ] }
//! ```
//!
//! Any verdict may carry `"emit": [ {"collection": s, "fields": {name: v, ...}}, ... ]`:
//! documents the host writes beside the verdict, as unsigned genesis
//! composites, so the same fact is the same record on every replica that
//! finds it (`governance::emission`). Emit only what no later arrival takes
//! back; a fork found mid-verdict qualifies, "not yet" never does.
//!
//! A step may ask for at most [`MAX_KEYS_PER_STEP`] keys and a verdict gather
//! at most [`MAX_INPUTS`], a key asked for twice is fetched once, and a
//! response whose length runs past the module's memory is refused before
//! anything is allocated for it.
//!
//! A module that traps, exceeds its fuel, or answers malformed CBOR is an
//! error, not a verdict: the composite stays unmerged and the sweep will
//! try again. A module that asks for more steps than the budget allows is
//! the same.
//!
//! # Engines
//!
//! The ABI names no engine. [`RuleEngine::Wasmi`] interprets, builds for
//! every target and is the only one a browser has;
//! [`RuleEngine::Wasmtime`] compiles and is a native node's default. Their
//! fuel units differ, so a budget is a bound on cost and never an input to
//! a verdict; the `engine` module says why that costs liveness, not
//! agreement.

use std::sync::Arc;

use async_trait::async_trait;
use ciborium::Value;
use cid::Cid;
use defra_core::thread_bounds::MaybeSendSync;
use document::NormalValue;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

mod engine;
mod wasmi_engine;
#[cfg(not(target_arch = "wasm32"))]
mod wasmtime_engine;

pub use engine::RuleEngine;
use engine::{Compiled, Runtime};

use super::awaited::Awaited;
use super::signature::SignatureStatus;
use super::validator::{DefinitionCandidate, MergeCandidate, MergeValidator};
use super::verdict::MergeVerdict;
use super::view::{FieldValue, MergeView};
use super::{Emission, Judged};

/// Where rule modules come from, by the CID the version names.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait RuleModules: MaybeSendSync {
    /// The module's bytes, or `None` when this node does not hold them.
    async fn module(&self, cid: &Cid) -> Result<Option<Vec<u8>>, String>;
}

/// Rule modules read from a blockstore: the module is a block like any other.
pub struct BlockstoreModules<B: blockstore::Blockstore> {
    blockstore: Arc<B>,
}

impl<B: blockstore::Blockstore> BlockstoreModules<B> {
    pub fn new(blockstore: Arc<B>) -> Self {
        Self { blockstore }
    }

    /// Hold a module's bytes, returning the CID a rule tag names it by.
    ///
    /// Marked merged as it is put: a module is not a delta, nothing will
    /// ever merge it, and a block left unmerged is one the governance sweep
    /// reads on every pass.
    pub async fn put(&self, bytes: &[u8]) -> Result<Cid, String> {
        let cid = module_cid(bytes);
        self.blockstore
            .put(&cid, bytes)
            .await
            .map_err(|error| error.to_string())?;
        self.blockstore
            .mark_as_merged(&cid)
            .await
            .map_err(|error| error.to_string())?;
        Ok(cid)
    }
}

/// The CID a rule tag names a module by: CIDv1, raw codec, SHA2-256 over
/// the module's bytes.
pub fn module_cid(bytes: &[u8]) -> Cid {
    use sha2::Digest as _;
    let digest = sha2::Sha256::digest(bytes);
    Cid::new_v1(
        RAW_CODEC,
        cid::multihash::Multihash::wrap(SHA2_256, &digest).expect("a 32-byte digest fits"),
    )
}

const RAW_CODEC: u64 = 0x55;
const SHA2_256: u64 = 0x12;

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl<B: blockstore::Blockstore + 'static> RuleModules for BlockstoreModules<B> {
    async fn module(&self, cid: &Cid) -> Result<Option<Vec<u8>>, String> {
        self.blockstore
            .get(cid)
            .await
            .map(|bytes| bytes.map(|bytes| bytes.to_vec()))
            .map_err(|error| error.to_string())
    }
}

/// The most keys one step may ask for, and the most inputs one verdict may
/// gather: each key is a fetch, `find:` a scan, and every request carries
/// every input gathered so far.
pub const MAX_KEYS_PER_STEP: usize = 64;
pub const MAX_INPUTS: usize = 256;

/// What one verdict may cost.
///
/// A bound, never an input: the verdict a module reaches within budget does
/// not depend on the budget, and a module that exceeds it produces no
/// verdict at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuleBudget {
    /// Fuel per step: roughly one unit per wasm instruction, in the units of
    /// the engine running it, which are not the same across engines.
    pub fuel: u64,
    /// Steps per verdict: how many times the module may ask for more input.
    pub steps: usize,
    /// Linear memory the module may grow to, in bytes.
    pub memory_bytes: usize,
}

impl Default for RuleBudget {
    fn default() -> Self {
        Self {
            fuel: 10_000_000,
            steps: 16,
            memory_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A validator whose rule is the wasm module the version names.
pub struct WasmRules {
    runtime: Runtime,
    modules: Arc<dyn RuleModules>,
    compiled: HopscotchMap<Cid, Arc<Compiled>, RandomState>,
    budget: RuleBudget,
}

impl WasmRules {
    /// On this target's default engine, under the default budget.
    pub fn new(modules: Arc<dyn RuleModules>) -> Result<Self, String> {
        Self::with_budget(modules, RuleBudget::default())
    }

    /// On this target's default engine.
    pub fn with_budget(modules: Arc<dyn RuleModules>, budget: RuleBudget) -> Result<Self, String> {
        Self::with_engine(modules, RuleEngine::default(), budget)
    }

    /// On `engine`, which fails when this build cannot run it.
    pub fn with_engine(
        modules: Arc<dyn RuleModules>,
        engine: RuleEngine,
        budget: RuleBudget,
    ) -> Result<Self, String> {
        Ok(Self {
            runtime: Runtime::new(engine)?,
            modules,
            compiled: HopscotchMap::with_hasher(RandomState::default()),
            budget,
        })
    }

    pub fn engine(&self) -> RuleEngine {
        self.runtime.engine()
    }

    pub fn budget(&self) -> RuleBudget {
        self.budget
    }

    /// Compile a module now and keep it for the versions that name it,
    /// returning its CID: an installer finds a module this engine cannot
    /// run when it installs it, not when the first write it governs stays
    /// unmerged.
    pub fn precompile(&self, bytes: &[u8]) -> Result<Cid, String> {
        let cid = module_cid(bytes);
        if self.compiled.get(&cid).is_none() {
            let module = self
                .runtime
                .compile(bytes)
                .map_err(|error| format!("rule module {cid} does not compile: {error}"))?;
            self.compiled.insert(cid, Arc::new(module));
        }
        Ok(cid)
    }

    /// The compiled module a version names, `Ok(None)` when its bytes are
    /// not held here.
    async fn module_for(&self, rule: &str) -> Result<Option<Arc<Compiled>>, String> {
        let cid: Cid = rule
            .parse()
            .map_err(|_| format!("rule tag {rule} is not a CID"))?;
        if let Some(module) = self.compiled.get(&cid) {
            return Ok(Some(module));
        }
        let Some(bytes) = self.modules.module(&cid).await? else {
            return Ok(None);
        };
        let module = Arc::new(
            self.runtime
                .compile(&bytes)
                .map_err(|error| format!("rule module {cid} does not compile: {error}"))?,
        );
        self.compiled.insert(cid, module.clone());
        Ok(Some(module))
    }

    /// One run of the module over `request`.
    fn step(&self, module: &Compiled, request: &[u8]) -> Result<Value, String> {
        let response = self.runtime.run(module, request, &self.budget)?;
        ciborium::from_reader(response.as_slice())
            .map_err(|error| format!("rule module response is not CBOR: {error}"))
    }

    /// Run the step protocol to a verdict.
    async fn run(
        &self,
        rule: Option<&str>,
        kind: &str,
        candidate: Value,
        view: &dyn MergeView,
    ) -> Result<Judged, String> {
        let Some(rule) = rule else {
            return Ok(MergeVerdict::reject("the version names no rule").into());
        };
        let Some(module) = self.module_for(rule).await? else {
            // A block, not a composite: nothing merges to release it, so the
            // sweep is what re-judges this.
            return Ok(MergeVerdict::defer(
                format!("rule module {rule} not held"),
                std::iter::empty::<Awaited>(),
            )
            .into());
        };
        let mut inputs: Vec<(Value, Value)> = Vec::new();
        let mut fetched: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
        for step in 0..self.budget.steps {
            let request = Value::Map(vec![
                (text("step"), Value::Integer((step as u64).into())),
                (text("kind"), text(kind)),
                (text("candidate"), candidate.clone()),
                (text("inputs"), Value::Map(inputs.clone())),
            ]);
            let mut bytes = Vec::new();
            ciborium::into_writer(&request, &mut bytes).map_err(|error| error.to_string())?;
            let response = self.step(&module, &bytes)?;
            match parse_response(&response)? {
                Response::Judged(judged) => return Ok(judged),
                Response::Need(keys) => {
                    if keys.len() > MAX_KEYS_PER_STEP {
                        return Err(format!(
                            "rule module {rule} asked for {} keys in one step, more than {MAX_KEYS_PER_STEP}",
                            keys.len()
                        ));
                    }
                    for key in keys {
                        // A key already fetched is already in `inputs`.
                        if !fetched.insert(key.clone()) {
                            continue;
                        }
                        if fetched.len() > MAX_INPUTS {
                            return Err(format!(
                                "rule module {rule} asked for more than {MAX_INPUTS} inputs"
                            ));
                        }
                        match fetch(view, &key).await? {
                            Fetched::Value(value) => inputs.push((text(&key), value)),
                            Fetched::Defer(verdict) => return Ok(verdict.into()),
                        }
                    }
                }
            }
        }
        Err(format!(
            "rule module {rule} asked for more than {} steps",
            self.budget.steps
        ))
    }
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
impl MergeValidator for WasmRules {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge(candidate, view).await?.verdict)
    }

    async fn validate_definition(
        &self,
        candidate: &DefinitionCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        Ok(self.judge_definition(candidate, view).await?.verdict)
    }

    async fn judge(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<Judged, String> {
        let fields = view
            .composite_fields(candidate.cid)
            .await?
            .map(fields_value)
            .unwrap_or(Value::Null);
        let (signature, signer) = match &candidate.signature {
            SignatureStatus::Verified(did) => ("verified", Some(did.clone())),
            SignatureStatus::Invalid(_) => ("invalid", None),
            SignatureStatus::Unsigned => ("unsigned", None),
            SignatureStatus::NotHeld(_) => ("not_held", None),
        };
        let value = Value::Map(vec![
            (text("cid"), text(&candidate.cid.to_string())),
            (text("doc_id"), text(candidate.doc_id)),
            (text("collection"), text(&candidate.collection.name)),
            (
                text("collection_id"),
                text(&candidate.collection.collection_id),
            ),
            (text("version_id"), text(&candidate.collection.version_id)),
            (
                text("rule"),
                opt_text(candidate.collection.governance_rule.as_deref()),
            ),
            (text("is_genesis"), Value::Bool(candidate.is_genesis)),
            (text("signature"), text(signature)),
            (text("signer"), opt_text(signer.as_deref())),
            (text("fields"), fields),
        ]);
        self.run(
            candidate.collection.governance_rule.as_deref(),
            "composite",
            value,
            view,
        )
        .await
    }

    async fn judge_definition(
        &self,
        candidate: &DefinitionCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<Judged, String> {
        let value = Value::Map(vec![
            (text("cid"), text(&candidate.cid.to_string())),
            (text("collection"), text(&candidate.version.name)),
            (
                text("collection_id"),
                text(&candidate.version.collection_id),
            ),
            (text("version_id"), text(&candidate.version.version_id)),
            (
                text("rule"),
                opt_text(candidate.version.governance_rule.as_deref()),
            ),
            (text("is_initial"), Value::Bool(candidate.is_initial())),
            (
                text("previous_version_id"),
                opt_text(
                    candidate
                        .previous
                        .map(|previous| previous.version_id.as_str()),
                ),
            ),
            (
                text("previous_rule"),
                opt_text(
                    candidate
                        .previous
                        .and_then(|previous| previous.governance_rule.as_deref()),
                ),
            ),
        ]);
        // The rule in force judges the change to it: a patch is judged by the
        // module the version it supersedes names, so a patch cannot admit
        // itself by naming a permissive module. An initial definition is
        // self-certifying and judged by its own; so is a patch of a version
        // that named no rule, since no code was in force.
        let in_force = candidate
            .previous
            .and_then(|previous| previous.governance_rule.as_deref())
            .or(candidate.version.governance_rule.as_deref());
        self.run(in_force, "definition", value, view).await
    }
}

enum Response {
    Judged(Judged),
    Need(Vec<String>),
}

fn parse_response(response: &Value) -> Result<Response, String> {
    let map = response
        .as_map()
        .ok_or("rule module response is not a map")?;
    let get = |key: &str| {
        map.iter()
            .find(|(k, _)| k.as_text() == Some(key))
            .map(|(_, v)| v)
    };
    let verdict = get("verdict")
        .and_then(Value::as_text)
        .ok_or("rule module response names no verdict")?;
    let reason = || {
        get("reason")
            .and_then(Value::as_text)
            .unwrap_or("rule module gave no reason")
            .to_string()
    };
    let emit = || -> Result<Vec<Emission>, String> {
        get("emit")
            .and_then(Value::as_array)
            .map(|items| {
                items
                    .iter()
                    .map(parse_emission)
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()
            .map(Option::unwrap_or_default)
    };
    let judged = |verdict: MergeVerdict| -> Result<Response, String> {
        Ok(Response::Judged(Judged {
            verdict,
            emit: emit()?,
        }))
    };
    Ok(match verdict {
        "accept" => judged(MergeVerdict::Accept)?,
        "reject" => judged(MergeVerdict::reject(reason()))?,
        "defer" => {
            let awaiting = get("awaiting")
                .and_then(Value::as_array)
                .map(|items| {
                    items
                        .iter()
                        .map(parse_awaited)
                        .collect::<Result<Vec<_>, _>>()
                })
                .transpose()?
                .unwrap_or_default();
            judged(MergeVerdict::defer(reason(), awaiting))?
        }
        "need" => {
            let keys = get("keys")
                .and_then(Value::as_array)
                .ok_or("rule module needs nothing it names")?
                .iter()
                .map(|key| {
                    key.as_text()
                        .map(str::to_string)
                        .ok_or("rule module key is not text".to_string())
                })
                .collect::<Result<Vec<_>, _>>()?;
            Response::Need(keys)
        }
        other => return Err(format!("rule module verdict {other} is unknown")),
    })
}

/// `{"collection": s, "fields": {name: v, ...}}`.
fn parse_emission(value: &Value) -> Result<Emission, String> {
    let map = value.as_map().ok_or("emission is not a map")?;
    let get = |key: &str| {
        map.iter()
            .find(|(k, _)| k.as_text() == Some(key))
            .map(|(_, v)| v)
    };
    let collection = get("collection")
        .and_then(Value::as_text)
        .ok_or("emission names no collection")?;
    let fields = get("fields")
        .and_then(Value::as_map)
        .ok_or("emission has no fields map")?
        .iter()
        .map(|(name, value)| {
            let name = name
                .as_text()
                .ok_or("emission field name is not text")?
                .to_string();
            Ok((name, normal_value(value)?))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Emission {
        collection: collection.to_string(),
        fields,
    })
}

fn parse_awaited(value: &Value) -> Result<Awaited, String> {
    let map = value.as_map().ok_or("awaited entry is not a map")?;
    let (key, inner) = map.first().ok_or("awaited entry is empty")?;
    match key.as_text() {
        Some("composite") => {
            let cid = inner.as_text().ok_or("awaited composite is not text")?;
            Ok(Awaited::Composite(
                cid.parse().map_err(|_| "awaited composite is not a CID")?,
            ))
        }
        Some("immutable_field") => {
            let fields = inner.as_map().ok_or("awaited field is not a map")?;
            let field_of = |name: &str| {
                fields
                    .iter()
                    .find(|(k, _)| k.as_text() == Some(name))
                    .map(|(_, v)| v)
                    .ok_or(format!("awaited field lacks {name}"))
            };
            let collection = field_of("collection")?
                .as_text()
                .ok_or("awaited collection is not text")?;
            let field = field_of("field")?
                .as_text()
                .ok_or("awaited field name is not text")?;
            let value = normal_value(field_of("value")?)?;
            Ok(Awaited::immutable_field(collection, field, value))
        }
        _ => Err("awaited entry names neither a composite nor a field".to_string()),
    }
}

enum Fetched {
    Value(Value),
    Defer(MergeVerdict),
}

async fn fetch(view: &dyn MergeView, key: &str) -> Result<Fetched, String> {
    let (kind, rest) = key
        .split_once(':')
        .ok_or_else(|| format!("rule key {key} has no kind"))?;
    Ok(match kind {
        "fields" => {
            let cid: Cid = rest
                .parse()
                .map_err(|_| format!("rule key {key} is not a CID"))?;
            match view.composite_fields(&cid).await? {
                Some(fields) => Fetched::Value(fields_value(fields)),
                None => Fetched::Defer(MergeVerdict::defer(
                    format!("composite {cid} not held"),
                    [cid],
                )),
            }
        }
        "genesis" => {
            let cid: Cid = rest
                .parse()
                .map_err(|_| format!("rule key {key} is not a CID"))?;
            match view.genesis(&cid).await? {
                Some(genesis) => Fetched::Value(text(&genesis.to_string())),
                None => Fetched::Defer(MergeVerdict::defer(
                    format!("an ancestor of {cid} is not held"),
                    std::iter::empty::<Awaited>(),
                )),
            }
        }
        "find" => {
            let mut parts = rest.splitn(3, ':');
            let (Some(collection), Some(field), Some(hex_value)) =
                (parts.next(), parts.next(), parts.next())
            else {
                return Err(format!(
                    "rule key {key} is not find:<collection>:<field>:<hex>"
                ));
            };
            let bytes =
                hex::decode(hex_value).map_err(|_| format!("rule key {key} value is not hex"))?;
            let value: NormalValue = ciborium::from_reader(bytes.as_slice())
                .map_err(|_| format!("rule key {key} value is not CBOR"))?;
            let ids = view.find_documents(collection, field, &value).await?;
            Fetched::Value(Value::Array(ids.iter().map(|id| text(id)).collect()))
        }
        "immutable" => {
            let (collection, doc_id) = rest
                .split_once(':')
                .ok_or_else(|| format!("rule key {key} is not immutable:<collection>:<doc_id>"))?;
            match view.immutable_fields(collection, doc_id).await? {
                Some(fields) => Fetched::Value(Value::Array(
                    fields
                        .into_iter()
                        .map(|(name, value)| Value::Array(vec![text(&name), cbor(&value)]))
                        .collect(),
                )),
                None => Fetched::Value(Value::Null),
            }
        }
        other => return Err(format!("rule key kind {other} is unknown")),
    })
}

fn fields_value(fields: Vec<(String, FieldValue)>) -> Value {
    Value::Array(
        fields
            .into_iter()
            .map(|(name, value)| {
                let value = match value {
                    FieldValue::Value(value) => Value::Map(vec![(text("value"), cbor(&value))]),
                    FieldValue::Encrypted => text("encrypted"),
                    FieldValue::NotHeld(cid) => {
                        Value::Map(vec![(text("not_held"), text(&cid.to_string()))])
                    }
                    FieldValue::Undecodable => text("undecodable"),
                };
                Value::Array(vec![text(&name), value])
            })
            .collect(),
    )
}

/// A `NormalValue` as CBOR, through its own serialisation.
fn cbor(value: &NormalValue) -> Value {
    let mut bytes = Vec::new();
    if ciborium::into_writer(value, &mut bytes).is_err() {
        return Value::Null;
    }
    ciborium::from_reader(bytes.as_slice()).unwrap_or(Value::Null)
}

fn normal_value(value: &Value) -> Result<NormalValue, String> {
    let mut bytes = Vec::new();
    ciborium::into_writer(value, &mut bytes).map_err(|error| error.to_string())?;
    ciborium::from_reader(bytes.as_slice())
        .map_err(|error| format!("value is not a field value: {error}"))
}

fn text(s: &str) -> Value {
    Value::Text(s.to_string())
}

fn opt_text(s: Option<&str>) -> Value {
    s.map(text).unwrap_or(Value::Null)
}
