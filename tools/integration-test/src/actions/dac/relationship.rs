//! What adding and deleting a document actor relationship share: the target
//! actor, the call each client makes, and the flag Go returns.

use serde_json::{json, Value};

use crate::scenario::{Common, Failure};
use crate::test_case::State;

/// Whether the call adds or deletes the relationship.
#[derive(Clone, Copy)]
pub(super) enum Change {
    Add,
    Delete,
}

impl Change {
    /// The key Go's result has: `ExistedAlready` for an add, `RecordFound`
    /// for a delete.
    fn key(self) -> &'static str {
        match self {
            Self::Add => "existedAlready",
            Self::Delete => "recordFound",
        }
    }

    fn printed_key(self) -> &'static str {
        match self {
            Self::Add => "ExistedAlready",
            Self::Delete => "RecordFound",
        }
    }

    fn cli(self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Delete => "delete",
        }
    }
}

/// One relationship change, with the target resolved.
pub(super) struct Relationship<'a> {
    pub change: Change,
    pub op: &'static str,
    pub common: &'a Common,
    pub collection: &'a str,
    pub doc_id: &'a str,
    pub relation: &'a str,
    pub target: String,
    pub txn: Option<usize>,
}

impl Relationship<'_> {
    fn txn(&self, state: &State) -> Result<Option<String>, Failure> {
        self.txn
            .map(|index| {
                state
                    .txns
                    .get(index)
                    .map(|id| id.to_string())
                    .ok_or_else(|| Failure::operation(format!("no transaction {index}")))
            })
            .transpose()
    }

    pub fn in_process(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.acting_identity(self.op, state)?;
        let node = state.node_on(self.common.node())?;
        let (collection, doc_id, relation) = (self.collection, self.doc_id, self.relation);
        let scoped = state
            .scope_as_on(self.common.node(), None, identity)
            .scoped(&node);
        let found = if let Some(index) = self.txn {
            let id = state
                .txns
                .get(index)
                .ok_or_else(|| Failure::operation(format!("no transaction {index}")))?;
            let scoped = scoped.with_transaction(id);
            match self.change {
                Change::Add => state.block_on(scoped.add_dac_actor_relationship(
                    collection,
                    doc_id,
                    relation,
                    &self.target,
                )),
                Change::Delete => state.block_on(scoped.delete_dac_actor_relationship(
                    collection,
                    doc_id,
                    relation,
                    &self.target,
                )),
            }
        } else {
            match self.change {
                Change::Add => state.block_on(scoped.add_dac_actor_relationship(
                    collection,
                    doc_id,
                    relation,
                    &self.target,
                )),
                Change::Delete => state.block_on(scoped.delete_dac_actor_relationship(
                    collection,
                    doc_id,
                    relation,
                    &self.target,
                )),
            }
        }
        .map_err(|e| Failure::operation(e.to_string()))?;
        Ok(json!({ self.change.key(): found }))
    }

    pub fn http(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.acting_identity(self.op, state)?;
        let change = self.change;
        let txn = self.txn(state)?;
        let (collection, doc_id) = (self.collection.to_owned(), self.doc_id.to_owned());
        let (relation, target) = (self.relation.to_owned(), self.target.clone());
        let printed = state
            .http_as(self.common.node(), identity, |client| async move {
                match change {
                    Change::Add => {
                        client
                            .acp_doc_relationship_add(
                                &collection,
                                &doc_id,
                                &relation,
                                &target,
                                txn.as_deref(),
                            )
                            .await
                    }
                    Change::Delete => {
                        client
                            .acp_doc_relationship_delete(
                                &collection,
                                &doc_id,
                                &relation,
                                &target,
                                txn.as_deref(),
                            )
                            .await
                    }
                }
            })?
            .map_err(|e| Failure::operation(e.to_string()))?;
        self.printed(&printed)
    }

    pub fn cli(&self, state: &State) -> Result<Value, Failure> {
        let identity = self.common.acting_identity(self.op, state)?;
        let txn = self.txn(state)?;
        let mut args = vec![
            "acp",
            "document",
            "relationship",
            self.change.cli(),
            "--collection",
            self.collection,
            "--docID",
            self.doc_id,
            "--relation",
            self.relation,
            "--actor",
            self.target.as_str(),
        ];
        if let Some(txn) = txn.as_deref() {
            args.extend(["--tx", txn]);
        }
        let printed = state
            .defra_client_as(self.common.node(), None, identity, &args)
            .map_err(Failure::operation)?;
        self.printed(&printed)
    }

    /// Go's flag as the HTTP API and the CLI print it.
    fn printed(&self, printed: &Value) -> Result<Value, Failure> {
        match printed
            .get(self.change.printed_key())
            .and_then(Value::as_bool)
        {
            Some(found) => Ok(json!({ self.change.key(): found })),
            None => Err(Failure::operation(format!(
                "no {} in {printed}",
                self.change.printed_key()
            ))),
        }
    }
}
