//! The process-wide collection cache: each collection's current version,
//! stored by identity and reachable by name and by version.

use rapidhash::RapidHashMap;

use crate::collection::Collection;

/// Whether [`CollectionMap::offer`] took the collection it was given.
///
/// The attribute sits on the type rather than on the method deliberately: a
/// method's `#[must_use]` is satisfied by the `map_err` every caller applies,
/// and the value falling out of `?` is then an ordinary expression statement
/// that nothing lints. A `#[must_use]` type is linted wherever it is dropped.
#[must_use]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cached {
    /// The collection is now the cached entry for its name.
    Taken,
    /// Another collection holds that name, so the cache was left alone.
    NameHeldByAnother,
}

/// Each collection's current version, keyed by collection ID.
///
/// A collection's identity is its collection ID; its name is a label that a
/// patch can change and that two collections can share, so the name cannot be
/// the key. Entries are stored by ID, with a name index and a version index
/// kept beside them, so all three lookups are direct.
///
/// Names and entries are one-to-one: every entry is reachable by exactly one
/// name, and a name reaches at most one entry. GraphQL resolves a collection
/// by name, so a name has to answer with a single collection; the two ways of
/// writing an entry differ only in how they settle a name that is taken.
#[derive(Debug, Clone, Default)]
pub struct CollectionMap {
    by_id: RapidHashMap<String, Collection>,
    id_by_name: RapidHashMap<String, String>,
    id_by_version: RapidHashMap<String, String>,
}

impl CollectionMap {
    /// Record `collection` as the current version under its own name.
    ///
    /// For a version this node has committed as current, where the schema's
    /// name is the registered name: creation, and a rename the node itself
    /// completed. Callers caching a definition whose name is not (yet) the
    /// registered lookup name use [`CollectionMap::put_named`] instead.
    pub fn put(&mut self, collection: Collection) {
        let name = collection.name().to_string();
        self.take_name(&name, collection);
    }

    /// Record `collection` as the current version under `name`.
    ///
    /// `name` is the registered lookup name, which can differ from the
    /// schema's own name: a placeholder whose synced definition carries the
    /// eventual name, or a reload reading the name index before it has moved.
    /// The entry is keyed by collection ID either way, so the two spellings
    /// cannot fork the cache.
    pub fn put_named(&mut self, name: &str, collection: Collection) {
        self.take_name(name, collection);
    }

    /// Offer `collection` for its name without displacing another collection.
    ///
    /// For a definition that arrived from elsewhere, and so may merely share a
    /// name with a collection this node already has. It takes the name when
    /// the name is free, already names the same collection, or names a
    /// placeholder standing in for a definition that has not arrived.
    pub fn offer(&mut self, collection: Collection) -> Cached {
        let name = collection.name().to_string();
        if let Some(holder) = self.get(&name) {
            let holder = holder.schema();
            if holder.collection_id != collection.collection_id() && !holder.is_placeholder {
                return Cached::NameHeldByAnother;
            }
        }
        self.take_name(&name, collection);
        Cached::Taken
    }

    /// Remove the collection holding `name`, returning it.
    pub fn remove(&mut self, name: &str) -> Option<Collection> {
        let id = self.id_by_name.remove(name)?;
        let entry = self.by_id.remove(&id)?;
        self.id_by_version.remove(entry.version_id());
        Some(entry)
    }

    /// The collection holding `name`.
    pub fn get(&self, name: &str) -> Option<&Collection> {
        self.by_id.get(self.id_by_name.get(name)?)
    }

    /// Whether a collection holds `name`.
    pub fn contains_name(&self, name: &str) -> bool {
        self.id_by_name.contains_key(name)
    }

    /// The collection with `collection_id`.
    pub fn by_id(&self, collection_id: &str) -> Option<&Collection> {
        self.by_id.get(collection_id)
    }

    /// The collection whose cached version is `version_id`.
    ///
    /// Only the current version of each collection is cached, so a superseded
    /// version is not found here; reading those goes to the store.
    pub fn by_version(&self, version_id: &str) -> Option<&Collection> {
        self.by_id.get(self.id_by_version.get(version_id)?)
    }

    pub fn len(&self) -> usize {
        self.by_id.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_id.is_empty()
    }

    /// Every name that holds a collection.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.id_by_name.keys()
    }

    /// Every cached collection.
    pub fn values(&self) -> impl Iterator<Item = &Collection> {
        self.by_id.values()
    }

    /// Every cached collection with the name that reaches it.
    pub fn iter(&self) -> impl Iterator<Item = (&String, &Collection)> {
        self.id_by_name
            .iter()
            .filter_map(|(name, id)| self.by_id.get(id).map(|entry| (name, entry)))
    }

    /// The collections by name, for a snapshot taken at this moment.
    pub fn by_name(&self) -> RapidHashMap<String, Collection> {
        self.iter()
            .map(|(name, entry)| (name.clone(), entry.clone()))
            .collect()
    }

    /// Claim `name` for `collection`'s ID.
    ///
    /// Whoever held `name` loses the name but keeps its entry, reachable by
    /// ID and by version: a name collision must not delete another
    /// collection's data, only `offer` decides whether to take a held name at
    /// all. This collection's previous name — an old spelling after a rename
    /// — is dropped so an entry stays reachable by exactly one name.
    fn take_name(&mut self, name: &str, collection: Collection) {
        let id = collection.collection_id().to_string();
        if let Some(previous) = self.by_id.remove(&id) {
            self.id_by_version.remove(previous.version_id());
            if self.id_by_name.get(previous.name()).map(String::as_str) == Some(id.as_str())
                && previous.name() != name
            {
                self.id_by_name.remove(previous.name());
            }
        }
        self.id_by_name.insert(name.to_string(), id.clone());
        self.id_by_version
            .insert(collection.version_id().to_string(), id.clone());
        self.by_id.insert(id, collection);
    }
}
