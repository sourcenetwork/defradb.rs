//! Adding collections from SDL: the one implementation every edge calls.

use super::*;
use defra_core::RequestContext;
use identity::Did;
use rapidhash::RapidHashSet;

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Parse `sdl` against `known_types`, validate it and check each `@policy`
    /// directive, producing the collections to create.
    pub(crate) async fn prepare_new_collections(
        &self,
        sdl: &str,
        known_types: RapidHashSet<String>,
    ) -> client::Result<Vec<CollectionVersion>> {
        let collections =
            query::parse_sdl_with_known_types(sdl, known_types).map_err(|e| match e {
                query::QueryError::Parse(message) => client::Error::Parse(message),
                other => client::Error::Parse(other.to_string()),
            })?;
        schema::definition_validation::validate_new_collections(&collections)
            .map_err(|e| client::Error::Validation(e.to_string()))?;
        self.check_policies(&collections).await?;
        Ok(collections)
    }

    async fn check_policies(&self, collections: &[CollectionVersion]) -> client::Result<()> {
        let Some(lookup) = self.policy_lookup() else {
            return Ok(());
        };
        for policy in collections.iter().filter_map(|c| c.policy.as_ref()) {
            let stored = lookup.get_policy(&policy.id).await.map_err(|e| {
                client::Error::Policy(format!(
                    "policy validation failed with acp: {}. PolicyID: {}",
                    e, policy.id
                ))
            })?;
            acp::validate_resource_interface(&policy.id, &policy.resource_name, stored.as_ref())
                .map_err(client::Error::Policy)?;
        }
        Ok(())
    }

    /// The identity acting in `context`. A present but malformed identity
    /// fails, so a permissioned collection is never created unregistered.
    pub(crate) fn creator_of(context: &RequestContext) -> client::Result<Option<Did>> {
        context
            .identity()
            .map(Did::new)
            .transpose()
            .map_err(|e| client::Error::InvalidIdentity(format!("malformed acting identity: {e}")))
    }

    pub(crate) async fn check_collection_patch(
        &self,
        context: &RequestContext,
    ) -> client::Result<()> {
        self.check_node_access_as(context, acp::nac::NodePermission::CollectionPatch)
            .await
            .map_err(|e| client::Error::NotAuthorized(e.to_string()))
    }
}

impl<S> crate::database::DB<S>
where
    S: Store + 'static,
{
    /// Parse `sdl`, check it and create every collection it defines, or none.
    /// The one core function every edge's `Client::add_collection` calls.
    pub async fn add_collection(
        &self,
        context: &RequestContext,
        sdl: &str,
    ) -> client::Result<Vec<CollectionVersion>> {
        self.check_collection_patch(context).await?;
        let creator = Self::creator_of(context)?;
        let known_types = self
            .list_collections()
            .map_err(|e| client::Error::Storage(e.to_string()))?
            .into_iter()
            .collect();
        let collections = self.prepare_new_collections(sdl, known_types).await?;
        match self.document_acp().cloned() {
            Some(document_acp) => {
                self.create_collections_atomic_with_acp_registration_as(
                    context,
                    collections,
                    document_acp,
                    creator,
                )
                .await
            }
            None => {
                self.create_collections_atomic_as(context, collections)
                    .await
            }
        }
        .map_err(|e| client::Error::Create(e.to_string()))
    }
}
