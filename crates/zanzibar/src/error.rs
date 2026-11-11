use thiserror::Error;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum Error {
    #[error("a different policy already exists under ID {0}")]
    PolicyAlreadyExists(String),

    #[error("cannot resolve an exclusion through a relationship cycle")]
    IndeterminateExclusion,

    #[error("permission evaluation {0} limit exceeded")]
    EvaluationLimitExceeded(&'static str),

    #[error(
        "the permission check needs a store read that is not ready; use the asynchronous check"
    )]
    EvaluationPending,

    #[error("serialization error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("invalid DID: {0}")]
    InvalidDid(String),

    #[error("policy not found: {0}")]
    PolicyNotFound(String),

    #[error("relation not found: {relation} in resource {resource}")]
    RelationNotFound { resource: String, relation: String },

    #[error("resource not found: {0}")]
    ResourceNotFound(String),

    #[error("invalid expression: {0}")]
    InvalidExpression(String),

    #[error("invalid policy: {0}")]
    InvalidPolicy(String),

    #[error("invalid EntitySet reference: resource '{resource}' relation '{relation}' does not exist in policy")]
    InvalidEntitySetReference { resource: String, relation: String },

    #[error("subject restriction violated: {message}")]
    SubjectRestrictionViolation { message: String },

    #[error("DPI violation: resource '{resource}' must have an 'owner' relation")]
    DpiMissingOwner { resource: String },

    #[error("DPI violation: permission '{relation}' on resource '{resource}' must include 'owner' in its expression")]
    DpiExpressionMissingOwner { resource: String, relation: String },

    #[error("DPI violation: resource '{resource}' relation '{relation}' uses disallowed operation '{operation}'")]
    DpiDisallowedOperation {
        resource: String,
        relation: String,
        operation: String,
    },

    #[error("invalid relationship field '{field}': {reason}")]
    InvalidRelationshipField { field: String, reason: String },

    #[error("serialization error: {0}")]
    Serialization(String),

    #[error("invalid subject encoding: {0}")]
    InvalidSubjectEncoding(String),

    /// A scope's commit lost to a transaction that committed first and
    /// changed something the scope read; a fresh run decides again.
    #[error("transaction conflict: {0}")]
    TransactionConflict(String),
}

impl Error {
    /// Whether a scope's commit lost to another commit, so a fresh run may
    /// succeed.
    pub fn is_conflict(&self) -> bool {
        matches!(self, Error::TransactionConflict(_))
    }
}

#[cfg(test)]
mod tests {
    use super::Error;

    #[test]
    fn json_errors_preserve_their_type() {
        let err = serde_json::from_str::<serde_json::Value>("{").unwrap_err();
        let error: Error = err.into();
        assert!(matches!(error, Error::Json(_)));
        assert!(error.to_string().starts_with("serialization error: "));
    }
}
