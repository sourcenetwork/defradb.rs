//! The datastore backend names a config file, a CLI flag or a C caller may give.

/// Where a node keeps its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreBackend {
    /// A regolith database on disk.
    Regolith,
    /// A regolith database kept in memory.
    Memory,
}

/// Names of backends regolith replaced, plus Go's `badger`. Each resolves to
/// regolith so an existing config file keeps loading.
const LEGACY_NAMES: [&str; 5] = ["lark", "redb", "fjall", "rocksdb", "badger"];

/// Resolves a backend name, case-insensitively; `None` for anything unknown.
pub fn parse_store_backend(name: &str) -> Option<StoreBackend> {
    let name = name.to_lowercase();
    match name.as_str() {
        "memory" => Some(StoreBackend::Memory),
        "regolith" => Some(StoreBackend::Regolith),
        legacy if LEGACY_NAMES.contains(&legacy) => Some(StoreBackend::Regolith),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_names_resolve_to_regolith() {
        for name in [
            "regolith", "lark", "redb", "fjall", "rocksdb", "badger", "RocksDB",
        ] {
            assert_eq!(
                parse_store_backend(name),
                Some(StoreBackend::Regolith),
                "{name}"
            );
        }
    }

    #[test]
    fn memory_and_unknown_names() {
        assert_eq!(parse_store_backend("memory"), Some(StoreBackend::Memory));
        for name in ["", "leveldb", "postgres"] {
            assert_eq!(parse_store_backend(name), None, "{name}");
        }
    }
}
