use super::QueryRunner;
use crate::error::{QueryError, Result};
use crate::fetcher::{DocFetcher, DocumentArrivalOptions};
use crate::mapper::{Requestable, Select};
use crate::txn::TransactionRegistry;
use identity::Did;
use serde_json::{json, Map, Value};

impl<F: DocFetcher + 'static, R: TransactionRegistry> QueryRunner<F, R> {
    pub(crate) async fn execute_arrivals_query(
        &self,
        select: &Select,
        fetcher: &dyn DocFetcher,
        caller: Option<Did>,
    ) -> Result<Value> {
        let collection_name = select
            .arrival_collection
            .as_ref()
            .ok_or_else(|| QueryError::parse("_documentArrivals requires collection"))?;
        if select.filter.is_some()
            || select.order_by.is_some()
            || select.group_by.is_some()
            || select.cid.is_some()
            || select.depth.is_some()
            || select.limit.as_ref().is_some_and(|v| v.offset != 0)
        {
            return Err(QueryError::parse(
                "_documentArrivals accepts collection, after, limit and docID only",
            ));
        }
        let collection = self
            .effective_provider()
            .get_collection(collection_name)
            .await?
            .ok_or_else(|| QueryError::collection_not_found(collection_name))?;
        let limit = select.limit.as_ref().and_then(|v| v.limit).unwrap_or(256);
        if limit == 0 || limit > 1024 || select.doc_ids.as_ref().is_some_and(|ids| ids.len() > 1024)
        {
            return Err(QueryError::parse(
                "arrival limit and docID count must be between 1 and 1024",
            ));
        }
        let page = fetcher
            .get_document_arrivals(&DocumentArrivalOptions {
                collection: collection_name.clone(),
                after: select.arrival_after,
                limit,
                doc_ids: select.doc_ids.clone(),
            })
            .await?;
        let identity = acp::Identity::from(caller);
        let checker = crate::txn::OverlayChecker {
            acp: self.acp.as_ref(),
            identity: &identity,
        };
        let mut entries = Vec::new();
        for entry in page.entries {
            let allowed = match &collection.policy {
                None => true,
                Some(policy) => acp::read_access::check_doc_read_access(
                    &checker,
                    &policy.id,
                    &policy.resource_name,
                    &collection.collection_id,
                    collection.is_branchable,
                    &entry.doc_id,
                )
                .await
                .map_err(|e| QueryError::execution(e.to_string()))?,
            };
            if allowed {
                entries.push(json!({"cursor": entry.cursor.to_string(), "docID": entry.doc_id}));
            }
        }
        project(
            &json!({"head":page.head.to_string(), "next":page.next.to_string(), "entries":entries}),
            &select.fields,
        )
    }
}

fn project(value: &Value, fields: &[Requestable]) -> Result<Value> {
    if let Value::Array(rows) = value {
        return rows
            .iter()
            .map(|row| project(row, fields))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
    }
    let mut output = Map::new();
    for field in fields {
        match field {
            Requestable::Field(field) => {
                let item = value.get(&field.name).ok_or_else(|| {
                    QueryError::parse(format!("unknown arrival field {}", field.name))
                })?;
                if item.is_array() || item.is_object() {
                    return Err(QueryError::parse(
                        "arrival entries requires a selection set",
                    ));
                }
                output.insert(field.output_name().into(), item.clone());
            }
            Requestable::Select(select) if select.field.name == "entries" => {
                output.insert(
                    select.field.output_name().into(),
                    project(&value["entries"], &select.fields)?,
                );
            }
            _ => return Err(QueryError::parse("invalid arrival selection")),
        }
    }
    Ok(Value::Object(output))
}
