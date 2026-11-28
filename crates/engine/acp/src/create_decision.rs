//! Whether an actor may create a document in a collection whose policy
//! resource restricts who may add to it (its `collection.create`).
//!
//! `create` is a union, intersection or difference of tuple to usersets
//! (`author->publish`), evaluated on a document that does not exist yet: an
//! arrow holds when a relationship derived from the document's
//! `@policy(relation:)` fields links an object on which the actor holds the
//! computed name. So the decision is one permission check per linked object,
//! combined as the expression says (`proofs/lean/CreateDecision.lean` proves
//! it equal to acp-rs's `check_create`). Any other name never admits: acp-rs
//! refuses it in `create`, and a create the node cannot decide is refused.

use std::future::Future;

use rapidhash::{HashMapExt as _, RapidHashMap};

use crate::{DerivedRelationship, Error, RelationExpression, Result};

/// One permission check `create` depends on: `computed` on a linked object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct CreateCheck {
    /// The name the arrow computes on the linked object, such as `publish`.
    pub computed: String,
    /// The linked object's resource.
    pub resource: String,
    /// The linked object's ID.
    pub object: String,
}

/// Whether `create` admits nobody but by checks, so a decision is needed:
/// it is present and not `"*"`.
pub fn is_gated(create: Option<&str>) -> bool {
    create.is_some_and(|create| create.trim() != "*")
}

/// The checks `create` needs over `derived`, each once, in the order the
/// expression names them.
pub fn create_checks(create: &str, derived: &[DerivedRelationship]) -> Result<Vec<CreateCheck>> {
    let expression = parse(create)?;
    let mut checks = Vec::new();
    collect(&expression, derived, &mut checks);
    Ok(checks)
}

/// Whether `create` admits, given each check's answer in `answers`. A check
/// without an answer does not hold.
pub fn admits(
    create: &str,
    derived: &[DerivedRelationship],
    answers: &RapidHashMap<CreateCheck, bool>,
) -> Result<bool> {
    Ok(evaluate(&parse(create)?, derived, answers))
}

/// Decide `create` over `derived`, asking `check` for each linked object.
pub async fn authorize_create<F, Fut>(
    create: &str,
    derived: &[DerivedRelationship],
    mut check: F,
) -> Result<bool>
where
    F: FnMut(CreateCheck) -> Fut,
    Fut: Future<Output = Result<bool>>,
{
    if !is_gated(Some(create)) {
        return Ok(true);
    }
    let checks = create_checks(create, derived)?;
    let mut answers = RapidHashMap::with_capacity(checks.len());
    for wanted in checks {
        let allowed = check(wanted.clone()).await?;
        answers.insert(wanted, allowed);
    }
    admits(create, derived, &answers)
}

/// Decide `create` for `actor` over `derived` under `policy_id`, through
/// `provider`'s named permission checks on each linked object.
pub async fn authorize_create_through(
    provider: &dyn crate::DocumentACP,
    actor: &identity::Did,
    policy_id: &str,
    create: &str,
    derived: &[DerivedRelationship],
) -> Result<bool> {
    authorize_create(create, derived, |wanted| async move {
        provider
            .check_named_permission(
                actor,
                &wanted.computed,
                policy_id,
                &wanted.resource,
                &wanted.object,
            )
            .await
    })
    .await
}

fn parse(create: &str) -> Result<RelationExpression> {
    RelationExpression::parse(create)
        .map_err(|error| Error::InvalidPolicy(format!("collection create `{create}`: {error}")))
}

fn linked<'a>(
    derived: &'a [DerivedRelationship],
    relation: &'a str,
) -> impl Iterator<Item = &'a DerivedRelationship> {
    derived.iter().filter(move |link| link.relation == relation)
}

fn check_of(link: &DerivedRelationship, computed: &str) -> CreateCheck {
    CreateCheck {
        computed: computed.to_owned(),
        resource: link.target_resource.clone(),
        object: link.target_doc_id.clone(),
    }
}

fn collect(
    expression: &RelationExpression,
    derived: &[DerivedRelationship],
    out: &mut Vec<CreateCheck>,
) {
    match expression {
        RelationExpression::TupleToUserset {
            tuple_relation,
            computed_relation,
        } => {
            for link in linked(derived, tuple_relation) {
                let wanted = check_of(link, computed_relation);
                if !out.contains(&wanted) {
                    out.push(wanted);
                }
            }
        }
        RelationExpression::Union(parts) | RelationExpression::Intersection(parts) => {
            for part in parts {
                collect(part, derived, out);
            }
        }
        RelationExpression::Difference { base, subtract } => {
            collect(base, derived, out);
            collect(subtract, derived, out);
        }
        // `_this` and names of the document's own resource follow nothing.
        _ => {}
    }
}

fn evaluate(
    expression: &RelationExpression,
    derived: &[DerivedRelationship],
    answers: &RapidHashMap<CreateCheck, bool>,
) -> bool {
    match expression {
        RelationExpression::TupleToUserset {
            tuple_relation,
            computed_relation,
        } => linked(derived, tuple_relation).any(|link| {
            answers
                .get(&check_of(link, computed_relation))
                .copied()
                .unwrap_or(false)
        }),
        RelationExpression::Union(parts) => {
            parts.iter().any(|part| evaluate(part, derived, answers))
        }
        RelationExpression::Intersection(parts) => {
            parts.iter().all(|part| evaluate(part, derived, answers))
        }
        RelationExpression::Difference { base, subtract } => {
            evaluate(base, derived, answers) && !evaluate(subtract, derived, answers)
        }
        // `_this` and names of the document's own resource never admit.
        _ => false,
    }
}

#[cfg(test)]
#[path = "create_decision_tests.rs"]
mod tests;
