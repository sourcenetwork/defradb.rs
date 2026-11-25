//! Query command implementation

use std::path::PathBuf;

use clap::Args;
use serde::Serialize;
use serde_json::Value as JsonValue;

use super::http_client::{GraphQLError, GraphQLResponse, HttpClient};
use super::ClientContext;
use crate::error::{Error, Result};

/// Execute a GraphQL query against a DefraDB node
#[derive(Args, Debug)]
pub struct QueryArgs {
    /// The GraphQL query to execute
    #[arg(value_name = "QUERY")]
    pub query: Option<String>,

    /// Path to a file containing the GraphQL query
    #[arg(long, short = 'f', conflicts_with = "query")]
    pub file: Option<PathBuf>,

    /// Name of the operation to execute in the query
    #[arg(long, short = 'o')]
    pub operation: Option<String>,

    /// Variables to pass to the query (JSON format)
    #[arg(long, short = 'v')]
    pub variables: Option<String>,
}

impl QueryArgs {
    /// Execute the query command
    pub async fn execute(&self, ctx: &ClientContext) -> Result<()> {
        let query = self.get_query()?;
        let variables = self.parse_variables()?;

        let client = HttpClient::new(&ctx.url, &ctx.edge)?
            .with_auth_token(ctx.auth_token.clone())
            .with_verbose(ctx.verbose);
        let response = client.graphql(&query, variables, ctx.tx_id.clone()).await?;

        println!("{}", printed(&response)?);
        match response.has_errors() {
            true => Err(Error::Server(response.error_message())),
            false => Ok(()),
        }
    }

    fn get_query(&self) -> Result<String> {
        if let Some(ref query) = self.query {
            return Ok(query.clone());
        }

        if let Some(ref path) = self.file {
            return std::fs::read_to_string(path).map_err(|e| Error::ReadFile {
                path: path.clone(),
                source: e,
            });
        }

        Err(Error::MissingInput(
            "either a query or --file must be provided".to_string(),
        ))
    }

    fn parse_variables(&self) -> Result<Option<JsonValue>> {
        match &self.variables {
            Some(vars) => {
                let parsed: JsonValue = serde_json::from_str(vars)?;
                Ok(Some(parsed))
            }
            None => Ok(None),
        }
    }
}

/// A GraphQL response as `client query` prints it: its errors, its data, and
/// its extensions when it has some, in the key order Go's `client query`
/// prints. Each error keeps the server's `extensions`, so `extensions.code`
/// reaches the user; Go prints an error's message only, so the error
/// `extensions` are ours.
#[derive(Serialize)]
struct Printed<'a> {
    #[serde(skip_serializing_if = "<[GraphQLError]>::is_empty")]
    errors: &'a [GraphQLError],
    data: &'a JsonValue,
    #[serde(skip_serializing_if = "Option::is_none")]
    extensions: Option<&'a JsonValue>,
}

fn printed(response: &GraphQLResponse) -> Result<String> {
    let printed = Printed {
        errors: &response.errors,
        data: response.data.as_ref().unwrap_or(&JsonValue::Null),
        extensions: response.extensions.as_ref(),
    };
    Ok(serde_json::to_string_pretty(&printed)?)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_query_args_get_query_inline() {
        let args = QueryArgs {
            query: Some("{ Users { name } }".to_string()),
            file: None,
            operation: None,
            variables: None,
        };
        assert_eq!(args.get_query().unwrap(), "{ Users { name } }");
    }

    #[test]
    fn test_query_args_parse_variables() {
        let args = QueryArgs {
            query: Some("{ Users { name } }".to_string()),
            file: None,
            operation: None,
            variables: Some(r#"{"limit": 10}"#.to_string()),
        };
        let vars = args.parse_variables().unwrap().unwrap();
        assert_eq!(vars["limit"], 10);
    }

    #[test]
    fn test_query_args_no_query() {
        let args = QueryArgs {
            query: None,
            file: None,
            operation: None,
            variables: None,
        };
        assert!(args.get_query().is_err());
    }

    #[test]
    fn an_error_prints_with_its_code_before_null_data() {
        let response: GraphQLResponse = serde_json::from_value(serde_json::json!({
            "data": null,
            "errors": [{
                "message": "refused",
                "extensions": {"code": "PERMISSION_DENIED"},
            }],
        }))
        .unwrap();
        let printed = printed(&response).unwrap();
        assert_eq!(
            printed,
            "{\n  \"errors\": [\n    {\n      \"message\": \"refused\",\n      \"extensions\": {\n        \"code\": \"PERMISSION_DENIED\"\n      }\n    }\n  ],\n  \"data\": null\n}"
        );
    }

    #[test]
    fn an_error_keeps_every_extension_the_server_sent() {
        let response: GraphQLResponse = serde_json::from_value(serde_json::json!({
            "data": null,
            "errors": [
                {"message": "plain"},
                {
                    "message": "outside",
                    "extensions": {"code": "DELEGATION_OUTSIDE_WINDOW", "providerTime": 7},
                },
            ],
        }))
        .unwrap();
        let printed: JsonValue = serde_json::from_str(&printed(&response).unwrap()).unwrap();
        assert_eq!(
            printed["errors"],
            serde_json::json!([
                {"message": "plain"},
                {
                    "message": "outside",
                    "extensions": {"code": "DELEGATION_OUTSIDE_WINDOW", "providerTime": 7},
                },
            ])
        );
    }

    #[test]
    fn a_result_prints_its_data_and_extensions_only() {
        let response: GraphQLResponse = serde_json::from_value(serde_json::json!({
            "data": {"User": []},
            "extensions": {"warnings": []},
        }))
        .unwrap();
        let printed: JsonValue = serde_json::from_str(&printed(&response).unwrap()).unwrap();
        assert_eq!(
            printed,
            serde_json::json!({"data": {"User": []}, "extensions": {"warnings": []}})
        );
    }

    #[test]
    fn test_query_short_variables_flag() {
        #[derive(clap::Parser)]
        struct Cli {
            #[command(flatten)]
            args: QueryArgs,
        }
        use clap::Parser;
        let cli = Cli::try_parse_from(["q", "{ a }", "-v", r#"{"x":1}"#]).unwrap();
        assert_eq!(cli.args.variables.as_deref(), Some(r#"{"x":1}"#));
        assert!(Cli::try_parse_from(["q", "{ a }", "--txn-id", "1"]).is_err());
    }
}
