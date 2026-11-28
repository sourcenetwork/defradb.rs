//! The DAC bypass of the request a runner serves: decided once, in the
//! request's own transaction, and honoured by every document check the
//! runner makes (Go's `canDACBypass`, which runs before every one).

use acp::Identity;
use identity::Did;

use crate::error::Result;
use crate::txn::TransactionRegistry;

use super::{DocFetcher, QueryRunner};

impl<F, R> QueryRunner<F, R>
where
    F: DocFetcher + 'static,
    R: TransactionRegistry,
{
    /// Whether this request's DAC bypass (`RequestContext::dac_bypass`)
    /// decides every document check for `identity`.
    pub(crate) fn dac_bypassed(&self, identity: &Identity) -> bool {
        identity.is_authenticated() && self.context().dac_bypass()
    }

    /// This runner with the bypass `fetcher` decides for `caller`, read in
    /// the fetcher's transaction: `None` when the request already carries
    /// a bypass, its caller denied it one, it has no caller, or node access
    /// control does not grant `caller` `bypass-dac`.
    pub(crate) async fn with_decided_dac_bypass(
        &self,
        fetcher: &dyn DocFetcher,
        caller: Option<&Did>,
    ) -> Result<Option<Self>> {
        let context = self.context();
        let Some(caller) = caller else {
            return Ok(None);
        };
        if context.dac_bypass() || context.dac_bypass_denied() {
            return Ok(None);
        }
        if !fetcher.dac_bypass(caller).await? {
            return Ok(None);
        }
        Ok(self.with_dac_bypass_as_decided(true))
    }

    /// This runner with the bypass an attempt decided in its own
    /// transaction: `None` when that is no bypass or the request already
    /// carries one. A request its caller denied the bypass never gets one.
    pub(crate) fn with_dac_bypass_as_decided(&self, bypassed: bool) -> Option<Self> {
        let context = self.context();
        if !bypassed || context.dac_bypass() || context.dac_bypass_denied() {
            return None;
        }
        Some(self.for_request(context.into_owned().with_dac_bypass(true)))
    }
}
