use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};

use crate::did::Did;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NodeId(String);

impl NodeId {
    pub(crate) fn new(resource: &str, object_id: &str, relation: &str) -> Self {
        Self(format!("{resource}/{object_id}#{relation}"))
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct NodeTrail {
    visited: RapidHashSet<NodeId>,
}

impl NodeTrail {
    pub(crate) fn new() -> Self {
        Self {
            visited: RapidHashSet::new(),
        }
    }

    pub(crate) fn contains(&self, node: &NodeId) -> bool {
        self.visited.contains(node)
    }

    pub(crate) fn insert(&mut self, node: NodeId) {
        self.visited.insert(node);
    }

    pub(crate) fn with_node(&self, node: NodeId) -> Self {
        let mut new_trail = self.clone();
        new_trail.insert(node);
        new_trail
    }
}

pub(crate) struct CheckCache {
    results: HopscotchMap<String, bool, RandomState>,
}

impl Default for CheckCache {
    fn default() -> Self {
        Self {
            results: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl CheckCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    fn cache_key(resource: &str, object_id: &str, relation: &str, subject: &Did) -> String {
        format!("{resource}/{object_id}#{relation}@{subject}")
    }

    pub(crate) fn get(
        &self,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
    ) -> Option<bool> {
        let key = Self::cache_key(resource, object_id, relation, subject);
        self.results.get(&key)
    }

    pub(crate) fn set(
        &self,
        resource: &str,
        object_id: &str,
        relation: &str,
        subject: &Did,
        result: bool,
    ) {
        let key = Self::cache_key(resource, object_id, relation, subject);
        self.results.insert(key, result);
    }
}
