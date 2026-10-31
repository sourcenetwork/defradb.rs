//! Running one request on a transaction's turn, where the transaction lives.
//!
//! The edge submits the request to the registry ([`TransactionRegistry::run`]
//! for an explicit transaction, [`TransactionRegistry::read`] for a
//! standalone query's snapshot) and waits for one answer. On the transaction's
//! side the request parses, opens its statement when it writes, borrows the
//! transaction for its handles, runs, drops every handle and closes the
//! statement. A remote input the statement needs (an embedding, a KMS key) is
//! never fetched there: the statement ends undone, the edge fetches the
//! input, and submits the request again with the input in its memo.

use std::sync::Arc;
use std::time::Duration;

use identity::Did;

use crate::error::{QueryError, TransactionError};
use crate::executor::{GqlWarning, QueryRequest, QueryResponse, QueryResponseError};
use crate::mutator::DocMutator;
use crate::prepared::{PreparedInputMemo, PreparedInputRequest};
use crate::query_parse::{parse_request_with_limits, validate_parsed_operation, ParsedOperation};
use crate::txn::{txn_job, TransactionRegistry, TxnJob, TxnTurn};

use super::executor::{check_nac, convert_variables, permission_for_operation};
use super::{DocFetcher, QueryRunner};

/// How one run of a request on a transaction ended.
pub(super) enum Turned {
    /// It answered.
    Done(QueryResponse),
    /// Its statement needs a remote input, and was undone: fetch the input
    /// and run the request again. The transaction's mutator comes along to
    /// fetch it with when the runner has none of its own.
    NeedsInput(PreparedInputRequest, Option<Arc<dyn DocMutator>>),
}

