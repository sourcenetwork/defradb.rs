//! Query command implementation

use std::path::PathBuf;

use clap::Args;
use serde_json::Value as JsonValue;

use super::http_client::HttpClient;
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

        if response.has_errors() {
            return Err(Error::Server(response.error_message()));
        }

        let data = response.data.unwrap_or(serde_json::Value::Null);
        let wrapped = serde_json::json!({ "data": data });
        let output = serde_json::to_string_pretty(&wrapped)?;
        println!("{output}");

        Ok(())
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
