//! Creating a policy on vera.rs, under the ID its definition, the actor and
//! a nonce derive. A create sent again with its nonce is refused as already
//! existing; the policy there is then the caller's if it is the definition,
//! actor and nonce asked for, and its ID is returned. Modeled by
//! proofs/tla/VeraPolicyCreate.tla.

use alloy_sol_types::{SolCall, SolEvent};
use vera_client::random_policy_nonce;
use vera_domain::{RefusalKind, RefusingModule};
use vera_modules::acp::types::PolicyRecord;

use super::submission::{rejected, ConfirmedSubmission};
use super::{bearer, did_spelling, verified, ProviderError, VeraRsProvider, ACP_ADDRESS};
use crate::vera_rs::abi::IAcp;
use crate::vera_rs::predict_policy_id;

impl VeraRsProvider {
    /// The ID the provider's actor gets for the YAML `policy` created under
    /// `nonce`.
    ///
    /// # Errors
    /// The definition does not compile.
    pub fn predict_policy_id(
        &self,
        policy: &str,
        nonce: acp::PolicyNonce,
    ) -> Result<String, ProviderError> {
        predict_policy_id(policy, nonce, &self.actor_did)
    }

    /// Create `policy_yaml` as the provider's actor under `nonce`, or under a
    /// nonce drawn once for this call, so a send again inside it never
    /// makes a second policy.
    pub(super) async fn create_policy_with(
        &self,
        policy_yaml: &str,
        nonce: Option<acp::PolicyNonce>,
    ) -> Result<String, ProviderError> {
        let nonce = nonce.unwrap_or_else(|| acp::PolicyNonce(random_policy_nonce().0));
        let creator = did_spelling::wire(&self.actor_did);
        let token = bearer::create_scoped_bearer_token(
            self.signer.as_ref(),
            &creator,
            &self.worker_did,
            self.deployment,
            300,
            vera_client::DelegationScope::CreatePolicy,
        )
        .await
        .map_err(|e| ProviderError::Config(e.to_string()))?;
        let call = IAcp::bearerCreatePolicyCall {
            bearerToken: token,
            policy: policy_yaml.as_bytes().to_vec().into(),
            marshalType: 1,
            nonce: nonce.0.into(),
        };
        let confirmed = self.confirm_tx(call.abi_encode().into()).await?;
        if !confirmed.receipt.success() {
            return self.adopt_existing(policy_yaml, nonce, &confirmed).await;
        }
        // vera.rs compiled the definition to create it, so it compiles here.
        let id = self
            .predict_policy_id(policy_yaml, nonce)
            .map_err(|e| ProviderError::Query(format!("created policy: {e}")))?;
        let mut events = confirmed.receipt.logs().iter().filter(|log| {
            log.address == ACP_ADDRESS
                && log.topics().first() == Some(&IAcp::DelegatedPolicyCreated::SIGNATURE_HASH)
        });
        let log = events
            .next()
            .ok_or_else(|| ProviderError::Query("creation event missing".into()))?;
        let event = IAcp::DelegatedPolicyCreated::decode_raw_log_validate(
            log.topics().iter().copied(),
            &log.data.data,
        )
        .map_err(|e| ProviderError::Query(format!("creation event: {e}")))?;
        if events.next().is_some()
            || !did_spelling::same_actor(&event.creator, &self.actor_did)
            || hex::encode(event.policyId) != id
        {
            return Err(ProviderError::Query(
                "creation event does not match the actor".into(),
            ));
        }
        let record = self.read_policy_record(&id).await?;
        if record.raw_policy != policy_yaml
            || record.nonce.0 != nonce.0
            || !did_spelling::same_actor(&record.metadata.owner_did, &self.actor_did)
            || record.metadata.tx_signer != self.worker_did
            || record.metadata.tx_hash != confirmed.receipt.tx_hash.as_slice()
            || record.metadata.creation_ts.block_height != confirmed.revision
        {
            return Err(ProviderError::Query(
                "created policy does not match the signed request".into(),
            ));
        }
        Ok(id)
    }

    /// A create refused as already existing: the policy under the ID the
    /// definition, actor and nonce derive, if it is the one asked for. An
    /// owner's edit since keeps the ID but not the definition, and such a
    /// policy is not handed back as the caller's.
    async fn adopt_existing(
        &self,
        policy_yaml: &str,
        nonce: acp::PolicyNonce,
        confirmed: &ConfirmedSubmission,
    ) -> Result<String, ProviderError> {
        let exists = confirmed.receipt.refusal.as_ref().is_some_and(|refusal| {
            refusal.module == RefusingModule::Acp && refusal.kind == RefusalKind::AlreadyExists
        });
        if !exists {
            return Err(rejected(confirmed));
        }
        let id = self.predict_policy_id(policy_yaml, nonce)?;
        let record = self.read_policy_record(&id).await?;
        if record.raw_policy != policy_yaml
            || record.nonce.0 != nonce.0
            || !did_spelling::same_actor(&record.metadata.owner_did, &self.actor_did)
        {
            return Err(ProviderError::Transaction(format!(
                "policy {id} exists, and its definition is no longer the one asked for"
            )));
        }
        Ok(id)
    }

    /// The verified record of policy `id`, which must exist.
    async fn read_policy_record(&self, id: &str) -> Result<PolicyRecord, ProviderError> {
        let record = self
            .admitted_read(async {
                verified(self.light_client.header_chain(), || {
                    self.light_client.read_policy(id)
                })
                .await
                .map_err(|e| ProviderError::Query(format!("policy proof: {e}")))
            })
            .await?;
        let record: PolicyRecord = serde_json::from_slice(
            record
                .value
                .as_deref()
                .ok_or_else(|| ProviderError::Query(format!("policy {id} absent")))?,
        )
        .map_err(|e| ProviderError::Query(format!("policy record: {e}")))?;
        if record.policy.id.as_str() != id || record.derived_id() != id {
            return Err(ProviderError::Query(format!(
                "policy {id} is stored under another ID"
            )));
        }
        Ok(record)
    }
}
