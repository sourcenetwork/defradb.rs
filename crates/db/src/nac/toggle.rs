//! Turning NAC off and back on, Go's `DisableNAC` and `ReEnableNAC`: the
//! status guards, the caller's permission and the status write run in one
//! write scope, so a grant, a revocation or another status change that
//! commits first fails the commit, and the decision runs again on what
//! committed.

use acp::nac::{NacStatus, NodePermission};
use acp::ZanzibarStore;
use identity::Did;

use super::{Error, NacManager, Result};

/// Who asks for a NAC operation: the identity acting, if any, and the node's
/// own identity, which Go's `checkNodeAccess` always lets through.
#[derive(Debug, Clone, Default)]
pub struct NacCaller {
    /// The identity acting in the request; `None` for an anonymous one.
    pub identity: Option<Did>,
    /// The node's own identity, when it has one.
    pub node: Option<Did>,
}

/// Which way a status change goes.
#[derive(Debug, Clone, Copy)]
enum Toggle {
    Disable,
    ReEnable,
}

impl<S> NacManager<S>
where
    S: ZanzibarStore + ?Sized + 'static,
{
    /// Turn NAC off for `caller`, keeping its relationships for a re-enable.
    /// Go's order: refused when NAC is not set up or already off, then
    /// checked against `NacDisable`.
    pub async fn disable_as(&self, caller: &NacCaller) -> Result<()> {
        self.toggle_as(caller, Toggle::Disable).await
    }

    /// Turn a disabled NAC back on for `caller`. Go's order: refused when
    /// NAC is not set up, checked against `NacReEnable` in the stored
    /// relationships even while NAC is off, then refused when NAC is on.
    pub async fn re_enable_as(&self, caller: &NacCaller) -> Result<()> {
        self.toggle_as(caller, Toggle::ReEnable).await
    }

    /// Decides on the state each write's own scope reads, through
    /// [`in_write`](NacManager::in_write).
    async fn toggle_as(&self, caller: &NacCaller, toggle: Toggle) -> Result<()> {
        let asked = caller.clone();
        let written = self
            .in_write(move |manager| {
                let caller = asked.clone();
                async move { manager.decide_toggle(&caller, toggle).await }
            })
            .await?;
        match written {
            Some(()) => Ok(()),
            None => self.decide_toggle(caller, toggle).await,
        }
    }

    /// The checks in Go's order, then the write, all on this manager's
    /// store.
    async fn decide_toggle(&self, caller: &NacCaller, toggle: Toggle) -> Result<()> {
        let status = self.nac.status().await;
        if status == NacStatus::NotConfigured {
            return Err(Error::NotConfigured);
        }
        let written = match toggle {
            Toggle::Disable => {
                if status == NacStatus::DisabledTemporarily {
                    return Err(Error::AlreadyDisabled);
                }
                self.require(caller, NodePermission::NacDisable).await?;
                self.nac.disable().await
            }
            Toggle::ReEnable => {
                self.require(caller, NodePermission::NacReEnable).await?;
                if status == NacStatus::Enabled {
                    return Err(Error::AlreadyEnabled);
                }
                self.nac.re_enable().await
            }
        };
        written.map_err(|error| Error::Acp(error.to_string()))
    }

    /// Go's `checkNodeAccess` for `permission`: the node's own identity
    /// passes; anyone else, or the wildcard for an anonymous caller, needs
    /// the permission while NAC is on, and re-enabling needs it in the stored
    /// relationships while NAC is off too.
    pub(super) async fn require(
        &self,
        caller: &NacCaller,
        permission: NodePermission,
    ) -> Result<()> {
        if caller.identity.is_some() && caller.identity == caller.node {
            return Ok(());
        }
        let identity = caller.identity.clone().unwrap_or_else(Did::wildcard);
        let allowed = match permission {
            NodePermission::NacReEnable => {
                self.nac
                    .check_permission_persisted(&identity, permission)
                    .await
            }
            _ => self.nac.check_permission(&identity, permission).await,
        }
        .map_err(|error| Error::Acp(error.to_string()))?;
        match allowed {
            true => Ok(()),
            false => Err(Error::NotAuthorized(permission)),
        }
    }
}
