#[cfg(feature = "vera")]
use crate::Error;
use crate::Result;
use std::sync::Arc;

pub(crate) enum Persistence {
    Persistent,
    Memory,
}

pub(crate) struct DocumentAcpSetup {
    pub document_acp: Arc<dyn acp::DocumentACP>,
    pub local_zanzibar_store: Option<Arc<dyn acp::ZanzibarStore>>,
    #[cfg(feature = "vera")]
    pub vera_acp: Option<Arc<vera::VeraDocumentACP>>,
}

pub(crate) async fn create_document_acp<S>(
    store: Arc<S>,
    _persistence: Persistence,
    config: &crate::DocumentAcpConfig,
    #[cfg_attr(not(feature = "vera"), allow(unused_variables))] network: Option<&edge::Edge>,
) -> Result<DocumentAcpSetup>
where
    S: storage::corekv::Store + 'static,
{
    match config {
        #[cfg(feature = "vera")]
        crate::DocumentAcpConfig::Vera(vera_config) => {
            let tuning = vera::AcpTuning::default();
            let provider = Arc::new(
                vera::CosmosProvider::new(
                    vera_config.grpc_address.clone(),
                    vera_config.comet_rpc_address.clone(),
                    &vera_config.signer_key,
                    &vera_config.chain_id,
                    &tuning,
                )
                .map_err(Error::VeraProvider)?,
            );
            let sh_acp = Arc::new(vera::VeraDocumentACP::tuned(provider, &tuning, network));
            Ok(DocumentAcpSetup {
                document_acp: gate_external_acp(sh_acp.clone(), store, network)?,
                local_zanzibar_store: None,
                vera_acp: Some(sh_acp),
            })
        }
        #[cfg(feature = "vera")]
        crate::DocumentAcpConfig::VeraWithLcd {
            config: vera_config,
            lcd_address,
        } => {
            let tuning = vera::AcpTuning::default();
            let provider = Arc::new(
                vera::CosmosProvider::new_with_grpc(
                    lcd_address.clone(),
                    vera_config.grpc_address.clone(),
                    vera_config.comet_rpc_address.clone(),
                    &vera_config.signer_key,
                    &vera_config.chain_id,
                    &tuning,
                )
                .map_err(Error::VeraProvider)?,
            );
            let vera_acp = Arc::new(vera::VeraDocumentACP::tuned(provider, &tuning, network));
            Ok(DocumentAcpSetup {
                document_acp: gate_external_acp(vera_acp.clone(), store, network)?,
                local_zanzibar_store: None,
                vera_acp: Some(vera_acp),
            })
        }
        crate::DocumentAcpConfig::Local => {
            // Memory and disk profiles both share the engine with document rows.
            let zanzibar_store = Arc::new(acp::PersistentZanzibarStore::from_store(store));
            let document_acp = Arc::new(acp::ZanzibarDocumentACP::new(zanzibar_store.clone()));
            Ok(local_document_acp_setup(document_acp, zanzibar_store))
        }
    }
}

/// Gate a Vera provider with the registrations creates still owe it, and
/// settle in the background any a previous run left owed (a crash, or a
/// provider call that failed). Until settled, such a document is readable only
/// by its creator.
#[cfg(feature = "vera")]
fn gate_external_acp<S>(
    provider: Arc<dyn acp::DocumentACP>,
    store: Arc<S>,
    network: Option<&edge::Edge>,
) -> Result<Arc<dyn acp::DocumentACP>>
where
    S: storage::corekv::Store + 'static,
{
    let gate = Arc::new(acp::ExternalDocumentAcp::new(provider, store));
    let redrive = Arc::clone(&gate);
    // The redrive's provider calls are network work, on the node's runtime.
    let network = match network {
        Some(edge) => edge,
        None => edge::on_demand()?,
    };
    drop(network.spawn(async move {
        match redrive.redrive_owed().await {
            Ok(report) if report.settled > 0 => tracing::info!(
                settled = report.settled,
                failed = report.failed,
                "Settled ACP registrations owed from a previous run"
            ),
            Ok(_) => {}
            Err(error) => tracing::warn!(
                %error,
                "Could not read owed ACP registrations; their documents stay readable only by their creators"
            ),
        }
    }));
    Ok(gate)
}

fn local_document_acp_setup(
    document_acp: Arc<dyn acp::DocumentACP>,
    zanzibar_store: Arc<dyn acp::ZanzibarStore>,
) -> DocumentAcpSetup {
    DocumentAcpSetup {
        document_acp,
        local_zanzibar_store: Some(zanzibar_store),
        #[cfg(feature = "vera")]
        vera_acp: None,
    }
}

#[cfg(test)]
mod tests {
    use super::{create_document_acp, Persistence};
    use crate::DocumentAcpConfig;
    use acp::{DocumentPermission, Identity, StorePolicyOptions};
    use identity::Did;
    use std::sync::Arc;

    fn did(value: &str) -> Did {
        Did::new(value).unwrap()
    }

    #[test]
    fn local_document_acp_enforces_stored_custom_policy() {
        workers::block_on(async {
            let store = Arc::new(storage::RegolithStore::in_memory().unwrap());
            let acp_setup =
                create_document_acp(store, Persistence::Memory, &DocumentAcpConfig::Local, None)
                    .await
                    .unwrap();

            #[cfg(feature = "vera")]
            assert!(acp_setup.vera_acp.is_none());
            let document_acp = acp_setup.document_acp;
            let local_store = acp_setup
                .local_zanzibar_store
                .expect("local ACP should expose a Zanzibar store");

            let parsed = acp::policy_yaml::parse_policy_yaml(
                r#"
name: Viewer Policy
resources:
  - name: users
    relations:
      - name: viewer
      - name: editor
      - name: remover
    permissions:
      - name: read
        expr: viewer
      - name: update
        expr: editor
      - name: delete
        expr: remover
"#,
            )
            .unwrap();
            let policy = acp::policy_yaml::build_policy(&parsed, 1).unwrap();
            let options = StorePolicyOptions::new()
                .with_validation()
                .with_dpi_enforcement();
            local_store
                .store_policy_with_options(&policy, &options)
                .await
                .unwrap();

            let owner = did("did:key:z6MkhaXgBZDvotDkL5257faiztiGiC2QtKLGpbnnEGta2doK");
            let viewer = did("did:key:z6MkfXG2FkNy3u7Eg3jm8e2YQpGz7Z1JqWgHDAP1hLk9r2bR");

            document_acp
                .register_doc_object(&owner, &policy.id, "users", "doc1")
                .await
                .unwrap();
            document_acp
                .add_actor_relationship(&owner, &viewer, &policy.id, "users", "doc1", "viewer", &[])
                .await
                .unwrap();

            assert!(document_acp
                .check_doc_access(
                    &Identity::Authenticated(viewer.clone()),
                    DocumentPermission::Read,
                    &policy.id,
                    "users",
                    "doc1",
                )
                .await
                .unwrap());
            assert!(!document_acp
                .check_doc_access(
                    &Identity::Authenticated(viewer),
                    DocumentPermission::Update,
                    &policy.id,
                    "users",
                    "doc1",
                )
                .await
                .unwrap());
        })
        .unwrap()
    }
}
