//! The datastore backend names a config file, a CLI flag or a C caller may give.

/// Where a node keeps its data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreBackend {
    /// A regolith database on disk.
    Regolith,
    /// A regolith database kept in memory.
    Memory,
}

/// Resolves a backend name, case-insensitively; `None` for anything else.
///
/// `badger` is Go's name for its persistent store, accepted as regolith because
/// the cross-runtime tests hand one `--store` string to both binaries. The
/// backends regolith replaced (`lark`, `redb`, `fjall`, `rocksdb`, `leveldb`)
/// are refused: their stores are in a format regolith cannot read.
pub fn parse_store_backend(name: &str) -> Option<StoreBackend> {
    match name.to_lowercase().as_str() {
        "memory" => Some(StoreBackend::Memory),
        "regolith" | "badger" => Some(StoreBackend::Regolith),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn regolith_and_badger_resolve_to_regolith() {
        for name in ["regolith", "badger", "Regolith"] {
            assert_eq!(
                parse_store_backend(name),
                Some(StoreBackend::Regolith),
                "{name}"
            );
        }
    }

    #[test]
    fn memory_resolves_and_removed_and_unknown_names_are_refused() {
        assert_eq!(parse_store_backend("memory"), Some(StoreBackend::Memory));
        for name in [
            "", "lark", "redb", "fjall", "rocksdb", "leveldb", "RocksDB", "postgres",
        ] {
            assert_eq!(parse_store_backend(name), None, "{name}");
        }
    }
}
