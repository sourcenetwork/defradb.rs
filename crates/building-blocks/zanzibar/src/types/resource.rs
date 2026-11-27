use serde::{Deserialize, Serialize};

use super::subject::SubjectRestriction;
use crate::expression::RelationExpression;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Resource {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub relations: Vec<Relation>,
    /// The `create` expression of the resource's `collection` section: who
    /// may add an object to it. The engine never evaluates it; a host that
    /// gates creates reads it. Absent, and `"*"`, mean anyone may.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub collection_create: Option<String>,
}

impl Resource {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            relations: Vec::new(),
            collection_create: None,
        }
    }

    /// Whether adding an object to this resource is restricted: a `create`
    /// is present and is not `"*"`.
    pub fn is_create_gated(&self) -> bool {
        self.collection_create
            .as_deref()
            .is_some_and(|create| create.trim() != "*")
    }

    pub fn with_relation(mut self, relation: Relation) -> Self {
        self.relations.push(relation);
        self
    }

    pub fn get_relation(&self, name: &str) -> Option<&Relation> {
        self.relations.iter().find(|r| r.name == name)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Relation {
    pub name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    pub expression: RelationExpression,
    #[serde(default)]
    pub subject_restriction: Option<SubjectRestriction>,
    #[serde(default)]
    pub manages: Vec<String>,
}

impl Relation {
    pub fn direct(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            expression: RelationExpression::This,
            subject_restriction: None,
            manages: Vec::new(),
        }
    }

    pub fn computed(name: impl Into<String>, expression: RelationExpression) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            expression,
            subject_restriction: None,
            manages: Vec::new(),
        }
    }

    pub fn with_restriction(mut self, restriction: SubjectRestriction) -> Self {
        self.subject_restriction = Some(restriction);
        self
    }

    pub fn with_manages(mut self, manages: Vec<impl Into<String>>) -> Self {
        self.manages = manages.into_iter().map(|s| s.into()).collect();
        self
    }
}
