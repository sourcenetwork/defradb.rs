use crate::Subject;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetKind {
    Entity,
    EntitySet,
    Wildcard,
    TypedWildcard,
}

impl TargetKind {
    pub fn of(subject: &Subject) -> Option<Self> {
        match subject {
            Subject::Entity(_) => Some(Self::Entity),
            Subject::EntitySet { .. } => Some(Self::EntitySet),
            Subject::Wildcard => Some(Self::Wildcard),
            Subject::TypedWildcard { .. } => Some(Self::TypedWildcard),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    pub collection: String,
    pub resource: String,
    pub relation: String,
    pub target_kind: TargetKind,
}

pub fn in_scope(
    rules: &[Rule],
    collection: &str,
    resource: &str,
    relation: &str,
    target: &Subject,
) -> bool {
    let Some(kind) = TargetKind::of(target) else {
        return false;
    };
    rules.iter().any(|rule| {
        rule.collection == collection
            && rule.resource == resource
            && rule.relation == relation
            && rule.target_kind == kind
    })
}
