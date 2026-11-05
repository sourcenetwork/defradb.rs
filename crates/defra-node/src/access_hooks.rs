use std::sync::Arc;

#[cfg(feature = "governed-collections")]
use db::merge::governance::{MergeGovernance, MergeValidator};
use query::access_hooks::{ReadValidator, WriteValidator};

/// An app's own access control for an iroh node.
///
/// With `governed-collections`, the collections named in [`AccessHooks::new`]
/// are governed by the app: replicated composites into them are judged by the
/// merge validator, never by ACP, and a governed collection with no merge
/// validator merges nothing. The read and write validators are node-local and
/// narrow what this node's clients may do; they compose with ACP and never
/// widen it.
#[derive(Clone, Default)]
pub struct AccessHooks {
    #[cfg(feature = "governed-collections")]
    governed: Vec<String>,
    #[cfg(feature = "governed-collections")]
    merge: Option<Arc<dyn MergeValidator>>,
    read: Option<Arc<dyn ReadValidator>>,
    write: Option<Arc<dyn WriteValidator>>,
}

impl AccessHooks {
    /// Claim collections, by name, for the app's merge validator.
    #[cfg(feature = "governed-collections")]
    pub fn new(governed: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            governed: governed.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

    #[cfg(feature = "governed-collections")]
    pub fn with_merge_validator(mut self, validator: Arc<dyn MergeValidator>) -> Self {
        self.merge = Some(validator);
        self
    }

    pub fn with_read_validator(mut self, validator: Arc<dyn ReadValidator>) -> Self {
        self.read = Some(validator);
        self
    }

    pub fn with_write_validator(mut self, validator: Arc<dyn WriteValidator>) -> Self {
        self.write = Some(validator);
        self
    }

    #[cfg(feature = "governed-collections")]
    pub(crate) fn install<S>(&self, database: &db::DB<S>)
    where
        S: storage::corekv::Store,
    {
        let governance = MergeGovernance::new(self.governed.iter().cloned());
        database.set_merge_governance(match &self.merge {
            Some(validator) => governance.with_validator(validator.clone()),
            None => governance,
        });
    }

    pub(crate) fn apply<F, R>(&self, runner: query::QueryRunner<F, R>) -> query::QueryRunner<F, R>
    where
        F: query::runner::DocFetcher + 'static,
        R: query::txn::TransactionRegistry,
    {
        let runner = match &self.read {
            Some(validator) => runner.with_read_validator(validator.clone()),
            None => runner,
        };
        match &self.write {
            Some(validator) => runner.with_write_validator(validator.clone()),
            None => runner,
        }
    }
}
