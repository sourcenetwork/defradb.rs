//! KV snapshot import (Go Shinzo's `ImportRawKVsWithMapping`) through the
//! node's core, from a snapshot a Go Shinzo node exported:
//! `tests/shinzo/fixtures/<file>`, its field mapping beside it as
//! `<file>.mapping.json` (`tools/go-shinzo-kv/fixtures.sh` wrote them).

use std::path::PathBuf;

use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;
use db::kv_snapshot::FieldMapping;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/shinzo/fixtures")
        .join(name)
}

fn failed<E>(error: E) -> Failure
where
    E: std::fmt::Display,
{
    Failure::operation(error.to_string())
}

/// The `kv.import` op: load fixture snapshot `file` with its field mapping.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KvImport {
    pub file: String,
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
        let name = self.common.node();
        let node = state
            .node(name)?
            .core()
            .ok_or_else(|| Failure::operation(format!("node {name} has no Rust core")))?;
        let path = fixture(&self.file);
        let mapping: FieldMapping = serde_json::from_slice(
            &std::fs::read(fixture(&format!("{}.mapping.json", self.file))).map_err(failed)?,
        )
        .map_err(failed)?;
        let input = std::fs::File::open(&path).map_err(failed)?;
        let imported = state
            .block_on(node.database().import_kv_snapshot_as(
                &state.context(None),
                std::io::BufReader::new(input),
                Some(&mapping),
            ))
            .map_err(failed)?;
        Ok(json!({ "frames": imported.frames, "documents": imported.documents }))
    }
}

impl Action for KvImport {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
