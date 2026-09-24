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
//! A module that traps, exceeds its fuel, or answers malformed CBOR is an
//! error, not a verdict: the composite stays unmerged and the sweep will
//! try again. A module that asks for more steps than the budget allows is
//! the same.

use std::sync::Arc;

use async_trait::async_trait;
use ciborium::Value;
use cid::Cid;
use defra_core::thread_bounds::MaybeSendSync;
use document::NormalValue;
use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use wasmtime::{Config, Engine, Instance, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::awaited::Awaited;
use super::signature::SignatureStatus;
use super::validator::{DefinitionCandidate, MergeCandidate, MergeValidator};
use super::verdict::MergeVerdict;
use super::view::{FieldValue, MergeView};

/// Where rule modules come from, by the CID the version names.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
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
}

#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
impl<B: blockstore::Blockstore + 'static> RuleModules for BlockstoreModules<B> {
    async fn module(&self, cid: &Cid) -> Result<Option<Vec<u8>>, String> {
        self.blockstore
            .get(cid)
            .await
            .map(|bytes| bytes.map(|bytes| bytes.to_vec()))
            .map_err(|error| error.to_string())
    }
}

/// What one verdict may cost.
#[derive(Debug, Clone, Copy)]
pub struct RuleBudget {
    /// Fuel per step: roughly one unit per wasm instruction.
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
    engine: Engine,
    modules: Arc<dyn RuleModules>,
    compiled: HopscotchMap<Cid, Arc<Module>, RandomState>,
    budget: RuleBudget,
}

impl WasmRules {
    pub fn new(modules: Arc<dyn RuleModules>) -> Result<Self, String> {
        Self::with_budget(modules, RuleBudget::default())
    }

    pub fn with_budget(modules: Arc<dyn RuleModules>, budget: RuleBudget) -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        let engine = Engine::new(&config).map_err(|error| error.to_string())?;
        Ok(Self {
            engine,
            modules,
            compiled: HopscotchMap::with_hasher(RandomState::default()),
            budget,
        })
    }

    /// The compiled module a version names, `Ok(None)` when its bytes are
    /// not held here.
    async fn module_for(&self, rule: &str) -> Result<Option<Arc<Module>>, String> {
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
            Module::new(&self.engine, &bytes)
                .map_err(|error| format!("rule module {cid} does not compile: {error}"))?,
        );
        self.compiled.insert(cid, module.clone());
        Ok(Some(module))
    }

    /// One run of the module over `request`.
    fn step(&self, module: &Module, request: &[u8]) -> Result<Value, String> {
        let limits = StoreLimitsBuilder::new()
            .memory_size(self.budget.memory_bytes)
            .build();
        let mut store: Store<StoreLimits> = Store::new(&self.engine, limits);
        store.limiter(|limits| limits);
        store
            .set_fuel(self.budget.fuel)
            .map_err(|error| error.to_string())?;
        let instance = Instance::new(&mut store, module, &[])
            .map_err(|error| format!("rule module does not instantiate: {error}"))?;
        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or("rule module exports no memory")?;
        let alloc = instance
            .get_typed_func::<i32, i32>(&mut store, "alloc")
            .map_err(|error| format!("rule module exports no alloc: {error}"))?;
        let judge = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "judge")
            .map_err(|error| format!("rule module exports no judge: {error}"))?;

        let len = i32::try_from(request.len()).map_err(|_| "request too large")?;
        let ptr = alloc
            .call(&mut store, len)
            .map_err(|error| format!("rule module alloc failed: {error}"))?;
        memory
            .write(&mut store, ptr as usize, request)
            .map_err(|error| format!("rule module memory write failed: {error}"))?;
        let out = judge
            .call(&mut store, (ptr, len))
            .map_err(|error| format!("rule module trapped: {error}"))?;
        let mut header = [0u8; 4];
        memory
            .read(&store, out as usize, &mut header)
            .map_err(|error| format!("rule module response unreadable: {error}"))?;
        let out_len = u32::from_le_bytes(header) as usize;
        let mut response = vec![0u8; out_len];
        memory
            .read(&store, out as usize + 4, &mut response)
            .map_err(|error| format!("rule module response unreadable: {error}"))?;
        ciborium::from_reader(response.as_slice())
            .map_err(|error| format!("rule module response is not CBOR: {error}"))
    }

    /// Run the step protocol to a verdict.
    async fn judge(
        &self,
        rule: Option<&str>,
        kind: &str,
        candidate: Value,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
        let Some(rule) = rule else {
            return Ok(MergeVerdict::reject("the version names no rule"));
        };
        let Some(module) = self.module_for(rule).await? else {
            // A block, not a composite: nothing merges to release it, so the
            // sweep is what re-judges this.
            return Ok(MergeVerdict::defer(
                format!("rule module {rule} not held"),
                std::iter::empty::<Awaited>(),
            ));
        };
        let mut inputs: Vec<(Value, Value)> = Vec::new();
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
                Response::Verdict(verdict) => return Ok(verdict),
                Response::Need(keys) => {
                    for key in keys {
                        match fetch(view, &key).await? {
                            Fetched::Value(value) => inputs.push((text(&key), value)),
                            Fetched::Defer(verdict) => return Ok(verdict),
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
impl MergeValidator for WasmRules {
    async fn validate(
        &self,
        candidate: &MergeCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
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
        self.judge(
            candidate.collection.governance_rule.as_deref(),
            "composite",
            value,
            view,
        )
        .await
    }

    async fn validate_definition(
        &self,
        candidate: &DefinitionCandidate<'_>,
        view: &dyn MergeView,
    ) -> Result<MergeVerdict, String> {
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
        // A patch is judged by the rule it names; an initial definition by
        // its own. Either way the module is the one the version commits to.
        self.judge(
            candidate.version.governance_rule.as_deref(),
            "definition",
            value,
            view,
        )
        .await
    }
}

enum Response {
    Verdict(MergeVerdict),
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
    Ok(match verdict {
        "accept" => Response::Verdict(MergeVerdict::Accept),
        "reject" => Response::Verdict(MergeVerdict::reject(reason())),
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
            Response::Verdict(MergeVerdict::defer(reason(), awaiting))
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
