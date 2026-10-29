use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

/// One WASM transform and its arguments.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct LensModule {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Arguments")]
    pub arguments: Value,
}

/// A lens: the transforms it applies in order.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lens {
    #[serde(rename = "Lenses", default, skip_serializing_if = "Option::is_none")]
    pub lenses: Option<Vec<LensModule>>,
}

impl Lens {
    /// Go's `model.Lens`, with each corpus module path (`bin/lenses/<name>.wasm`)
    /// resolved to the module Go's `tests/lenses` builds.
    pub fn to_wire(&self) -> Value {
        let modules: Vec<Value> = self
            .lenses
            .iter()
            .flatten()
            .map(|module| json!({ "Path": module_path(&module.path), "Arguments": module.arguments }))
            .collect();
        json!({ "Lenses": modules })
    }
}

/// Where Go's lens module `path` names is built: `DEFRADB_GO_REPO`, else the
/// Go checkout under `$HOME/go/src`. A path that is not a corpus name is kept.
fn module_path(path: &str) -> String {
    let Some(name) = path
        .strip_prefix("bin/lenses/")
        .and_then(|file| file.strip_suffix(".wasm"))
    else {
        return path.to_owned();
    };
    let crate_name = format!("rust_wasm32_{}", name.replace('-', "_"));
    let repo = std::env::var_os("DEFRADB_GO_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                .join("go/src/github.com/sourcenetwork/defradb")
        });
    repo.join("tests/lenses")
        .join(&crate_name)
        .join("target/wasm32-unknown-unknown/debug")
        .join(format!("{crate_name}.wasm"))
        .to_string_lossy()
        .into_owned()
}
