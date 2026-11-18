//! Schema-aware multi-level relation paths of a filter.

use std::sync::Arc;

use schema::CollectionVersion;

use super::super::builder::Planner;
use crate::mapper::Filter;

impl Planner {
    /// Multi-level filter paths that run through relations, read against the schema.
    ///
    /// [`Filter::get_multi_level_relation_paths`] sees only the filter's shape, so a
    /// JSON sub-path such as `{jsonField: {i: {love: {_like: "%x%"}}}}` looks like a
    /// relation chain. Each path is cut at its first segment that is not a relation
    /// field; the rest is a scalar condition on that field. Paths left with a single
    /// segment are single-level relation filters and are dropped. A path that is
    /// all relations comes back unchanged (`proofs/lean/JsonSubPathCut.lean`).
    pub(in crate::planner) fn relation_filter_paths(
        &self,
        filter: &Filter,
        collection: &CollectionVersion,
    ) -> Vec<Vec<String>> {
        filter
            .get_multi_level_relation_paths()
            .into_iter()
            .map(|path| self.relation_prefix(&path, collection))
            .filter(|path| path.len() > 1)
            .collect()
    }

    fn relation_prefix(&self, path: &[String], collection: &CollectionVersion) -> Vec<String> {
        let mut target: Option<Arc<CollectionVersion>> = None;
        for (i, name) in path.iter().enumerate() {
            let current = target.as_deref().unwrap_or(collection);
            let Some(field) = current.field_by_name(name) else {
                return path.to_vec();
            };
            if !field.kind.is_relation() {
                return path[..i].to_vec();
            }
            match self.resolve_relation_target_collection(current, field, name) {
                Ok(next) => target = Some(next),
                Err(_) => return path.to_vec(),
            }
        }
        path.to_vec()
    }
}
