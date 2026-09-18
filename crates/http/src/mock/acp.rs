//! Mock ACP operations for testing ACP handlers.

use async_trait::async_trait;
use kovan::{Atom, AtomOption};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use crate::mock::update_vec;
use crate::router::{AcpLightClientStatus, AcpOperations, PolicyInfo};

/// Mock ACP operations for testing ACP handlers.
#[derive(Debug)]
pub struct MockAcpOperations {
    policies: Arc<Atom<Vec<PolicyInfo>>>,
    next_id: Arc<AtomicU64>,
    light_client_status: Arc<AtomOption<AcpLightClientStatus>>,
}

impl Clone for MockAcpOperations {
    fn clone(&self) -> Self {
        Self {
            policies: Arc::clone(&self.policies),
            next_id: Arc::clone(&self.next_id),
            light_client_status: Arc::clone(&self.light_client_status),
        }
    }
}

impl Default for MockAcpOperations {
    fn default() -> Self {
        Self::new()
    }
}

impl MockAcpOperations {
    /// Create a new mock ACP operations instance.
    pub fn new() -> Self {
        Self {
            policies: Arc::new(Atom::new(vec![])),
            next_id: Arc::new(AtomicU64::new(1)),
            light_client_status: Arc::new(AtomOption::none()),
        }
    }

    /// Create with a pre-existing policy.
    pub fn with_policy(self, id: &str, name: Option<&str>) -> Self {
        update_vec(&self.policies, |policies| {
            policies.push(PolicyInfo {
                id: id.to_string(),
                name: name.map(|s| s.to_string()),
                description: None,
                resources: None,
                actor: None,
                creation_time: Some("2024-01-01T00:00:00Z".to_string()),
            })
        });
        self
    }

    /// Create with a pre-existing ACP light client status.
    pub fn with_light_client_status(self, status: AcpLightClientStatus) -> Self {
        self.light_client_status.store_some(status);
        self
    }
}

#[async_trait]
impl AcpOperations for MockAcpOperations {
    async fn add_policy(&self, _policy: &str) -> Result<String, String> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let policy_id = format!("policy-{:04}", id);

        update_vec(&self.policies, |policies| {
            policies.push(PolicyInfo {
                id: policy_id.clone(),
                name: Some("Test Policy".to_string()),
                description: Some("A test policy".to_string()),
                resources: None,
                actor: None,
                creation_time: Some("2024-01-01T00:00:00Z".to_string()),
            })
        });

        Ok(policy_id)
    }

    async fn list_policies(&self) -> Result<Vec<PolicyInfo>, String> {
        Ok(self.policies.load_clone())
    }

    async fn get_policy(&self, id: &str) -> Result<Option<PolicyInfo>, String> {
        Ok(self
            .policies
            .peek(|policies| policies.iter().find(|p| p.id == id).cloned()))
    }

    async fn get_light_client_status(&self) -> Result<AcpLightClientStatus, String> {
        self.light_client_status
            .load()
            .map(|status| (*status).clone())
            .ok_or_else(|| "ACP light client status is not configured in this mock".to_string())
    }
}

/// Mock ACP operations that always fails with a configurable error.
#[derive(Debug, Clone)]
pub struct FailingMockAcpOperations {
    error: String,
}

impl FailingMockAcpOperations {
    /// Create a new failing mock with the given error message.
    pub fn new(error: impl Into<String>) -> Self {
        Self {
            error: error.into(),
        }
    }
}

#[async_trait]
impl AcpOperations for FailingMockAcpOperations {
    async fn add_policy(&self, _policy: &str) -> Result<String, String> {
        Err(self.error.clone())
    }

    async fn list_policies(&self) -> Result<Vec<PolicyInfo>, String> {
        Err(self.error.clone())
    }

    async fn get_policy(&self, _id: &str) -> Result<Option<PolicyInfo>, String> {
        Err(self.error.clone())
    }

    async fn get_light_client_status(&self) -> Result<AcpLightClientStatus, String> {
        Err(self.error.clone())
    }
}
