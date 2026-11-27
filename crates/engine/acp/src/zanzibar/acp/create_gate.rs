//! The relationships a document's `@policy(relation:)` fields carry, stored
//! with its owner when the local engine registers it. The create that
//! registers decides `create` over them first, in the same transaction
//! (`register_created_doc` in the db crate).

use zanzibar::store::ZanzibarStore;
use zanzibar::types::{Relationship, Subject};

use super::ZanzibarDocumentACP;
use crate::error::Result;
use crate::DerivedRelationship;

impl<S: ZanzibarStore + ?Sized + 'static> ZanzibarDocumentACP<S> {
    /// Store `derived` on `doc_id`, each an edge to the linked object.
    pub(super) async fn store_derived(
        &self,
        policy_id: &str,
        resource: &str,
        doc_id: &str,
        derived: &[DerivedRelationship],
    ) -> Result<()> {
        for link in derived {
            let edge = Relationship::try_new(
                resource,
                doc_id,
                link.relation.as_str(),
                Subject::entity_set(
                    link.target_resource.as_str(),
                    link.target_doc_id.as_str(),
                    "",
                ),
            )?;
            self.store.store_relationship(policy_id, &edge).await?;
        }
        Ok(())
    }
}
