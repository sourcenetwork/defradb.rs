use std::collections::{HashMap, HashSet};

use super::{ParsedPolicy, PolicyRelation};
use crate::error::{Error, Result};

fn invalid(reason: impl Into<String>) -> Error {
    Error::InvalidPolicy(reason.into())
}

fn identifier(name: &str) -> Result<()> {
    let mut bytes = name.bytes();
    if name.len() > 128
        || !bytes
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == b'_')
        || !bytes.all(|c| c.is_ascii_alphanumeric() || c == b'_')
    {
        return Err(invalid(format!("invalid policy identifier '{name}'")));
    }
    Ok(())
}

pub(super) fn validate(policy: &ParsedPolicy) -> Result<()> {
    if policy.name.trim().is_empty() || policy.name.len() > 256 {
        return Err(invalid("policy name must contain 1–256 bytes"));
    }
    let mut names = HashMap::new();
    for resource in &policy.resources {
        identifier(&resource.name)?;
        if resource.name == "actor" || names.contains_key(resource.name.as_str()) {
            return Err(invalid(format!(
                "duplicate or reserved resource '{}'",
                resource.name
            )));
        }
        let mut relations = relation_names(&resource.relations, true)?;
        for permission in &resource.permissions {
            identifier(&permission.name)?;
            if !relations.insert(permission.name.as_str()) {
                return Err(invalid(format!(
                    "duplicate or reserved relation '{}'",
                    permission.name
                )));
            }
        }
        names.insert(resource.name.as_str(), relations);
    }
    names.insert("actor", relation_names(&policy.actor.relations, false)?);
    for resource in &policy.resources {
        validate_references(&resource.relations, &names)?;
    }
    validate_references(&policy.actor.relations, &names)
}

fn relation_names(relations: &[PolicyRelation], with_owner: bool) -> Result<HashSet<&str>> {
    let mut names = HashSet::new();
    for relation in relations {
        identifier(&relation.name)?;
        if !names.insert(relation.name.as_str()) {
            return Err(invalid(format!("duplicate relation '{}'", relation.name)));
        }
        if relation.name == "owner"
            && (!with_owner || !relation.types.is_empty() || !relation.manages.is_empty())
        {
            return Err(invalid("the owner relation cannot be customized"));
        }
    }
    if with_owner {
        names.insert("owner");
    }
    Ok(names)
}

fn validate_references(
    relations: &[PolicyRelation],
    resources: &HashMap<&str, HashSet<&str>>,
) -> Result<()> {
    for relation in relations {
        for managed in &relation.manages {
            if managed == "owner" || !relations.iter().any(|r| r.name == *managed) {
                return Err(invalid(format!("invalid managed relation '{managed}'")));
            }
        }
        for subject_type in &relation.types {
            let subject_type = subject_type.trim();
            let (resource, relation) = subject_type.split_once("->").unwrap_or((subject_type, ""));
            let valid = resources
                .get(resource)
                .is_some_and(|names| relation.is_empty() || names.contains(relation));
            if !valid {
                return Err(invalid(format!("unknown subject type '{subject_type}'")));
            }
        }
    }
    Ok(())
}
