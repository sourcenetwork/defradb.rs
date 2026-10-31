//! KV snapshot export and import (Go Shinzo's `ExportDocKVs`,
//! `ImportRawKVsWithMapping`), through the node's core. Go reaches them only
//! in-process, so a Go node is stopped and `kvsnap`
//! (`tools/go-shinzo-kv`, at `DEFRA_GO_SHINZO_KVSNAP`) runs them against its
//! data directory, then the node starts again.

use std::path::{Path, PathBuf};
use std::process::Command;

use crate::scenario::{Common, Failure, Op, Text};
use crate::test_case::State;
use db::kv_snapshot::{ExportOptions, FieldMapping};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

/// Where snapshot `name` of this test process lives; its mapping is beside it.
fn snapshot_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("defra-kv-{}-{name}", std::process::id()))
}

fn mapping_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".mapping.json");
    PathBuf::from(name)
}

/// The Rust core of node `name`, whatever client reaches it.
fn core(state: &State, name: &str) -> Result<std::sync::Arc<defra_node::Node>, Failure> {
    state
        .node(name)?
        .core()
        .ok_or_else(|| Failure::operation(format!("node {name} has no Rust core")))
}

fn failed<E>(error: E) -> Failure
where
    E: std::fmt::Display,
{
    Failure::operation(error.to_string())
}

/// Run `kvsnap` against Go node `name` with it stopped, then start it again.
fn kvsnap(state: &mut State, name: &str, args: &[String]) -> Result<String, Failure> {
    let binary = std::env::var_os("DEFRA_GO_SHINZO_KVSNAP").ok_or_else(|| {
        Failure::operation("DEFRA_GO_SHINZO_KVSNAP names no kvsnap binary".to_owned())
    })?;
    let dir = state.node_dir(name).ok_or_else(|| {
        Failure::operation(format!(
            "Go node {name} keeps no data on disk: give it databaseDir"
        ))
    })?;
    state.stop_node(name, true);
    let output = Command::new(binary)
        .arg(&args[0])
        .arg(&dir)
        .args(&args[1..])
        .output()
        .map_err(failed);
    state.restart_node(name)?;
    let output = output?;
    if !output.status.success() {
        return Err(Failure::operation(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

/// The `kv.export` op: write `docIDs` of `collection` to snapshot `file`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvExport {
    pub collection: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub doc_ids: Vec<Text>,
    pub file: String,
    /// Go's blocks only: each head and the blocks it links (a Go node never
    /// writes more).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub without_history: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub datastore_only: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for KvExport {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let path = snapshot_path(&self.file);
        let doc_ids: Vec<String> = self
            .doc_ids
            .iter()
            .map(|id| id.as_str().to_owned())
            .collect();
        let name = self.common.node();
        let count = match state.is_go_node(name) {
            true => {
                if self.datastore_only {
                    return Err(Failure::unsupported("kv.export datastoreOnly on a Go node"));
                }
                let mut args = vec![
                    "export".to_owned(),
                    self.collection.clone(),
                    path.display().to_string(),
                ];
                args.extend(doc_ids);
                kvsnap(state, name, &args)?
                    .parse::<usize>()
                    .map_err(failed)?
            }
            false => {
                let node = core(state, name)?;
                let mut out = Vec::new();
                let exported = state
                    .block_on(node.database().export_kv_snapshot_as(
                        &state.context(None),
                        &self.collection,
                        &doc_ids,
                        ExportOptions {
                            datastore_only: self.datastore_only,
                            history: !self.without_history,
                        },
                        &mut out,
                    ))
                    .map_err(failed)?;
                std::fs::write(&path, out).map_err(failed)?;
                std::fs::write(
                    mapping_path(&path),
                    serde_json::to_vec(&exported.mapping).map_err(failed)?,
                )
                .map_err(failed)?;
                exported.frames
            }
        };
        if let Some(expected) = self.expected_count {
            if count != expected {
                return Err(Failure::operation(format!(
                    "expected {expected} exported frames; got {count}"
                )));
            }
        }
        Ok(json!({}))
    }
}

impl Action for KvExport {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// The `kv.import` op: load snapshot `file`, with its field mapping unless
/// `withoutMapping`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvImport {
    pub file: String,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub without_mapping: bool,
    /// A Go node rebuilds this collection's indexes after the import
    /// (`kv import --rebuild-indexes`); defradb.rs's merge keeps them.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rebuild_indexes: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_count: Option<usize>,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for KvImport {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let path = snapshot_path(&self.file);
        let mapping = mapping_path(&path);
        let name = self.common.node();
        let count = match state.is_go_node(name) {
            true => {
                let mut args = vec![
                    "import".to_owned(),
                    path.display().to_string(),
                    match self.without_mapping {
                        true => "-".to_owned(),
                        false => mapping.display().to_string(),
                    },
                ];
                args.extend(self.rebuild_indexes.clone());
                kvsnap(state, name, &args)?
                    .parse::<usize>()
                    .map_err(failed)?
            }
            false => {
                let node = core(state, name)?;
                let mapping: Option<FieldMapping> = match self.without_mapping {
                    true => None,
                    false => Some(
                        serde_json::from_slice(&std::fs::read(&mapping).map_err(failed)?)
                            .map_err(failed)?,
                    ),
                };
                let input = std::fs::File::open(&path).map_err(failed)?;
                state
                    .block_on(node.database().import_kv_snapshot_as(
                        &state.context(None),
                        std::io::BufReader::new(input),
                        mapping.as_ref(),
                    ))
                    .map_err(failed)?
                    .frames
            }
        };
        if let Some(expected) = self.expected_count {
            if count != expected {
                return Err(Failure::operation(format!(
                    "expected {expected} imported frames; got {count}"
                )));
            }
        }
        Ok(json!({}))
    }
}

impl Action for KvImport {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}

/// The `kv.compare` op: snapshots `left` and `right` hold the same bytes,
/// and their mappings name the same collection and fields.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvCompare {
    pub left: String,
    pub right: String,
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for KvCompare {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run(&self, _state: &State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let read = |name: &str| std::fs::read(snapshot_path(name)).map_err(failed);
        let (left, right) = (read(&self.left)?, read(&self.right)?);
        if left != right {
            let at = left.iter().zip(&right).take_while(|(l, r)| l == r).count();
            return Err(Failure::operation(format!(
                "snapshots {} ({} bytes) and {} ({} bytes) differ from byte {at}",
                self.left,
                left.len(),
                self.right,
                right.len()
            )));
        }
        let mapping = |name: &str| -> Result<FieldMapping, Failure> {
            let bytes = std::fs::read(mapping_path(&snapshot_path(name))).map_err(failed)?;
            let mut mapping: FieldMapping = serde_json::from_slice(&bytes).map_err(failed)?;
            mapping.documents.clear();
            Ok(mapping)
        };
        if mapping(&self.left)? != mapping(&self.right)? {
            return Err(Failure::operation(format!(
                "the mappings of {} and {} differ",
                self.left, self.right
            )));
        }
        Ok(json!({}))
    }
}

impl Action for KvCompare {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
