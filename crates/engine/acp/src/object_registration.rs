//! Several objects registered to one owner in one go.
//!
//! An external provider applies a registration as one request, and a request
//! costs it a block. The registrations one transaction makes for one owner
//! go together ([`crate::DocumentACP::register_objects`]), as one list the
//! provider applies in order, all or none. A provider that cannot apply a
//! list sends them one at a time, as it always did.

use crate::error::Error;

/// The object on which a child registration requires permission.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct RegistrationParent {
    pub resource: String,
    pub object_id: String,
}

impl RegistrationParent {
    /// Resolve the rule's single object type from the compiled relation.
    pub fn from_policy(policy: &zanzibar::Policy, child: &str, id: &str) -> crate::Result<Self> {
        Self::with_requirement(policy, child, id).map(|(parent, _)| parent)
    }

    pub fn with_requirement<'a>(
        policy: &'a zanzibar::Policy,
        child: &str,
        id: &str,
    ) -> crate::Result<(Self, &'a str)> {
        let resource = policy
            .get_resource(child)
            .ok_or_else(|| Error::InvalidPolicy(format!("unknown child resource '{child}'")))?;
        let rule = resource.create.as_ref().ok_or_else(|| {
            Error::InvalidPolicy(format!("resource '{child}' has no create rule"))
        })?;
        let restriction = resource
            .get_relation(&rule.parent)
            .and_then(|relation| relation.subject_restriction.as_ref());
        match restriction {
            Some(zanzibar::SubjectRestriction::EntitySet { resource, relation })
                if relation.is_empty() && !id.is_empty() =>
            {
                Ok((
                    Self {
                        resource: resource.clone(),
                        object_id: id.into(),
                    },
                    rule.requires.as_str(),
                ))
            }
            _ => Err(Error::InvalidPolicy(
                "registration requires a single parent object".into(),
            )),
        }
    }
}

/// One object an owner is registered for: a document (its DocID) or a
/// branchable collection (its collection ID).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AcpObject<'a> {
    pub policy_id: &'a str,
    pub resource: &'a str,
    pub object_id: &'a str,
}

/// Where a list of registrations stopped: the objects before `index` are
/// registered, the one at `index` failed with `error`, and the ones after it
/// were not tried.
#[derive(Debug)]
pub struct RegistrationFailure {
    pub index: usize,
    pub error: Error,
}

impl RegistrationFailure {
    pub fn new(index: usize, error: Error) -> Self {
        Self { index, error }
    }
}
