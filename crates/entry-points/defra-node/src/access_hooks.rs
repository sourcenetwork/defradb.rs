use std::sync::Arc;

use db::merge::governance::{MergeGovernance, MergeValidator};
use query::access_hooks::{ReadValidator, WriteValidator};

/// An app's own access control for an iroh node.
///
/// The merge validator decides whether a replicated composite merges into a
/// collection named in [`AccessHooks::new`] and, once a merge validator is
/// installed, into every collection whose definition declares a governance
/// root (`@governed(root:)`) and carries no `@policy`, named or not, so one
/// defined after the node started is judged from its first composite. Most of
/// the ACP merge hook does not run for a judged collection, and a collection
/// with `@policy` is judged only when named (see [`MergeGovernance`]). A named
/// collection with no merge validator merges nothing replicated; with no
/// merge validator, a collection that declares a root and is not named
/// merges as any other.
/// The read and write validators are node-local and narrow what this node's
/// clients may do; they compose with ACP and never widen it.
#[derive(Clone, Default)]
pub struct AccessHooks {
    governed: Vec<String>,
    merge: Option<Arc<dyn MergeValidator>>,
    read: Option<Arc<dyn ReadValidator>>,
    write: Option<Arc<dyn WriteValidator>>,
}

impl AccessHooks {
    /// Claim collections, by name, for the app's merge validator. A merge
    /// validator also judges every collection that declares a governance
    /// root and carries no `@policy`, named or not.
    pub fn new(governed: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            governed: governed.into_iter().map(Into::into).collect(),
            ..Self::default()
        }
    }

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
