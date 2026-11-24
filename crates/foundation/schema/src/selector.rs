//! Which collection versions a request selects.
//!
//! Go runs one lookup, `getCollections` (`internal/db/collection.go:193-301`),
//! behind every surface that takes the `name` / `version_id` / `collection_id`
//! / `get_inactive` selectors: `GetCollections` and `RefreshViews` both call
//! it. This is that lookup, defined once for the same reason.
//!
//! The precedence in Go's candidate `switch` is load-bearing and is what the
//! two helpers below encode. A flat AND over the four selectors gets two cases
//! wrong: `collection_id` is a candidate selector only, so a name plus a
//! disagreeing collection id still returns the named collection; and `name`
//! with `get_inactive` deliberately misses the active-by-name case, which is
//! how Go reaches an inactive collection by name.

use crate::CollectionVersion;

/// Go's `GetCollectionsOptions`, as the collection lookup consumes them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CollectionSelector {
    /// Restrict to these collection names (None = every name).
    pub names: Option<Vec<String>>,
    /// Restrict to the collection version with this version id.
    pub version_id: Option<String>,
    /// Restrict to versions belonging to this collection id.
    pub collection_id: Option<String>,
    /// Include inactive collection versions.
    pub get_inactive: bool,
}

impl CollectionSelector {
    /// Select everything active.
    pub fn all() -> Self {
        Self::default()
    }

    /// Select only the named collections.
    pub fn with_names(names: Vec<String>) -> Self {
        Self {
            names: Some(names),
            ..Self::default()
        }
    }

    /// Whether this selects everything active, narrowing nothing.
    ///
    /// A caller that can answer the unnarrowed case more cheaply, or from a
    /// different source, uses this to tell that it may.
    pub fn is_unfiltered(&self) -> bool {
        self.names.is_none()
            && self.version_id.is_none()
            && self.collection_id.is_none()
            && !self.get_inactive
    }

    /// Whether inactive versions have to be loaded to answer this selection.
    ///
    /// A named version is returned whether or not it is active, so asking for
    /// one directly requires the full listing just as `get_inactive` does.
    /// When an active name selected the candidate first, the version id only
    /// filters that candidate and does not widen the lookup.
    pub fn needs_all_versions(&self) -> bool {
        self.get_inactive || self.resolves_by_version_lookup()
    }

    /// Whether this collection version is selected.
    pub fn selects(&self, collection: &CollectionVersion) -> bool {
        let version_matches = self
            .version_id
            .as_ref()
            .is_none_or(|id| &collection.version_id == id);
        let name_matches = self
            .names
            .as_ref()
            .is_none_or(|names| names.contains(&collection.name));
        let collection_matches = !self.applies_collection_id()
            || self
                .collection_id
                .as_ref()
                .is_none_or(|id| &collection.collection_id == id);
        let visible =
            self.get_inactive || collection.is_active || self.resolves_by_version_lookup();

        version_matches && name_matches && collection_matches && visible
    }

    /// Whether Go resolves this selection by looking the version id up
    /// directly, rather than using it only as a filter.
    ///
    /// The candidate switch tries the active-by-name arm first, so a name with
    /// `get_inactive` false wins and the version id never reaches the lookup
    /// (`internal/db/collection.go:203-215`). The distinction is load-bearing:
    /// the direct lookup propagates not-found, where the stage-two filter just
    /// yields an empty selection.
    pub fn resolves_by_version_lookup(&self) -> bool {
        self.version_id.is_some() && (self.get_inactive || self.names.is_none())
    }

    /// Go picks candidates by collection id only when neither a name nor a
    /// version already picked them, so those two take precedence over it.
    fn applies_collection_id(&self) -> bool {
        (self.get_inactive || self.names.is_none()) && self.version_id.is_none()
    }

    /// The versions of `versions` this selector picks, in the order Go's
    /// storage keys give them: active versions by name; with inactive ones,
    /// by version ID, except that a collection ID lists its root version
    /// first (`internal/db/description/collection.go`,
    /// `getCollectionVersionIDs`).
    ///
    /// `None` when a lone version lookup misses: Go propagates not-found
    /// only from that direct lookup (`internal/db/collection.go:210-215`), so
    /// `version_id=nope` alone is an error while `name=Users&version_id=nope`
    /// is an empty answer.
    pub fn select(&self, versions: Vec<CollectionVersion>) -> Option<Vec<CollectionVersion>> {
        if let Some(version_id) = self
            .version_id
            .as_ref()
            .filter(|_| self.resolves_by_version_lookup())
        {
            if !versions.iter().any(|v| &v.version_id == version_id) {
                return None;
            }
        }
        let mut selected: Vec<_> = versions
            .into_iter()
            .filter(|version| self.selects(version))
            .collect();
        match (self.get_inactive, &self.collection_id) {
            (false, _) => {
                selected.sort_by(|a, b| (&a.name, &a.version_id).cmp(&(&b.name, &b.version_id)))
            }
            (true, Some(root)) => selected.sort_by(|a, b| {
                (&a.version_id != root, &a.version_id).cmp(&(&b.version_id != root, &b.version_id))
            }),
            (true, None) => selected.sort_by(|a, b| a.version_id.cmp(&b.version_id)),
        }
        Some(selected)
    }
}
