mod bounded_yaml;
mod parse;
mod schema;
mod validate;

pub use parse::{check_duplicate_yaml_keys, parse_policy_yaml};
pub use validate::validate_policy_expressions;

use std::collections::BTreeMap;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use zanzibar::{
    Policy, PolicySpecification, Relation, RelationExpression, Resource, SubjectRestriction,
};

/// Generate a Go-compatible policy ID from parsed policy fields.
///
/// Matches Go's `acp_core` `IdTransformer.Transform` + `hashPol`:
/// 1. Inner hash: SHA256(name + sorted resources/relations/permissions)
/// 2. Outer hash: SHA256(inner_hash_bytes + counter_as_string)
pub fn generate_policy_id(parsed: &ParsedPolicy, counter: u64) -> String {
    let inner_hash = hash_policy_fields(parsed);

    let mut outer_hasher = Sha256::new();
    outer_hasher.update(&inner_hash);
    outer_hasher.update(format!("{}", counter).as_bytes());

    let hash = outer_hasher.finalize();
    hash.iter().map(|b| format!("{:02x}", b)).collect()
}

fn hash_policy_fields(policy: &ParsedPolicy) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(policy.name.as_bytes());

    let mut resources: Vec<_> = policy.resources.iter().collect();
    resources.sort_by(|a, b| a.name.cmp(&b.name));

    for resource in &resources {
        hasher.update(resource.name.as_bytes());

        let mut relations: Vec<_> = resource.relations.iter().collect();
        relations.sort_by(|a, b| a.name.cmp(&b.name));
        for relation in relations {
            hasher.update(relation.name.as_bytes());
        }

        let mut permissions: Vec<_> = resource.permissions.iter().collect();
        permissions.sort_by(|a, b| a.name.cmp(&b.name));
        for permission in permissions {
            hasher.update(permission.name.as_bytes());
            hasher.update(permission.expr.as_bytes());
        }

        // Only a present `create` is hashed, so a policy without one keeps
        // the ID it had before the section existed.
        if let Some(create) = resource.collection_create() {
            hasher.update(b"collection.create");
            hasher.update(create.as_bytes());
        }
    }

    let mut individual: Vec<_> = resources
        .iter()
        .flat_map(|resource| {
            resource
                .relations
                .iter()
                .map(move |relation| (resource.name.as_str(), relation))
        })
        .chain(
            policy
                .actor
                .relations
                .iter()
                .map(|relation| ("actor", relation)),
        )
        .filter(|(_, relation)| {
            relation
                .types
                .iter()
                .any(|ty| ty.trim() == "actor:individual")
        })
        .collect();
    individual.sort_by(|(a, b), (c, d)| (a, &b.name).cmp(&(c, &d.name)));
    if !individual.is_empty() {
        hasher.update(b"defra/individual-actor/v1\0");
        for (resource, relation) in individual {
            for part in [resource, relation.name.as_str()] {
                hasher.update((part.len() as u64).to_be_bytes());
                hasher.update(part.as_bytes());
            }
            let mut types = relation.types.clone();
            types.sort();
            hasher.update((types.len() as u64).to_be_bytes());
            for ty in types {
                hasher.update((ty.len() as u64).to_be_bytes());
                hasher.update(ty.as_bytes());
            }
        }
    }

    hasher.finalize().to_vec()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ParsedPolicy {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default, deserialize_with = "parse::deserialize_specification")]
    pub spec: PolicySpecification,
    #[serde(default)]
    pub meta: BTreeMap<String, String>,
    #[serde(default)]
    pub actor: PolicyActor,
    #[serde(default)]
    pub resources: Vec<PolicyResource>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyResource {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<PolicyPermission>,
    #[serde(default)]
    pub relations: Vec<PolicyRelation>,
    /// Collection access control: who may add objects to the resource.
    #[serde(default)]
    pub collection: Option<PolicyCollection>,
}

