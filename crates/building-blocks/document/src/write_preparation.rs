use cid::Cid;
use defra_core::encryption::EncryptionKey;
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};
use std::any::Any;
use std::sync::Arc;

/// Storage identity and encryption keys resolved once for a logical write.
/// This is transient input to a write, never part of the stored document.
#[derive(Clone)]
pub struct WritePreparation {
    pub collection_short_id: u32,
    pub doc_short_id: u64,
    pub encrypt_doc: bool,
    pub encrypted_fields: RapidHashSet<String>,
    keys: RapidHashMap<String, (Cid, EncryptionKey)>,
    key_scopes: RapidHashMap<String, Option<String>>,
    requires_kms_keys: bool,
    payload: Option<Arc<dyn Any + Send + Sync>>,
}

impl std::fmt::Debug for WritePreparation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WritePreparation")
            .field("collection_short_id", &self.collection_short_id)
            .field("doc_short_id", &self.doc_short_id)
            .field("encrypt_doc", &self.encrypt_doc)
            .field("encrypted_fields", &self.encrypted_fields)
            .field("key_count", &self.keys.len())
            .field("requires_kms_keys", &self.requires_kms_keys)
            .field("has_prepared_payload", &self.payload.is_some())
            .finish()
    }
}

impl WritePreparation {
    pub fn new(collection_short_id: u32, doc_short_id: u64) -> Self {
        Self {
            collection_short_id,
            doc_short_id,
            encrypt_doc: false,
            encrypted_fields: RapidHashSet::default(),
            keys: RapidHashMap::new(),
            key_scopes: RapidHashMap::new(),
            requires_kms_keys: false,
            payload: None,
        }
    }

    pub fn insert_key(&mut self, field: String, cid: Cid, key: [u8; 32]) {
        self.insert_scoped_key(field, cid, key, None);
    }

    pub fn insert_scoped_key(
        &mut self,
        field: String,
        cid: Cid,
        key: [u8; 32],
        scope_field: Option<String>,
    ) {
        self.encrypted_fields.insert(field.clone());
        self.key_scopes.insert(field.clone(), scope_field);
        self.keys
            .insert(field, (cid, EncryptionKey::new(key.to_vec())));
    }

    pub fn key_scope_matches(&self, field: &str, scope_field: Option<&str>) -> bool {
        self.key_scopes
            .get(field)
            .is_some_and(|scope| scope.as_deref() == scope_field)
    }

    pub fn requires_kms_keys(&self) -> bool {
        self.requires_kms_keys
    }

    pub fn set_requires_kms_keys(&mut self, required: bool) {
        self.requires_kms_keys = required;
    }

    pub fn key(&self, field: &str) -> Option<(Cid, &[u8; 32])> {
        self.keys.get(field).map(|(cid, key)| {
            (
                *cid,
                key.as_bytes()
                    .try_into()
                    .expect("prepared keys are 32 bytes"),
            )
        })
    }

    /// Attach a backend-specific immutable payload prepared for this logical
    /// write. The document crate keeps it opaque to avoid depending on DB.
    pub fn set_payload<T: Any + Send + Sync>(&mut self, payload: Arc<T>) {
        self.payload = Some(payload);
    }

    pub fn payload<T: Any + Send + Sync>(&self) -> Option<&T> {
        self.payload.as_ref()?.downcast_ref::<T>()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kms_key_scope_must_match_the_prepared_encryption_context() {
        let mut prepared = WritePreparation::new(1, 2);
        prepared.set_requires_kms_keys(true);
        prepared.insert_scoped_key(
            "secret".to_string(),
            Cid::default(),
            [7; 32],
            Some("secret".to_string()),
        );

        assert!(prepared.requires_kms_keys());
        assert!(prepared.key_scope_matches("secret", Some("secret")));
        assert!(!prepared.key_scope_matches("secret", None));
        assert!(!prepared.key_scope_matches("secret", Some("other")));
    }
}
