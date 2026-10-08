use cid::Cid;
use defra_core::encryption::EncryptionKey;
use rapidhash::{HashMapExt, RapidHashMap, RapidHashSet};

/// Storage identity and encryption keys resolved once for a logical write.
/// This is transient input to a write, never part of the stored document.
#[derive(Debug, Clone)]
pub struct WritePreparation {
    pub collection_short_id: u32,
    pub doc_short_id: u64,
    pub encrypt_doc: bool,
    pub encrypted_fields: RapidHashSet<String>,
    keys: RapidHashMap<String, (Cid, EncryptionKey)>,
}

impl WritePreparation {
    pub fn new(collection_short_id: u32, doc_short_id: u64) -> Self {
        Self {
            collection_short_id,
            doc_short_id,
            encrypt_doc: false,
            encrypted_fields: RapidHashSet::default(),
            keys: RapidHashMap::new(),
        }
    }

    pub fn insert_key(&mut self, field: String, cid: Cid, key: [u8; 32]) {
        self.encrypted_fields.insert(field.clone());
        self.keys
            .insert(field, (cid, EncryptionKey::new(key.to_vec())));
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
}