/// A resource's `collection` section.
///
/// `create` is omitted (anyone may create), `"*"` (anyone, said explicitly),
/// or an expression over the relations a new object carries, which acp
/// evaluates. The local engine does not gate creates, so it keeps the text
/// without reading it.
#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyCollection {
    #[serde(default)]
    pub create: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyPermission {
    pub name: String,
    #[serde(default)]
    pub doc: String,
    #[serde(default)]
    pub expr: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRelation {
    pub name: String,
    #[serde(default)]
    pub doc: String,
    #[serde(default)]
    pub types: Vec<String>,
    #[serde(default)]
    pub manages: Vec<String>,
}

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyActor {
    #[serde(default)]
    pub relations: Vec<PolicyRelation>,
}

/// Map a relation's declared `types:` to an enforced [`SubjectRestriction`].
///
/// Each type names an allowed subject shape:
/// - `actor`               → an actor DID or the all-actors wildcard `*`
/// - `actor:individual`    → an individual actor DID only
/// - `resource`            → a cross-object edge to an object of `resource`
/// - `resource->relation`  → a userset `resource:obj#relation`
///
/// The userset separator is acp_core's tuple-to-userset operator `->`, not `#`
/// (which is the tuple-subject grammar handled by `parse_target_subject`).
///
/// Multiple types combine as a union. An empty list yields `None` (no
/// restriction), preserving the behaviour of relations that omit `types:`.
fn build_subject_restriction(types: &[String]) -> crate::error::Result<Option<SubjectRestriction>> {
    let mut restrictions = Vec::with_capacity(types.len());
    for ty in types {
        let ty = ty.trim();
        let restriction = if ty == "actor" {
            SubjectRestriction::Actor
        } else if ty == "actor:individual" {
            SubjectRestriction::Entity
        } else if let Some((resource, relation)) = ty.split_once("->") {
            if resource.is_empty() || relation.is_empty() {
                return Err(crate::error::Error::InvalidRelation(format!(
                    "invalid relation type '{}': expected 'actor', 'resource', or 'resource->relation'",
                    ty
                )));
            }
            SubjectRestriction::EntitySet {
                resource: resource.to_string(),
                relation: relation.to_string(),
            }
        } else if ty.is_empty() {
            return Err(crate::error::Error::InvalidRelation(
                "empty relation type".to_string(),
            ));
        } else {
            // A bare resource name authorises a cross-object edge: an EntitySet
            // subject of that resource carrying no relation.
            SubjectRestriction::EntitySet {
                resource: ty.to_string(),
                relation: String::new(),
            }
        };
        restrictions.push(restriction);
    }

    Ok(match restrictions.len() {
        0 => None,
        1 => Some(restrictions.into_iter().next().unwrap()),
        _ => Some(SubjectRestriction::AnyOf(restrictions)),
    })
}

impl ParsedPolicy {
    pub fn find_resource(&self, name: &str) -> Option<&PolicyResource> {
        self.resources.iter().find(|r| r.name == name)
    }
}

impl PolicyResource {
    /// The `create` expression of the resource's `collection` section.
    pub fn collection_create(&self) -> Option<&str> {
        self.collection.as_ref()?.create.as_deref()
    }

    pub fn has_permission(&self, name: &str) -> bool {
        self.permissions.iter().any(|p| p.name == name)
    }

    pub fn has_relation(&self, name: &str) -> bool {
        self.relations.iter().any(|r| r.name == name)
    }

    /// Get relation names that manage the given relation.
    ///
    /// For example, if "admin" has `manages: [reader]`, then
    /// `get_managers_for_relation("reader")` returns `["admin"]`.
    pub fn get_managers_for_relation(&self, relation: &str) -> Vec<&str> {
        self.relations
            .iter()
            .filter(|r| r.manages.iter().any(|m| m == relation))
            .map(|r| r.name.as_str())
            .collect()
    }
}

/// Build a Zanzibar Policy from an already-parsed YAML policy.
///
/// The `counter` parameter is a monotonic sequence number used together with
/// the parsed policy fields to generate Go-compatible policy IDs.
pub fn build_policy(parsed: &ParsedPolicy, counter: u64) -> crate::error::Result<Policy> {
    schema::validate(parsed)?;
    if parsed.spec == PolicySpecification::Defra {
        for resource in &parsed.resources {
            for permission in ["read", "write"] {
                if !resource.has_permission(permission) {
                    return Err(crate::error::Error::InvalidPolicy(format!(
                        "Defra specification requires permission '{permission}' on resource '{}'",
                        resource.name
                    )));
                }
            }
        }
    }
    let id = generate_policy_id(parsed, counter);

    let mut resources = Vec::new();
    for res in &parsed.resources {
        let mut relations = Vec::new();

        // Auto-inject the reserved 'owner' relation (matches Go DefraDB behavior)
        relations.push(
            match res
                .relations
                .iter()
                .find(|relation| relation.name == "owner")
            {
                Some(owner) => build_relation(owner)?,
                None => Relation::direct("owner"),
            },
        );

        for rel in &res.relations {
            if rel.name != "owner" {
                relations.push(build_relation(rel)?);
            }
        }

        for perm in &res.permissions {
            let mut expression = if perm.expr.is_empty() {
                // A permission with no explicit expression is still valid and
                // defaults to owner-only access in Go.
                RelationExpression::computed_userset("owner")
            } else {
                let user_expr = RelationExpression::parse(&perm.expr)?;
                // DPI: every permission must include 'owner' in its expression
                RelationExpression::Union(vec![
                    RelationExpression::computed_userset("owner"),
                    user_expr,
                ])
            };
            if parsed.spec == PolicySpecification::Defra && perm.name == "read" {
                expression = RelationExpression::Union(vec![
                    expression,
                    RelationExpression::computed_userset("write"),
                ]);
            }
            let mut relation = Relation::computed(&perm.name, expression);
            relation.description = perm.doc.clone();
            relations.push(relation);
        }

        resources.push(Resource {
            name: res.name.clone(),
            description: res.description.clone(),
            relations,
            collection_create: res.collection_create().map(str::to_owned),
        });
    }

    let policy = Policy {
        id,
        name: parsed.name.clone(),
        resources,
        attributes: parsed.meta.clone(),
        description: parsed.description.clone(),
        actor: Some(Resource {
            name: "actor".into(),
            description: String::new(),
            collection_create: None,
            relations: parsed
                .actor
                .relations
                .iter()
                .map(build_relation)
                .collect::<crate::error::Result<_>>()?,
        }),
        specification: parsed.spec,
    };
    policy
        .validate()
        .map_err(|error| crate::error::Error::InvalidPolicy(error.to_string()))?;
    Ok(policy)
}

fn build_relation(parsed: &PolicyRelation) -> crate::error::Result<Relation> {
    let mut relation = Relation::direct(&parsed.name);
    relation.description = parsed.doc.clone();
    relation.manages = parsed.manages.clone();
    relation.subject_restriction = build_subject_restriction(&parsed.types)?;
    Ok(relation)
}
