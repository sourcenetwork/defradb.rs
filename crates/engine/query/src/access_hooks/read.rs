use std::sync::Arc;

use async_trait::async_trait;
use defra_core::thread_bounds::MaybeSendSync;
use identity::Did;
use schema::CollectionVersion;

/// One document a client of this node asks to read.
pub struct ReadRequest<'a> {
    pub identity: Option<&'a Did>,
    pub collection: &'a CollectionVersion,
    pub doc_id: &'a str,
}

/// Decides which documents clients of this node may read.
///
/// Consulted for user queries, nested relations, aggregates, time-travel
/// queries, encrypted-index results and `_commits`, after any ACP check, so it
/// can only hide what ACP would show. It is node-local and may read anything
/// the node knows. An `Err` hides the document.
#[cfg_attr(not(target_arch = "wasm32"), async_trait)]
#[cfg_attr(target_arch = "wasm32", async_trait(?Send))]
pub trait ReadValidator: MaybeSendSync {
    /// Whether reads of this collection go through [`Self::may_read`].
    fn governs(&self, collection: &CollectionVersion) -> bool;

    async fn may_read(&self, request: &ReadRequest<'_>) -> Result<bool, String>;
}

/// A read validator bound to one query's identity and collection.
#[derive(Clone)]
pub struct AppReadCheck {
    validator: Arc<dyn ReadValidator>,
    identity: Option<Did>,
    collection: Arc<CollectionVersion>,
    retry_inputs: Option<crate::prepared::PreparedInputMemo>,
}

impl AppReadCheck {
    /// `None` when no validator is installed or it does not govern the
    /// collection, so ungoverned reads pay nothing.
    pub fn bind(
        validator: Option<&Arc<dyn ReadValidator>>,
        identity: Option<Did>,
        collection: &CollectionVersion,
    ) -> Option<Self> {
        let validator = validator.filter(|validator| validator.governs(collection))?;
        Some(Self {
            validator: validator.clone(),
            identity,
            collection: Arc::new(collection.clone()),
            retry_inputs: None,
        })
    }

    pub(crate) fn with_retry_inputs(
        mut self,
        inputs: Option<crate::prepared::PreparedInputMemo>,
    ) -> Self {
        self.retry_inputs = inputs;
        self
    }

    pub async fn allows(&self, doc_id: &str) -> crate::error::Result<bool> {
        if let Some(inputs) = &self.retry_inputs {
            let request = ReadValidationInput {
                identity: self.identity.as_ref().map(ToString::to_string),
                collection: (*self.collection).clone(),
                doc_id: doc_id.to_owned(),
            };
            let request = crate::prepared::PreparedInputRequest::new(
                crate::prepared::PreparedInputKind::ReadValidation,
                serde_json::to_vec(
                    &serde_json::to_value(&request)
                        .map_err(|error| crate::error::QueryError::execution(error.to_string()))?,
                )
                .map_err(|error| crate::error::QueryError::execution(error.to_string()))?,
            );
            let Some(response) = inputs.get(&request) else {
                return Err(crate::error::QueryError::NeedsInput(request));
            };
            return serde_json::from_slice(&response)
                .map_err(|error| crate::error::QueryError::execution(error.to_string()));
        }
        let request = ReadRequest {
            identity: self.identity.as_ref(),
            collection: &self.collection,
            doc_id,
        };
        Ok(self.validator
            .may_read(&request)
            .await
            .unwrap_or_else(|error| {
                tracing::warn!(%doc_id, collection = %self.collection.name, %error, "Read validator failed, hiding document");
                false
            }))
    }
}

/// Whether `check` lets `doc_id` through; no check lets everything through.
pub(crate) async fn app_allows(
    check: Option<&AppReadCheck>,
    doc_id: &str,
) -> crate::error::Result<bool> {
    match check {
        Some(check) => check.allows(doc_id).await,
        None => Ok(true),
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
pub(crate) struct ReadValidationInput {
    pub identity: Option<String>,
    #[serde(with = "super::collection_input")]
    pub collection: CollectionVersion,
    pub doc_id: String,
}

#[cfg(test)]
mod retry_tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Validator(AtomicUsize);

    #[async_trait]
    impl ReadValidator for Validator {
        fn governs(&self, _: &CollectionVersion) -> bool {
            true
        }
        async fn may_read(&self, _: &ReadRequest<'_>) -> Result<bool, String> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(true)
        }
    }

    #[test]
    fn prepared_read_validation_defers_provider_and_keys_all_decision_inputs() {
        workers::block_on(async {
            let provider = Arc::new(Validator(AtomicUsize::new(0)));
            let validator: Arc<dyn ReadValidator> = provider.clone();
            let mut schema = CollectionVersion::new("Users", "v1", "users", vec![]);
            schema.root_id = 42;
            let reader = Some(Did::new("did:key:reader").unwrap());
            let memo = crate::prepared::PreparedInputMemo::default();
            let check = AppReadCheck::bind(Some(&validator), reader.clone(), &schema)
                .unwrap()
                .with_retry_inputs(Some(memo.clone()));
            let crate::error::QueryError::NeedsInput(request) =
                check.allows("doc1").await.unwrap_err()
            else {
                panic!("expected input miss");
            };
            assert_eq!(provider.0.load(Ordering::SeqCst), 0);
            // Resolve through the ordinary provider path outside the strict attempt.
            let input: ReadValidationInput =
                serde_json::from_slice(request.exact_request()).unwrap();
            assert_eq!(input.identity.as_deref(), Some("did:key:reader"));
            assert_eq!(input.collection, schema);
            assert_eq!(input.doc_id, "doc1");
            let resolved = AppReadCheck::bind(Some(&validator), reader.clone(), &input.collection)
                .unwrap()
                .allows(&input.doc_id)
                .await
                .unwrap();
            memo.insert(request, serde_json::to_vec(&resolved).unwrap());
            assert!(check.allows("doc1").await.unwrap());
            assert_eq!(provider.0.load(Ordering::SeqCst), 1);
            assert!(matches!(
                check.allows("doc2").await,
                Err(crate::error::QueryError::NeedsInput(_))
            ));
            let other_reader = AppReadCheck::bind(Some(&validator), None, &schema)
                .unwrap()
                .with_retry_inputs(Some(memo.clone()));
            assert!(matches!(
                other_reader.allows("doc1").await,
                Err(crate::error::QueryError::NeedsInput(_))
            ));
            let changed_schema = CollectionVersion::new("Users", "v2", "users", vec![]);
            let other_schema = AppReadCheck::bind(Some(&validator), reader, &changed_schema)
                .unwrap()
                .with_retry_inputs(Some(memo));
            assert!(matches!(
                other_schema.allows("doc1").await,
                Err(crate::error::QueryError::NeedsInput(_))
            ));
            assert_eq!(provider.0.load(Ordering::SeqCst), 1);
        })
        .unwrap()
    }
}
