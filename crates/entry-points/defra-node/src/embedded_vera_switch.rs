//! Switching this node's embedded Vera device from the node, which then
//! sends what it owes the document ACP provider by itself.
//!
//! A backup takes no submission, so a document created on it owes its
//! registration and a relationship change staged on it waits, and so does a
//! submission in flight when a switch stopped the Vera node. Once the device
//! is the active one, these calls send all of it, so a host that switches
//! devices through them makes no further call.

use vera_embedded::{SoloKeys, TermCertificate};

use crate::{Error, Node, Result};

impl Node {
    /// Take the deployment over with `keys` in the term `certificate`
    /// starts ([`vera_embedded::HostedNode::take_over`]), then send every
    /// registration and relationship change the node owes, whoever's they
    /// are, each with the authority recorded with it. What the provider
    /// does not take stays owed, as [`Node::owed_acp`] reports.
    ///
    /// A switch that failed with the Vera node stopped is completed by
    /// [`Node::complete_embedded_vera_switch`].
    pub async fn take_over_embedded_vera(
        &self,
        keys: SoloKeys,
        certificate: TermCertificate,
    ) -> Result<()> {
        self.hosted_vera()?.take_over(keys, certificate).await?;
        self.core().send_all_owed_acp().await
    }

    /// Start the Vera node again in the role the last switch was heading to
    /// ([`vera_embedded::HostedNode::complete_switch`]), and send what the
    /// node owes once it is the active device.
    pub async fn complete_embedded_vera_switch(&self) -> Result<()> {
        let hosted = self.hosted_vera()?;
        hosted.complete_switch().await?;
        if hosted.is_active().await {
            self.core().send_all_owed_acp().await?;
        }
        Ok(())
    }

    fn hosted_vera(&self) -> Result<&vera_embedded::HostedNode> {
        self.embedded_vera()
            .ok_or(Error::EmbeddedVera(vera_embedded::HostError::NotRunning))
    }
}
