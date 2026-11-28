//! Encrypted (searchable-encryption) indexes as the caller `context` names,
//! in the transaction it carries if it carries one.

use defra_core::RequestContext;

use super::NodeCore;
use crate::ops::EncryptedIndexListing;
use crate::Result;

impl NodeCore<'_> {
    /// Add an equality encrypted index on `field_name` of `collection`.
    pub(crate) async fn add_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<EncryptedIndexListing> {
        let caller = context.clone();
        let owner = self.node.admitted_caller(context)?;
        self.node
            .ops()
            .add_encrypted_index(&owner, &caller, collection, field_name)
            .await
    }

    /// The encrypted indexes of `collection`.
    pub(crate) async fn list_encrypted_indexes(
        &self,
        context: &RequestContext,
        collection: &str,
    ) -> Result<Vec<EncryptedIndexListing>> {
        let caller = context.clone();
        let owner = self.node.admitted_caller(context)?;
        self.node
            .ops()
            .list_encrypted_indexes(&owner, &caller, collection)
            .await
    }

    /// Every collection's encrypted indexes, by collection name; a
    /// collection without one is left out.
    pub(crate) async fn list_all_encrypted_indexes(
        &self,
        context: &RequestContext,
    ) -> Result<Vec<(String, Vec<EncryptedIndexListing>)>> {
        let caller = context.clone();
        let owner = self.node.admitted_caller(context)?;
        self.node
            .ops()
            .list_all_encrypted_indexes(&owner, &caller)
            .await
    }

    /// Delete the encrypted index on `field_name` of `collection`.
    pub(crate) async fn delete_encrypted_index(
        &self,
        context: &RequestContext,
        collection: &str,
        field_name: &str,
    ) -> Result<()> {
        let caller = context.clone();
        let owner = self.node.admitted_caller(context)?;
        self.node
            .ops()
            .delete_encrypted_index(&owner, &caller, collection, field_name)
            .await
    }
}