fn parse_error(message: String) -> QueryResponse {
    QueryResponse {
        data: None,
        errors: vec![QueryResponseError {
            message,
            path: None,
            locations: None,
            extensions: None,
            retry_input: None,
        }],
        extensions: None,
    }
}

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    /// The runner's deadline for one run, `None` for none.
    pub(super) fn deadline(&self) -> Option<Duration> {
        (self.query_timeout > 0).then(|| Duration::from_secs(self.query_timeout))
    }

    /// Runs `request` through `submit` (a registry call that runs a job where
    /// the transaction lives) until it answers, fetching at the edge every
    /// remote input a run asks for, in the context of the request this
    /// runner serves. `parsed`, the request already parsed, serves the first
    /// run; a run after an input parses the request again.
    pub(super) async fn drive<Submit, Fut>(
        &self,
        request: &QueryRequest,
        mut parsed: Option<ParsedOperation>,
        submit: Submit,
    ) -> std::result::Result<QueryResponse, TransactionError>
    where
        Submit: Fn(TxnJob<Turned>) -> Fut,
        Fut: std::future::Future<Output = std::result::Result<Turned, TransactionError>>,
    {
        let inputs = PreparedInputMemo::default();
        let context = self.context();
        loop {
            let runner = self.scoped_runner(self.acp.clone());
            let attempt = request.clone();
            let memo = inputs.clone();
            let parsed = parsed.take();
            let job = txn_job(move |turn| {
                Box::pin(async move { runner.run_turn(turn, attempt, memo, parsed).await })
            });
            let (input, txn_mutator) = match submit(job).await? {
                Turned::Done(response) => return Ok(response),
                Turned::NeedsInput(input, txn_mutator) => (input, txn_mutator),
            };
            let Some(resolver) = self.mutator.clone().or(txn_mutator) else {
                return Ok(QueryResponse::error(QueryResponseError::from_query_error(
                    QueryError::NeedsInput(input),
                )));
            };
            if let Err(error) = inputs
                .answer_wave(&input, |wave| {
                    resolver.resolve_prepared_inputs_as(&context, wave)
                })
                .await
            {
                return Ok(QueryResponse::error(QueryResponseError::from_query_error(
                    error,
                )));
            }
        }
    }

    /// One run of `request` on `turn`: parse (unless `parsed` already holds
    /// the parse), open the statement when it writes, run with the
    /// transaction lent, close the statement.
    pub(super) async fn run_turn(
        &self,
        turn: &mut dyn TxnTurn,
        request: QueryRequest,
        inputs: PreparedInputMemo,
        parsed: Option<ParsedOperation>,
    ) -> Turned {
        let variables = convert_variables(&request.variables);
        let parsed = match parsed {
            Some(parsed) => parsed,
            None => match parse_request_with_limits(
                &request.query,
                variables.as_ref(),
                request.operation_name.as_deref(),
                self.query_limits,
            ) {
                Ok(parsed) => parsed,
                Err(error) => return Turned::Done(parse_error(error.to_string())),
            },
        };
        let statement = !turn.is_readonly()
            && matches!(&parsed, ParsedOperation::Mutation { explain: None, .. });
        if statement {
            if let Err(error) = turn.begin_statement() {
                return Turned::Done(QueryResponse::error(format!(
                    "failed to establish transaction statement savepoint: {error}"
                )));
            }
        }
        let turned = self
            .lent(turn, parsed, request, variables, inputs)
            .await
            .unwrap_or_else(|error| Turned::Done(QueryResponse::error(error.to_string())));
        if statement {
            let keep = matches!(&turned, Turned::Done(response) if response.errors.is_empty());
            if let Err(error) = turn.end_statement(keep) {
                return Turned::Done(QueryResponse::error(format!(
                    "failed to finish transaction statement savepoint: {error}"
                )));
            }
        }
        turned
    }

    /// The part of a run that holds the transaction: every handle it makes
    /// is dropped when it returns.
    async fn lent(
        &self,
        turn: &mut dyn TxnTurn,
        parsed: ParsedOperation,
        request: QueryRequest,
        variables: Option<rapidhash::RapidHashMap<String, serde_json::Value>>,
        inputs: PreparedInputMemo,
    ) -> std::result::Result<Turned, TransactionError> {
        let txn_ctx = turn.context()?;
        // The transaction's own provider, so its uncommitted schemas are visible.
        let txn_provider = txn_ctx
            .collection_provider()
            .unwrap_or_else(|| self.collection_provider.clone());

        // Validate that all referenced collections exist before execution.
        if let Err(error) = validate_parsed_operation(&parsed, txn_provider.as_ref()).await {
            return Ok(Turned::Done(parse_error(error.to_string())));
        }

        let nac_identity = request.identity.clone().unwrap_or_else(Did::wildcard);
        match txn_ctx
            .doc_fetcher()
            .check_node_permission(&nac_identity, permission_for_operation(&parsed))
            .await
        {
            Ok(Some(false)) => {
                return Ok(Turned::Done(QueryResponse::success(serde_json::json!({}))))
            }
            Ok(Some(true)) => {}
            Ok(None) => {
                if let Some(denial) = check_nac(self, &request.identity, &parsed).await {
                    return Ok(Turned::Done(denial));
                }
            }
            Err(error) => return Ok(Turned::Done(QueryResponse::error(error.to_string()))),
        }

        let identity = self.resolve_identity(request.identity);
        let mut warnings: Vec<GqlWarning> = Vec::new();

        let scoped_acp = match txn_ctx.doc_fetcher().scoped_acp(self.acp.clone()).await {
            Ok(provider) => provider,
            Err(error) => return Ok(Turned::Done(QueryResponse::error(error.to_string()))),
        };
        let mut scoped_runner = self.scoped_runner(scoped_acp);
        scoped_runner.collection_provider = txn_provider.clone();
        if !scoped_runner.acp.supports_transactions() {
            // Even transaction-context implementations without an ACP
            // projection must never fall back to mutating an external
            // provider before the storage transaction commits. The mutator's
            // ordered hook registration will fail closed if it cannot retain
            // hooks.
            scoped_runner.acp_registration_projection = Some(
                txn_ctx
                    .acp_registration_projection()
                    .unwrap_or_else(crate::txn::AcpRegistrationProjection::detached_shared),
            );
        }

        let execution = async {
            match parsed {
                ParsedOperation::Query {
                    mut selects,
                    explain,
                    exhaustive,
                } => {
                    if exhaustive {
                        for select in &mut selects {
                            select.exhaustive = true;
                        }
                    }
                    let fetcher = txn_ctx.doc_fetcher();
                    if let Some(explain_type) = explain {
                        self.with_collection_provider(txn_provider)
                            .explain_query_with_identity_and_vars(
                                &request.query,
                                identity,
                                explain_type,
                                variables.as_ref(),
                            )
                            .await
                    } else {
                        scoped_runner
                            .execute_selects_internal(
                                selects,
                                fetcher.as_ref(),
                                identity,
                                &mut warnings,
                            )
                            .await
                    }
                }
                ParsedOperation::Mutation { explain, .. } => {
                    if let Some(explain_type) = explain {
                        self.with_collection_provider(txn_provider)
                            .explain_mutation_with_identity(&request.query, identity, explain_type)
                            .await
                    } else {
                        if txn_ctx.is_readonly() {
                            return Err(QueryError::execution(
                                "No sets or deletes are allowed in a read-only transaction",
                            ));
                        }
                        let mutator = txn_ctx.doc_mutator().ok_or_else(|| {
                            QueryError::execution(
                                "mutations not supported in this transaction context",
                            )
                        })?;
                        mutator.set_document_acp(self.acp.clone());
                        let attempt = mutator
                            .with_prepared_inputs(inputs.clone())
                            .unwrap_or_else(|| mutator.clone());
                        scoped_runner
                            .execute_mutation_internal_with_vars(
                                &request.query,
                                attempt,
                                identity,
                                variables.as_ref(),
                                Some(txn_ctx.doc_fetcher()),
                            )
                            .await
                    }
                }
                ParsedOperation::Subscription { .. } => Err(QueryError::parse(
                    "Subscriptions must be executed via Server-Sent Events (SSE). \
                     Send the request with Accept: text/event-stream header.",
                )),
                ParsedOperation::Introspection { query } => {
                    scoped_runner.execute_introspection(&query).await
                }
            }
        };

        let result = execution.await;

        Ok(Turned::Done(match result {
            Ok(data) => QueryResponse {
                data: Some(data),
                errors: vec![],
                extensions: None,
            }
            .with_warnings(warnings),
            Err(QueryError::NeedsInput(input)) => {
                return Ok(Turned::NeedsInput(input, txn_ctx.doc_mutator()))
            }
            Err(error) => {
                tracing::debug!(
                    txn_id = txn_ctx.id(),
                    error = %error,
                    "Query execution failed in transaction"
                );
                QueryResponse {
                    data: None,
                    errors: vec![QueryResponseError::from_query_error(error)],
                    extensions: None,
                }
                .with_warnings(warnings)
            }
        }))
    }
}

/// A registry's answer to a run, as the response its caller gets.
pub(super) fn answered(
    outcome: std::result::Result<QueryResponse, TransactionError>,
) -> QueryResponse {
    match outcome {
        Ok(response) => response,
        Err(TransactionError::NotFound(_)) => QueryResponse::error("transaction not found"),
        Err(TransactionError::Execution(message)) => QueryResponse::error(
            QueryResponseError::from_query_error(QueryError::execution(message)),
        ),
        Err(TransactionError::Storage(cause)) => QueryResponse::error(
            QueryResponseError::from_query_error(QueryError::Storage(cause)),
        ),
        Err(error) => QueryResponse::error(error.to_string()),
    }
}

#[cfg(test)]
mod storage_failure_tests {
    use super::*;

    #[test]
    fn transaction_storage_failure_retains_code_without_authorizing_retry() {
        let error = TransactionError::Storage(storage::Error::from(std::io::Error::new(
            std::io::ErrorKind::StorageFull,
            "localized storage failure",
        )));
        assert!(!error.is_retryable());
        let response = answered(Err(error));
        assert_eq!(
            response.errors[0].code(),
            Some(crate::executor::STORAGE_FULL_ERROR_CODE)
        );
        assert!(!response.is_transaction_conflict());
    }
}
