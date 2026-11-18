use crate::Collection;
use acp::DocumentACP;

pub async fn resolve_push_creator(
    document_acp: Option<&dyn DocumentACP>,
    collection: &Collection,
    doc_id: &str,
    fallback_creator: &str,
) -> String {
    let Some(policy) = &collection.schema().policy else {
        return fallback_creator.to_string();
    };

    let mut resource_names = vec![policy.resource_name.clone()];
    for candidate in [
        collection.name().to_string(),
        collection.name().to_lowercase(),
        format!("{}s", collection.name().to_lowercase()),
    ] {
        if !resource_names.iter().any(|existing| existing == &candidate) {
            resource_names.push(candidate);
        }
    }

    let Some(acp) = document_acp else {
        return fallback_creator.to_string();
    };

    for resource_name in &resource_names {
        match acp.get_doc_owner(&policy.id, resource_name, doc_id).await {
            Ok(Some(owner)) => {
                if resource_name != &policy.resource_name {
                    tracing::info!(
                        collection = %collection.name(),
                        collection_id = %collection.collection_id(),
                        resource_name = %resource_name,
                        doc_id = %doc_id,
                        "Resolved ACP owner for replicator push using fallback resource name"
                    );
                }
                return owner.to_string();
            }
            Ok(None) => {}
            Err(error) => {
                tracing::warn!(
                    collection = %collection.name(),
                    collection_id = %collection.collection_id(),
                    resource_name = %resource_name,
                    doc_id = %doc_id,
                    error = %error,
                    "Failed to resolve ACP owner for replicator push"
                );
            }
        }
    }

    // No owner to carry: the document is unregistered (public under Local
    // DAC, and every replicated document is deliberately left unregistered by
    // acp_merge_handler.rs), or ACP could not say. Either way replay under the
    // same creator the live path uses (broadcast.rs:201) and Go uses on every
    // push (replicator.go:269,406,887); Go never consults ACP here at all.
    fallback_creator.to_string()
}
