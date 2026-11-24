use crate::error::Result;
use crate::planner::{Doc, ExecInfo};

use super::node::TypeJoinMany;

impl TypeJoinMany {
    /// Attach the children of a parent read outside this join's parent plan.
    ///
    /// The children are the ones the join attaches to a parent it scans: a
    /// child scan for this parent in per-parent mode, the child cache
    /// otherwise, built from the child plan on the first parent. `reads` gains
    /// what the lookups read. `Ok(false)` means the relation filter rejects the
    /// parent.
    pub(super) async fn join_children_of_fetched(
        &mut self,
        doc: &mut Doc,
        reads: &mut ExecInfo,
    ) -> Result<bool> {
        if !self.fetched_child_cache_built {
            if let Some(filter_reads) = self.build_filter_child_cache(None).await? {
                reads.merge(&filter_reads);
            }
            if !self.per_parent_child_scan {
                self.build_child_cache(None).await?;
                reads.merge(&self.child_exec_info);
            }
            self.fetched_child_cache_built = true;
        }

        let Some(parent_doc_id) = doc.doc_id().map(String::from) else {
            if self.relation_filter.is_some() {
                return Ok(false);
            }
            self.merge_children(doc, Vec::new());
            return Ok(true);
        };

        let children = if self.per_parent_child_scan {
            let before = self.go_child_metrics.clone();
            let mut children = self.scan_children_of(&parent_doc_id).await?;
            reads.iterations += self.go_child_metrics.iterations - before.iterations;
            reads.docs_fetched += self.go_child_metrics.doc_fetches - before.doc_fetches;
            reads.fields_fetched += self.go_child_metrics.field_fetches - before.field_fetches;
            reads.indexes_fetched += self.go_child_metrics.index_fetches - before.index_fetches;
            if !self.parent_passes_filter(&parent_doc_id, &children)? {
                return Ok(false);
            }
            self.order_children(&mut children);
            children
        } else {
            let all_children = self
                .child_cache
                .get(&parent_doc_id)
                .map(Vec::as_slice)
                .unwrap_or_default();
            if !self.parent_passes_filter(&parent_doc_id, all_children)? {
                return Ok(false);
            }
            self.find_child_docs(&parent_doc_id)
        };

        self.merge_children(doc, children);
        Ok(true)
    }
}
