//! The GraphQL response envelope, as `GET /graphql` answers it.

use serde::{Deserialize, Serialize};

/// [`query::executor::ErrorLocation`]: where in the document an error was.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct ErrorLocation {
    pub line: u32,
    pub column: u32,
}

/// [`query::executor::QueryResponseErrorExtensions`]: machine-readable metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QueryResponseErrorExtensions {
    pub code: String,
    #[serde(
        default,
        rename = "providerTime",
        skip_serializing_if = "Option::is_none"
    )]
    pub provider_time: Option<u64>,
}

/// [`query::executor::QueryResponseError`]: one error in the response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QueryResponseError {
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub path: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub locations: Option<Vec<ErrorLocation>>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub extensions: Option<QueryResponseErrorExtensions>,
}

/// [`query::executor::GqlWarning`]: something that happened in a request that
/// still worked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GqlWarning {
    pub code: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub detail: Option<serde_json::Map<String, serde_json::Value>>,
}

/// [`query::executor::GqlExtensions`]: what a working request adds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct GqlExtensions {
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub warnings: Vec<GqlWarning>,
}

/// [`query::executor::QueryResponse`]: the envelope every GraphQL answer has.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, schemars::JsonSchema)]
pub struct QueryResponse {
    /// The result, absent when errors stopped it.
    pub data: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub errors: Vec<QueryResponseError>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub extensions: Option<GqlExtensions>,
}

impl From<query::executor::GqlWarning> for GqlWarning {
    fn from(warning: query::executor::GqlWarning) -> Self {
        Self {
            code: warning.code,
            message: warning.message,
            detail: warning.detail,
        }
    }
}

impl From<query::executor::QueryResponse> for QueryResponse {
    fn from(response: query::executor::QueryResponse) -> Self {
        Self {
            data: response.data,
            errors: response
                .errors
                .into_iter()
                .map(|error| QueryResponseError {
                    message: error.message,
                    path: error.path,
                    locations: error.locations.map(|locations| {
                        locations
                            .into_iter()
                            .map(|location| ErrorLocation {
                                line: location.line,
                                column: location.column,
                            })
                            .collect()
                    }),
                    extensions: error
                        .extensions
                        .map(|extensions| QueryResponseErrorExtensions {
                            code: extensions.code,
                            provider_time: extensions.provider_time,
                        }),
                })
                .collect(),
            extensions: response.extensions.map(|extensions| GqlExtensions {
                warnings: extensions.warnings.into_iter().map(Into::into).collect(),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An envelope with data, an error carrying every optional field, and a
    /// warning serializes identically through the mirror.
    #[test]
    fn query_response_matches_its_source() {
        let mut response = query::executor::QueryResponse::success(serde_json::json!({
            "Users": []
        }));
        response.errors.push(
            query::executor::QueryResponseError::new("bad field")
                .with_path(vec!["Users".to_string()])
                .with_code("UNKNOWN_MUTATION_COLLECTION"),
        );
        assert_eq!(
            serde_json::to_value(QueryResponse::from(response.clone())).unwrap(),
            serde_json::to_value(&response).unwrap()
        );
    }

    /// The absent pieces are left out, as the source leaves them.
    #[test]
    fn an_empty_envelope_omits_errors_and_extensions() {
        let response = query::executor::QueryResponse {
            data: None,
            errors: Vec::new(),
            extensions: None,
        };
        let value = serde_json::to_value(QueryResponse::from(response.clone())).unwrap();
        assert_eq!(value, serde_json::to_value(&response).unwrap());
        assert!(value.get("errors").is_none());
    }
}
