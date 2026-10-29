use std::path::Path;

use rapidhash::RapidHashMap;

use super::error::{io, GenError, Result};

/// Maps each Go test (`path::TestName`, path below `tests/integration/`) and
/// each scenario ID to the mantra leaf that lists it: reqs/go leaves name
/// their Go tests, reqs/defra leaves their scenarios.
pub fn load<P>(dirs: &[P]) -> Result<RapidHashMap<String, String>>
where
    P: AsRef<Path>,
{
    let mut leaves: RapidHashMap<String, String> = RapidHashMap::default();
    for dir in dirs {
        load_dir(dir.as_ref(), &mut leaves)?;
    }
    Ok(leaves)
}

fn load_dir(dir: &Path, leaves: &mut RapidHashMap<String, String>) -> Result<()> {
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
            } else if let Some(list) = line
                .strip_prefix("- **Go-Tests:** ")
                .or_else(|| line.strip_prefix("- **Scenarios:** "))
            {
                let keys: Vec<String> = serde_json::from_str(list)
                    .map_err(|e| GenError::Mantra(format!("{}: {e}", path.display())))?;
                for key in keys {
                    leaves.insert(key, current.clone());
                }
            }
        }
    }
    Ok(())
}

pub fn key(source_path: &str, test: &str) -> String {
    let path = source_path
        .strip_prefix("tests/integration/")
        .unwrap_or(source_path);
    format!("{path}::{test}")
}
