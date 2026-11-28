//! Policy validation helpers shared across ACP backends.
//!
//! Matches Go's `acp/validation.go` semantics and error strings so that
//! Rust and Go produce indistinguishable output at the schema-add boundary.
//!
//! The canonical entry point is [`validate_resource_interface`] which runs
//! the full DRI → DPI chain:
//!
//! 1. Policy must exist in the ACP store
//! 2. Resource must exist on the policy
//! 3. Resource must declare all DPI-required permissions (`read`, `update`,
//!    `delete` for document ACP)
//!
//! Used by both `AcpAdapter` (local ACP backend) and `VeraAcpAdapter`
//! (Vera-backed ACP backend with a local policy cache). Go's equivalent
//! is called by the schema-add path at `internal/db/definition_validation.go`.

use crate::Policy;

/// The DPI-required permissions for a document resource.
///
/// Matches Go's `RequiredResourcePermissionsForDocument` in `acp/types/types.go`.
pub const REQUIRED_DOCUMENT_PERMISSIONS: &[&str] = &["read", "update", "delete"];

/// Validate a policy ID + resource name pair against a retrieved policy.
///
/// Returns `Ok(())` if the policy has the named resource and the resource
/// declares all DPI-required document permissions. Error strings are
/// Go-compatible (see `acp/validation.go`).
///
/// `maybe_policy` is the result of querying the backend's policy store by
/// `policy_id` — `None` means the policy doesn't exist.
///
/// `policy_id` and `resource_name` must be non-empty; empty-arg rejection
/// happens upstream in the SDL directive parser.
pub fn validate_resource_interface(
    policy_id: &str,
    resource_name: &str,
    maybe_policy: Option<&Policy>,
) -> Result<(), String> {
    if policy_id.is_empty() {
        return Err("policyID must not be empty".to_string());
    }
    if resource_name.is_empty() {
        return Err("resource name must not be empty".to_string());
    }

    let policy = maybe_policy.ok_or_else(|| {
        format!(
            "policyID specified does not exist with acp. PolicyID: {}",
            policy_id
        )
    })?;

    let resource = policy.get_resource(resource_name).ok_or_else(|| {
        format!(
            "resource does not exist on the specified policy. PolicyID: {}, ResourceName: {}",
            policy_id, resource_name
        )
    })?;

    for required in REQUIRED_DOCUMENT_PERMISSIONS {
        if resource.get_relation(required).is_none() {
            return Err(format!(
                "resource is missing required permission on policy. \
                 PolicyID: {}, ResourceName: {}, Permission: {}",
                policy_id, resource_name, required
            ));
        }
    }

    Ok(())
}

/// Validate that relation `relation` of resource `resource_name` can bind a
/// field whose value is a document of the collection under `target_resource`.
///
/// The relation must be one the resource declares (not a permission, not
/// `owner`), and its `types` must admit exactly `target_resource`: one object
/// type with no relation, so the relationship derived from the field,
/// `<resource>:<doc>#<relation>@<target_resource>:<value>`, is the only kind
/// it holds.
pub fn validate_relation_binding(
    policy: &Policy,
    resource_name: &str,
    relation: &str,
    target_resource: &str,
) -> Result<(), String> {
    let context = || {
        format!(
            "PolicyID: {}, ResourceName: {}, Relation: {}",
            policy.id, resource_name, relation
        )
    };
    let resource = policy.get_resource(resource_name).ok_or_else(|| {
        format!(
            "resource does not exist on the specified policy. PolicyID: {}, ResourceName: {}",
            policy.id, resource_name
        )
    })?;
    let declared = resource
        .get_relation(relation)
        .filter(|declared| declared.expression == crate::RelationExpression::This);
    let Some(declared) = declared else {
        return Err(format!(
            "relation does not exist on the resource. {}",
            context()
        ));
    };
    if relation == "owner" {
        return Err(format!(
            "the owner relation cannot be bound to a field. {}",
            context()
        ));
    }
    let admits_target = matches!(
        &declared.subject_restriction,
        Some(crate::SubjectRestriction::EntitySet { resource, relation })
            if resource == target_resource && relation.is_empty()
    );
    if !admits_target {
        return Err(format!(
            "relation types must be exactly the related collection's resource. {}, Expected: {}",
            context(),
            target_resource
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Relation, RelationExpression, Resource};

    fn policy_with_perms(id: &str, resource: &str, perms: &[&str]) -> Policy {
        let mut res = Resource::new(resource);
        // Minimal direct relations used by computed permissions
        res = res.with_relation(Relation::direct("owner"));
        for p in perms {
            res = res.with_relation(Relation::computed(
                *p,
                RelationExpression::computed_userset("owner"),
            ));
        }
        Policy::new(id, "test").with_resource(res)
    }

    #[test]
    fn empty_policy_id_errors() {
        let err = validate_resource_interface("", "users", None).unwrap_err();
        assert!(err.contains("policyID must not be empty"));
    }

    #[test]
    fn empty_resource_errors() {
        let err = validate_resource_interface("pid", "", None).unwrap_err();
        assert!(err.contains("resource name must not be empty"));
    }

    #[test]
    fn missing_policy_errors_with_go_message() {
        let err = validate_resource_interface("pid", "users", None).unwrap_err();
        assert!(err.contains("does not exist with acp"), "got: {}", err);
        assert!(err.contains("pid"));
    }

    #[test]
    fn missing_resource_errors_with_go_message() {
        let policy = policy_with_perms("pid", "users", &["read", "update", "delete"]);
        let err = validate_resource_interface("pid", "doesNotExist", Some(&policy)).unwrap_err();
        assert!(
            err.contains("resource does not exist on the specified policy"),
            "got: {}",
            err
        );
        assert!(err.contains("doesNotExist"));
    }

    #[test]
    fn missing_read_perm_errors() {
        let policy = policy_with_perms("pid", "users", &["update", "delete"]);
        let err = validate_resource_interface("pid", "users", Some(&policy)).unwrap_err();
        assert!(err.contains("missing required permission"), "got: {}", err);
        assert!(err.contains("Permission: read"));
    }

    #[test]
    fn missing_update_perm_errors() {
        let policy = policy_with_perms("pid", "users", &["read", "delete"]);
        let err = validate_resource_interface("pid", "users", Some(&policy)).unwrap_err();
        assert!(err.contains("Permission: update"), "got: {}", err);
    }

    #[test]
    fn missing_delete_perm_errors() {
        let policy = policy_with_perms("pid", "users", &["read", "update"]);
        let err = validate_resource_interface("pid", "users", Some(&policy)).unwrap_err();
        assert!(err.contains("Permission: delete"), "got: {}", err);
    }

    #[test]
    fn valid_policy_passes() {
        let policy = policy_with_perms("pid", "users", &["read", "update", "delete"]);
        validate_resource_interface("pid", "users", Some(&policy)).unwrap();
    }
}
