use std::path::Path;

use rapidhash::RapidHashMap;

use super::error::{io, GenError, Result};

/// Maps `path::TestName` (path below `tests/integration/`) to the mantra leaf
/// that lists it.
pub fn load(dir: &Path) -> Result<RapidHashMap<String, String>> {
    let mut leaves: RapidHashMap<String, String> = RapidHashMap::default();
    for entry in io(dir, std::fs::read_dir(dir))? {
        let path = io(dir, entry)?.path();
        if path.extension().is_none_or(|e| e != "md") {
            continue;
        }
        let text = io(&path, std::fs::read_to_string(&path))?;
        let mut current = String::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix("# `") {
                current = rest.split('`').next().unwrap_or_default().to_owned();
            } else if let Some(list) = line.strip_prefix("- **Go-Tests:** ") {
                let tests: Vec<String> = serde_json::from_str(list)
                    .map_err(|e| GenError::Mantra(format!("{}: {e}", path.display())))?;
                for test in tests {
                    leaves.insert(test, current.clone());
                }
            }
        }
    }
    Ok(leaves)
}

pub fn key(source_path: &str, test: &str) -> String {
    let path = source_path
        .strip_prefix("tests/integration/")
        .unwrap_or(source_path);
    format!("{path}::{test}")
}
