use std::path::{Path, PathBuf};

use integration_test::export::WireStep;
use serde::Deserialize;

use super::error::{io, GenError, Result};

#[derive(Debug, Deserialize)]
pub struct Source {
    pub path: String,
    pub test: String,
    #[serde(default)]
    pub subtest: Option<String>,
    pub revision: String,
    pub license: String,
    pub copyright: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Scenario {
    pub id: String,
    pub source: Source,
    pub steps: Vec<WireStep>,
    #[serde(default)]
    pub upstream_skip: Option<String>,
}

/// A scenario and where the corpus keeps it.
#[derive(Debug)]
pub struct Loaded {
    pub scenario: Scenario,
    /// Directory components below the corpus root, e.g. `["query", "simple"]`.
    pub dir: Vec<String>,
    /// The scenario file's name without `.json`.
    pub stem: String,
}

pub fn load(root: &Path) -> Result<Vec<Loaded>> {
    let mut files = Vec::new();
    collect(root, &mut files)?;
    files.sort();
    files
        .into_iter()
        .map(|file| {
            let text = io(&file, std::fs::read_to_string(&file))?;
            let scenario: Scenario =
                serde_json::from_str(&text).map_err(|source| GenError::Json {
                    path: file.clone(),
                    source,
                })?;
            let relative = file.strip_prefix(root).unwrap_or(&file);
            let mut dir: Vec<String> = relative
                .iter()
                .map(|c| c.to_string_lossy().into_owned())
                .collect();
            let name = dir.pop().unwrap_or_default();
            let stem = name.trim_end_matches(".json").to_owned();
            Ok(Loaded {
                scenario,
                dir,
                stem,
            })
        })
        .collect()
}

fn collect(dir: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in io(dir, std::fs::read_dir(dir))? {
        let path = io(dir, entry)?.path();
        if path.is_dir() {
            collect(&path, files)?;
        } else if path.extension().is_some_and(|e| e == "json") {
            files.push(path);
        }
    }
    Ok(())
}
