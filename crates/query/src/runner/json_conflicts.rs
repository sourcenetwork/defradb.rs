use super::QueryRunner;
use crate::error::{QueryError, Result};
use crate::fetcher::{DocFetcher, JsonConflictOptions};
use crate::mapper::{Requestable, Select};
use crate::txn::TransactionRegistry;
use identity::Did;
use serde_json::{json, Map, Value};

impl<F: DocFetcher + 'static, R: TransactionRegistry> QueryRunner<F, R> {
    /// `_jsonConflicts(collection, docID, field)`: the edits each path of a
    /// Json field's winner beat, for a document the caller can read.
    pub(crate) async fn execute_json_conflicts_query(
        &self,
        select: &Select,
        fetcher: &dyn DocFetcher,
        caller: Option<Did>,
    ) -> Result<Value> {
        let usage = || QueryError::parse("_jsonConflicts requires collection, docID and field");
        let collection_name = select.arrival_collection.as_ref().ok_or_else(usage)?;
        let field = select.json_field.as_ref().ok_or_else(usage)?;
        let doc_id = match select.doc_ids.as_deref() {
            Some([doc_id]) => doc_id,
            _ => return Err(usage()),
        };
        let collection = self
            .effective_provider()
            .get_collection(collection_name)
            .await?
            .ok_or_else(|| QueryError::collection_not_found(collection_name))?;
        if let Some(policy) = &collection.policy {
            let allowed = crate::txn::check_doc_read_access_as(
                &self.context(),
                self.acp.as_ref(),
                self.acp_registration_projection.as_deref(),
                &acp::Identity::from(caller),
                &policy.id,
                &policy.resource_name,
                &collection.collection_id,
                collection.is_branchable,
                doc_id,
                None,
            )
            .await?;
            if !allowed {
                return Ok(Value::Array(Vec::new()));
            }
        }
        let conflicts = fetcher
            .get_json_conflicts(&JsonConflictOptions {
                collection: collection_name.clone(),
                doc_id: doc_id.clone(),
                field: field.clone(),
            })
            .await?;
        conflicts
            .into_iter()
            .map(|conflict| {
                let row = json!({
                    "path": conflict.path,
                    "cid": conflict.cid,
                    "height": conflict.height,
                    "value": conflict.value,
                    "deleted": conflict.deleted,
                });
                project(&row, &select.fields)
            })
            .collect::<Result<Vec<_>>>()
            .map(Value::Array)
    }
}

fn project(row: &Value, fields: &[Requestable]) -> Result<Value> {
    let mut output = Map::new();
    for field in fields {
        let Requestable::Field(field) = field else {
            return Err(QueryError::parse("invalid _jsonConflicts selection"));
        };
        if field.name == "__typename" {
            output.insert(field.output_name().into(), json!("JsonConflict"));
            continue;
        }
        let item = row.get(&field.name).ok_or_else(|| {
            QueryError::parse(format!("unknown _jsonConflicts field {}", field.name))
        })?;
        output.insert(field.output_name().into(), item.clone());
    }
    Ok(Value::Object(output))
}
