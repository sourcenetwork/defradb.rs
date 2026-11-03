//! Collection types and operations

use std::hash::{Hash, Hasher};

/// Derive a short u32 ID from a collection_id string.
pub fn collection_short_id(collection_id: &str) -> u32 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    collection_id.hash(&mut hasher);
    hasher.finish() as u32
}

#[cfg(test)]
mod tests {
    use super::collection_short_id;

    #[test]
    fn collection_short_id_is_stable() {
        assert_eq!(collection_short_id("users"), 3731571252);
        assert_eq!(
            collection_short_id("bafyreihszin3nr7ja7ig3zpvxypv6h2pvf2kk2ul6w67qnx6n7fgslha6e"),
            2751466997
        );
    }
}
