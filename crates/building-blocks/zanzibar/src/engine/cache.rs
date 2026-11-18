use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;
use rapidhash::{HashSetExt, RapidHashSet};

use crate::did::Did;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct NodeId(String, String, String);

impl NodeId {
    pub(crate) fn new(resource: &str, object_id: &str, relation: &str) -> Self {
        Self(
            resource.to_owned(),
            object_id.to_owned(),
            relation.to_owned(),
        )
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub(crate) struct CheckKey(String, NodeId, Did);

impl CheckKey {
    pub(crate) fn new(
        policy: &str,
        resource: &str,
        object: &str,
        relation: &str,
        subject: &Did,
    ) -> Self {
        Self(
            policy.to_owned(),
            NodeId::new(resource, object, relation),
            subject.clone(),
        )
    }
}

pub(crate) struct CheckCache {
    pub(crate) budget: super::limits::EvaluationBudget,
    results: HopscotchMap<CheckKey, bool, RandomState>,
}

impl Default for CheckCache {
    fn default() -> Self {
        Self {
            budget: Default::default(),
            results: HopscotchMap::with_hasher(RandomState::default()),
        }
    }
}

impl CheckCache {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get(&self, key: &CheckKey) -> Option<bool> {
        self.results.get(key)
    }

    pub(crate) fn set(&self, key: CheckKey, result: bool) {
        self.results.insert(key, result);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn node_identity_keeps_delimited_fields_separate() {
        let first = NodeId::new("resource", "object#nested", "relation");
        let second = NodeId::new("resource", "object", "nested#relation");
        assert_ne!(first, second);
        let trail = NodeTrail::new().with_node(first);
        assert!(!trail.contains(&second));
        assert_ne!(NodeId::new("a/b", "c", "d"), NodeId::new("a", "b/c", "d"));
    }
}
