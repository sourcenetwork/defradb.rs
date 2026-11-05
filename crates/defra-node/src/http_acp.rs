//! HTTP document ACP calls use the node's relationship owner.

use std::sync::Arc;

use defra_core::RequestContext;
use defra_http::DocumentAcpOperations;
use identity::Did;

use crate::acp_ops::AcpOps;

pub(crate) struct HttpDocumentAcp {
    ops: Arc<dyn AcpOps>,
    context: Option<RequestContext>,
}

impl HttpDocumentAcp {
    pub(crate) fn new(ops: Arc<dyn AcpOps>) -> Self {
        Self { ops, context: None }
    }
}

#[async_trait::async_trait]
impl DocumentAcpOperations for HttpDocumentAcp {
    fn bind_request(&self, context: &RequestContext) -> Option<Arc<dyn DocumentAcpOperations>> {
        Some(Arc::new(Self {
            ops: self.ops.clone(),
            context: Some(context.clone()),
        }))
    }

    async fn check_doc_access(
        &self,
        actor: &Did,
        permission: acp::DocumentPermission,
        policy_id: &str,
        resource_name: &str,
        doc_id: &str,
    ) -> Result<bool, String> {
        let context = self
            .context
            .as_ref()
            .ok_or_else(|| "authentication required".to_owned())?;
        self.ops
            .check_doc_access(context, actor, permission, policy_id, resource_name, doc_id)
            .await
            .map_err(|error| error.to_string())
    }

    async fn add_doc_relationship(
        &self,
        requestor: &Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        let context = self
            .context
            .as_ref()
            .filter(|context| context.identity() == Some(requestor.as_str()))
            .ok_or_else(|| "request identity mismatch".to_owned())?;
        self.ops
            .add_dac_actor_relationship(
                context,
                requestor.as_str(),
                collection,
                doc_id,
                relation,
                target_actor,
            )
            .await
            .map(|existed_already| !existed_already)
            .map_err(|error| error.to_string())
    }

    async fn delete_doc_relationship(
        &self,
        requestor: &Did,
        target_actor: &str,
        collection: &str,
        doc_id: &str,
        relation: &str,
    ) -> Result<bool, String> {
        let context = self
            .context
            .as_ref()
            .filter(|context| context.identity() == Some(requestor.as_str()))
            .ok_or_else(|| "request identity mismatch".to_owned())?;
        self.ops
            .delete_dac_actor_relationship(
                context,
                requestor.as_str(),
                collection,
                doc_id,
                relation,
                target_actor,
            )
            .await
            .map_err(|error| error.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    #[derive(Default)]
    struct RecordContext(Mutex<Vec<(String, Option<String>)>>);

    #[async_trait::async_trait]
    impl AcpOps for RecordContext {
        async fn check_doc_access(
            &self,
            _context: &RequestContext,
            _actor: &Did,
            _permission: acp::DocumentPermission,
            _policy_id: &str,
            _resource: &str,
            _doc: &str,
        ) -> crate::Result<bool> {
            Ok(true)
        }

        async fn add_dac_actor_relationship(
            &self,
            context: &RequestContext,
            requestor: &str,
            _collection: &str,
            _doc: &str,
            _relation: &str,
            _target: &str,
        ) -> crate::Result<bool> {
            self.0
                .lock()
                .unwrap()
                .push((requestor.to_owned(), context.bearer_token_for(requestor)));
            Ok(false)
        }

        async fn delete_dac_actor_relationship(
            &self,
            context: &RequestContext,
            requestor: &str,
            collection: &str,
            doc: &str,
            relation: &str,
            target: &str,
        ) -> crate::Result<bool> {
            self.add_dac_actor_relationship(context, requestor, collection, doc, relation, target)
                .await
        }
    }

    #[test]
    fn relationship_calls_preserve_the_bound_callers_bearer() {
        workers::block_on(async {
            let ops = Arc::new(RecordContext::default());
            let adapter = HttpDocumentAcp::new(ops.clone());
            let caller = Did::new("did:key:caller").unwrap();
            let context = RequestContext::default()
                .with_identity(Some(caller.to_string()))
                .with_bearer(Some(caller.to_string()), Some("caller-token".into()));
            let bound = adapter.bind_request(&context).unwrap();
            bound
                .add_doc_relationship(&caller, "did:key:target", "Record", "doc", "reader")
                .await
                .unwrap();
            let other = Did::new("did:key:other").unwrap();
            assert!(bound
                .add_doc_relationship(&other, "did:key:target", "Record", "doc", "reader")
                .await
                .is_err());
            assert_eq!(
                *ops.0.lock().unwrap(),
                vec![(caller.to_string(), Some("caller-token".into()))]
            );
        })
        .unwrap();
    }
}
