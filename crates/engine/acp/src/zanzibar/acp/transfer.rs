//! Moving a document to another owner on the node's own document ACP.

use identity::Did;
use zanzibar::store::ZanzibarStore;
use zanzibar::types::Relationship;

use super::{ZanzibarDocumentACP, OWNER_RELATION};
use crate::error::{Error, Result};

impl<S: ZanzibarStore + ?Sized + 'static> ZanzibarDocumentACP<S> {
    /// Replace the owner tuple of one object, held by `actor` alone, with
    /// one naming `new_owner`, in the store this ACP writes through (the
    /// caller's transaction when it joins one). Every other relationship on
    /// the object stays. An actor that is not the object's one owner is
    /// refused with [`Error::NotOwner`]; giving the object to its owner
    /// changes nothing.
    pub(super) async fn transfer_owner(
        &self,
        actor: &Did,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
        new_owner: &Did,
    ) -> Result<()> {
        if new_owner.is_wildcard() {
            return Err(Error::UnsupportedSubject(new_owner.to_string()));
        }
        self.ensure_policy(policy_id, resource_name).await?;
        let owners = self
            .store
            .get_relation_subjects(policy_id, resource_name, doc_id, OWNER_RELATION)
            .await?;
        if owners.len() != 1 || owners[0].as_entity() != Some(actor) {
            return Err(Error::NotOwner {
                operation: "transfer the document's ownership".into(),
            });
        }
        if new_owner == actor {
            return Ok(());
        }
        let old = Relationship::with_entity(resource_name, doc_id, OWNER_RELATION, actor.clone());
        let new =
            Relationship::with_entity(resource_name, doc_id, OWNER_RELATION, new_owner.clone());
        self.store.delete_relationship(policy_id, &old).await?;
        self.store.store_relationship(policy_id, &new).await?;
        tracing::info!(
            target: "acp::audit",
            event = "document_transferred",
            previous_owner = %actor,
            owner = %new_owner,
            collection = %resource_name,
            doc_id = %doc_id,
            "Document ownership transferred via Zanzibar"
        );
        Ok(())
    }
}
