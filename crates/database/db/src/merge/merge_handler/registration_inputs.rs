use std::sync::Arc;

use acp::RegistrationParent;
use rapidhash::RapidHashMap;
use storage::corekv::Store;

use super::{DbMergeHandler, MergeError};

/// One external registration fact needed to persist a signed genesis.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum RegistrationLookup {
    Parent {
        policy: String,
        child: String,
        parent: String,
    },
    Registered {
        policy: String,
        resource: String,
        document: String,
    },
}

/// Only successful answers are reused, within the current batch or history page.
pub(super) type RegistrationInputs =
    Arc<RapidHashMap<RegistrationLookup, Option<RegistrationParent>>>;

impl<S: Store, B: blockstore::Blockstore> DbMergeHandler<S, B> {
    pub(super) fn registration_input(
        &self,
        lookup: RegistrationLookup,
    ) -> Result<Option<RegistrationParent>, MergeError> {
        self.registration_inputs
            .get(&lookup)
            .cloned()
            .ok_or_else(|| MergeError::NeedsRegistration(Box::new(lookup)))
    }

    pub(super) async fn resolve_registration(
        &mut self,
        lookup: RegistrationLookup,
    ) -> Result<(), MergeError> {
        let provider = self.db.document_acp().ok_or_else(|| {
            MergeError::MergeFailed("missing remote registration provider".into())
        })?;
        let answer = match &lookup {
            RegistrationLookup::Parent {
                policy,
                child,
                parent,
            } => Some(
                provider
                    .registration_parent(policy, child, parent)
                    .await
                    .map_err(|e| MergeError::MergeFailed(e.to_string()))?,
            ),
            RegistrationLookup::Registered {
                policy,
                resource,
                document,
            } => {
                if !provider
                    .is_doc_registered(policy, resource, document)
                    .await
                    .map_err(|e| MergeError::MergeFailed(e.to_string()))?
                {
                    return Err(MergeError::MergeFailed(
                        "parent-gated document awaits provider registration".into(),
                    ));
                }
                None
            }
        };
        Arc::make_mut(&mut self.registration_inputs).insert(lookup, answer);
        Ok(())
    }
}
