//! A copy of the node's NAC state into another store, so a snapshot of it can
//! answer permission checks without reading storage.

use rapidhash::{HashSetExt, RapidHashSet};
use zanzibar::{Relationship, Subject, ZanzibarStore};

use super::{NodeACP, DISABLED_RELATION, NODE_OBJECT_ID};
use crate::error::Result;
use crate::nac::permission::NodePermission;
use crate::nac::policy::{ADMIN_RELATION, NODE_POLICY_ID, NODE_RESOURCE_NAME, OWNER_RELATION};

impl<S> NodeACP<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// Copy the node policy and every relationship a node permission check
    /// can reach into `target`: the node object's, and those of every source
    /// object a grant names, transitively. Nothing is copied when NAC was
    /// never configured.
    pub async fn copy_into<T>(&self, target: &T) -> Result<()>
    where
        T: ZanzibarStore + ?Sized,
    {
        let Some(policy) = self.store.get_policy(NODE_POLICY_ID).await? else {
            return Ok(());
        };
        target.store_policy(&policy).await?;
        let mut relations = vec![OWNER_RELATION, ADMIN_RELATION, DISABLED_RELATION];
        relations.extend(
            NodePermission::all()
                .iter()
                .map(|permission| permission.as_str()),
        );
        let mut visited = RapidHashSet::new();
        let mut pending = vec![(NODE_RESOURCE_NAME.to_owned(), NODE_OBJECT_ID.to_owned())];
        while let Some((resource, object)) = pending.pop() {
            if !visited.insert((resource.clone(), object.clone())) {
                continue;
            }
            for &relation in &relations {
                let subjects = self
                    .store
                    .get_relation_subjects(NODE_POLICY_ID, &resource, &object, relation)
                    .await?;
                for subject in subjects {
                    if let Subject::EntitySet {
                        resource: child_resource,
                        object_id: child,
                        ..
                    } = &subject
                    {
                        pending.push((child_resource.clone(), child.clone()));
                    }
                    let relationship =
                        Relationship::new(resource.as_str(), object.as_str(), relation, subject);
                    target
                        .store_relationship(NODE_POLICY_ID, &relationship)
                        .await?;
                }
            }
        }
        Ok(())
    }
}
