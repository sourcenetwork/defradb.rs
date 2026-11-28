//! Secondary relation ID joins.
//!
//! Handles `_authorID`-style fields for secondary relations — creates
//! TypeJoinOne for reverse lookups when a secondary relation ID field
//! is selected but the relation object is not.

use std::collections::BTreeSet;
use std::sync::Arc;

use rapidhash::RapidHashSet;
use schema::CollectionVersion;
use tracing::debug;

use crate::document::DocumentMapping;
use crate::error::Result;
use crate::mapper::{Filter, Requestable, Select};
use crate::plan::{JoinSide, RelationFilter, ScanNode, TypeJoinOne};
use crate::planner::PlanNode;

use super::super::builder::Planner;

impl Planner {
    /// Handle secondary relation ID fields (e.g., `_authorID` for a secondary `author` relation).
    ///
    /// When a secondary relation ID field is selected but the relation object is not,
    /// we need to add a TypeJoin to compute the ID by doing a reverse lookup.
    pub(super) fn apply_secondary_id_joins(
        &self,
        mut plan: Box<dyn PlanNode>,
        mapping: &mut DocumentMapping,
        select: &Select,
        parent_collection: &CollectionVersion,
        synthetic_order_relations: &RapidHashSet<&str>,
    ) -> Result<Box<dyn PlanNode>> {
        let field_names = select
            .fields
            .iter()
            .filter_map(|requestable| match requestable {
                Requestable::Field(field) => Some(field.name.clone()),
                _ => None,
            })
            .chain(
                select
                    .filter
                    .as_ref()
                    .map(|filter| filter.referenced_fields())
                    .unwrap_or_default(),
            )
            .collect::<BTreeSet<_>>();

        for field_name in field_names {
            // Check if this is a relation ID field (pattern: `_{relationName}ID`)
            if !field_name.starts_with('_') || !field_name.ends_with("ID") {
                continue;
            }

            // Extract the relation name from the field name (e.g., "author" from "_authorID")
            let relation_name = &field_name[1..field_name.len() - 2];
            if relation_name.is_empty() {
                continue;
            }

            // Find the relation field in the parent collection
            let relation_field = match parent_collection.field_by_name(relation_name) {
                Some(f) => f,
                None => continue, // Not a valid relation name
            };

            // Check if it's a relation field and NOT primary (secondary relation)
            if !relation_field.kind.is_relation() || relation_field.is_primary {
                continue; // Only handle secondary relations
            }

            let selected_join_owns_id = select.fields.iter().any(|field| {
                matches!(field, Requestable::Select(select)
                    if select.field.name == relation_name
                        && select.filter.is_none()
                        && select.doc_ids.is_none())
            });
            let already_joined =
                synthetic_order_relations.contains(relation_name) || selected_join_owns_id;

            if already_joined {
                // An unfiltered selection or order join owns this relation and
                // fills its ID. A filtered selection does not: its child may be
                // hidden, but the parent ID still has to be evaluated.
                continue;
            }

            // Get the target collection
            let target_collection_id = match relation_field.kind.relation_collection_id() {
                Some(id) => id,
                None => continue,
            };

            let target_collection = if target_collection_id.is_empty() {
                Arc::new(parent_collection.clone())
            } else {
                match self.get_collection(target_collection_id) {
                    Some(c) => c,
                    None => continue,
                }
            };

            // Find the target relation field (the other side of the relation)
            let target_relation_field = if let Some(rel_name) = &relation_field.relation_name {
                target_collection.field_by_relation(
                    rel_name,
                    &parent_collection.name,
                    relation_name,
                )
            } else {
                None
            };

            // Build a minimal child mapping (just _docID for the reverse lookup).
            // Include render_key so the merged child renders with _docID for groupBy.
            let mut child_mapping = DocumentMapping::new();
            child_mapping.add(0, "_docID");
            child_mapping.add_render_key(0, "_docID");

            // Build scan mapping for the child
            let child_scan_mapping =
                self.build_scan_mapping_for_join(&target_collection, &child_mapping);

            // Get relation field index in parent mapping
            let relation_field_index = parent_collection
                .fields
                .iter()
                .position(|f| f.name == relation_name)
                .unwrap_or(0);

            // Set up child mapping in parent for TypeJoin
            mapping.set_child_at(relation_field_index, child_scan_mapping.clone());

            // Build child plan (simple scan with fetcher)
            let mut child_scan =
                ScanNode::new((*target_collection).clone(), child_scan_mapping.clone());
            if let Some(ref fetcher) = self.fetcher {
                child_scan = child_scan.with_fetcher(fetcher.clone());
            }
            let child_plan: Box<dyn PlanNode> = Box::new(child_scan);

            // Get child relation field index
            let child_relation_index = target_relation_field
                .and_then(|f| {
                    target_collection
                        .fields
                        .iter()
                        .position(|tf| tf.name == f.name)
                })
                .unwrap_or(0);

            // Create join sides
            let parent_side = JoinSide::new(
                parent_collection.clone(),
                relation_field.clone(),
                relation_field_index,
            )?;

            let child_side = JoinSide::new(
                (*target_collection).clone(),
                target_relation_field
                    .cloned()
                    .unwrap_or_else(|| relation_field.clone()),
                child_relation_index,
            )?;

            let mut join =
                TypeJoinOne::new(plan, child_plan, parent_side, child_side, mapping.clone());
            if let Some(conditions) = select
                .filter
                .as_ref()
                .and_then(|filter| filter.conditions().get(field_name.as_str()))
            {
                let mut child_conditions = serde_json::Map::new();
                child_conditions.insert("_docID".to_string(), conditions.clone());
                join = join.with_relation_filter(RelationFilter {
                    relation_field: relation_name.to_string(),
                    conditions: Filter::from_conditions(child_conditions),
                });
            }
            plan = Box::new(join);

            debug!(
                parent_collection = %parent_collection.name,
                target_collection = %target_collection.name,
                relation_id_field = %field_name,
                relation_field = %relation_name,
                relation_field_is_primary = %relation_field.is_primary,
                target_relation_field_name = ?target_relation_field.as_ref().map(|f| &f.name),
                target_relation_field_is_primary = ?target_relation_field.as_ref().map(|f| f.is_primary),
                "Added TypeJoinOne for secondary relation ID field"
            );
        }

        Ok(plan)
    }
}
