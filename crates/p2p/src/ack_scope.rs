//! The scope at which a replicator is pushed heads and acknowledges them.

/// Whether a replicator's pushes, and so its acknowledgements, cover a
/// collection's heads or each document's heads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AckScope {
    /// The collection's heads, for every document in it at once.
    Collection,
    /// Each document's own heads.
    Document,
}

impl AckScope {
    /// An unfiltered replicator of a `@branchable` collection is pushed the
    /// collection heads. A filtered one is pushed only the documents its
    /// filter matches, so it never receives collection heads.
    pub fn new(filtered: bool, branchable: bool) -> Self {
        if branchable && !filtered {
            Self::Collection
        } else {
            Self::Document
        }
    }
}
