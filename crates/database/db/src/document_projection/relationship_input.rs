//! The prepared input a projection reads for one relationship: the request
//! the edge answers, keyed by everything the answer depends on.

use identity::Did;

/// One relationship observation a projection asks the edge for.
pub(super) struct RelationshipInput<'a> {
    pub(super) actor: &'a str,
    pub(super) policy: &'a str,
    pub(super) resource: &'a str,
    pub(super) doc: &'a str,
    pub(super) bearer: Option<String>,
    pub(super) relation: &'a str,
    pub(super) target: &'a Did,
    /// The latest change staged on the tuple: a later change names another
    /// input, so an answer given before it is never read for it
    /// (`proofs/tla/SettleBeforeObserving.tla`, `INV_ReuseHoldsLatest`).
    pub(super) head: Option<u64>,
}

impl RelationshipInput<'_> {
    /// The request's bytes, as the edge reads them (`write::prepare`).
    pub(super) fn request(&self) -> serde_json::Result<Vec<u8>> {
        serde_json::to_vec(&(
            self.actor,
            "relationship",
            self.policy,
            self.resource,
            self.doc,
            &self.bearer,
            self.relation,
            self.target.as_str(),
            self.head,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(target: &Did, head: Option<u64>) -> RelationshipInput<'_> {
        RelationshipInput {
            actor: "did:key:z6MkActor",
            policy: "policy",
            resource: "users",
            doc: "bae-doc",
            bearer: None,
            relation: "reader",
            target,
            head,
        }
    }

    #[test]
    fn a_later_change_on_the_tuple_names_another_input() {
        let target = Did::new("did:key:z6MkTarget").unwrap();
        let first = input(&target, Some(1)).request().unwrap();
        assert_ne!(first, input(&target, Some(2)).request().unwrap());
        assert_ne!(first, input(&target, None).request().unwrap());
        assert_eq!(first, input(&target, Some(1)).request().unwrap());
    }

    #[test]
    fn the_edge_reads_the_target_and_relation_where_it_expects_them() {
        let target = Did::new("did:key:z6MkTarget").unwrap();
        let request = input(&target, Some(7)).request().unwrap();
        let fields: Vec<serde_json::Value> = serde_json::from_slice(&request).unwrap();
        assert_eq!(fields[1], "relationship");
        assert_eq!(fields[6], "reader");
        assert_eq!(fields[7], target.as_str());
        assert_eq!(fields[8], 7);
    }
}
