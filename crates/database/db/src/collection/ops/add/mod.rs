//! Adding collections from SDL: the one implementation every edge calls.

mod bindings;
mod error;

pub use error::AddCollectionError;

use super::*;
use defra_core::RequestContext;
use identity::Did;
use rapidhash::RapidHashMap;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// The collections a new one may relate to: every cached collection's
    /// name with its collection ID.
    ///
    /// A relation to one of them commits to that ID, so a collection added on
    /// its own has the identity it has when its target is declared beside it.
    /// The IDs are read outside the creating transaction, which checks every
    /// relation target again against the collections it sees.
    pub fn known_collections(&self) -> RapidHashMap<String, String> {
        self.collections.peek(|cache| {
            cache
                .names()
                .filter_map(|name| {
                    let collection = cache.get(name)?;
                    Some((name.clone(), collection.collection_id().to_owned()))
                })
                .collect()
        })
    }

    /// Parse `sdl` against `known_collections`, validate it and check each
    /// `@policy` directive, producing the collections to create.
    pub(crate) async fn prepare_new_collections(
        &self,
        sdl: &str,
        known_collections: RapidHashMap<String, String>,
    ) -> std::result::Result<Vec<CollectionVersion>, AddCollectionError> {
        let collections = query::parse_sdl_with_known_collections(sdl, known_collections).map_err(
            |e| match e {
                query::QueryError::Parse(message) => AddCollectionError::Parse(message),
                query::QueryError::UnsupportedFieldType(refusals) => {
                    AddCollectionError::UnsupportedFieldType(refusals)
                }
                other => AddCollectionError::Parse(other.to_string()),
            },
        )?;
        schema::definition_validation::validate_new_collections(&collections)
            .map_err(|e| AddCollectionError::Validation(format!("schema error: {e}")))?;
        self.check_policies(&collections).await?;
        self.check_policy_bindings(&collections).await?;
        Ok(collections)
    }

    async fn check_policies(
        &self,
        collections: &[CollectionVersion],
    ) -> std::result::Result<(), AddCollectionError> {
        if self.dac_policies().is_none() {
            return Ok(());
        }
        for policy in collections.iter().filter_map(|c| c.policy.as_ref()) {
            self.validate_resource_interface(&policy.id, &policy.resource_name)
                .await
                .map_err(|e| AddCollectionError::Policy(e.to_string()))?;
        }
        Ok(())
    }

    /// The identity acting in `context`. A present but malformed identity
    /// fails, so a permissioned collection is never created unregistered.
    pub(crate) fn creator_of(
        context: &RequestContext,
    ) -> std::result::Result<Option<Did>, AddCollectionError> {
        context.identity().map(Did::new).transpose().map_err(|e| {
            AddCollectionError::InvalidIdentity(format!("malformed acting identity: {e}"))
        })
    }

    pub(crate) async fn check_collection_patch(
        &self,
        context: &RequestContext,
    ) -> std::result::Result<(), AddCollectionError> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await
            .map_err(|e| AddCollectionError::NotAuthorized(e.to_string()))
    }
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Parse `sdl`, check it and create every collection it defines, or none.
    /// The one core function every edge calls.
    pub async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> std::result::Result<Vec<CollectionVersion>, AddCollectionError> {
        self.check_collection_patch(context).await?;
        let creator = Self::creator_of(context)?;
        let collections = self
            .prepare_new_collections(sdl, self.known_collections())
            .await?;

        self.create_collections_atomic(context, collections, self.document_acp().cloned(), creator)
            .await
            .map_err(|e| AddCollectionError::Create(e.to_string()))
    }
}
