use kovan_map::HopscotchMap;
use rapidhash::fast::RandomState;

/// In-memory cache of DAC policy documents, keyed by policy ID.
pub struct PolicyStore {
    policies: HopscotchMap<String, String, RandomState>,
}

impl Default for PolicyStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PolicyStore {
    /// Create a new empty policy store.
    pub fn new() -> Self {
        Self {
            policies: HopscotchMap::with_hasher(RandomState::default()),
        }
    }

    /// Store a policy with a known ID (used for Vera-created policies).
    pub fn store_policy(&self, id: &str, policy: &str) {
        self.policies.insert(id.to_string(), policy.to_string());
    }

    /// Remove a policy from the cache.
    pub fn remove_policy(&self, id: &str) {
        self.policies.remove(id);
    }

    /// Get a policy by ID.
    pub fn get_policy(&self, id: &str) -> Option<String> {
        self.policies.get(id)
    }

    /// List all policy IDs.
    pub fn list_policies(&self) -> Vec<String> {
        self.policies.keys().collect()
    }
}
