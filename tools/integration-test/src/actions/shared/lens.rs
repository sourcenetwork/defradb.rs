use std::path::{Path, PathBuf};

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
    /// resolved to the module `tools/lenses` builds.
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

/// Where `just build-lenses` puts the module `path` names. A path that is not
/// a corpus name is kept.
fn module_path(path: &str) -> String {
    let Some(name) = path
        .strip_prefix("bin/lenses/")
        .and_then(|file| file.strip_suffix(".wasm"))
    else {
        return path.to_owned();
    };
    // A `file://` URL, as Go's lens host requires and Rust accepts.
    format!(
        "file://{}",
        lens_module_file(&name.replace('-', "_")).display()
    )
}

/// The built WASM module of `tools/lenses/rust_wasm32_<name>`.
fn lens_module_file(name: &str) -> PathBuf {
    // Lens hosts refuse a module path with a `..` segment.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("integration-test lives under tools/")
        .join("lenses/target/wasm32-unknown-unknown/debug")
        .join(format!("rust_wasm32_{name}.wasm"))
}
