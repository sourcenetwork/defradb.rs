use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use alloy_primitives::{Address, Bytes};
use hub_client::BlsSigner;
use hub_domain::{ConsensusPublicKey, ExecutionReceipt, NativeTx, ReceiptResponse};
use serde::{Deserialize, Serialize};

use crate::provider::ProviderError;

const MAX_WIRE_BYTES: usize = 65_536;
const MAX_JOURNAL_BYTES: usize = 2 * MAX_WIRE_BYTES + 4096;

#[cfg(test)]
mod tests;

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u8,
    deployment: u64,
    key_name: String,
    next_sequence: u64,
    pending: Option<Bytes>,
}

/// Durable signing state for one worker, with an exclusive directory lock.
///
/// Private keys live in the supplied keyring. The journal holds key references
/// and signed requests. One unresolved request is retained until verified receipt
/// evidence allows the next sequence. Open and mutation methods perform blocking IO.
pub struct NativeWorker {
    _lock: File,
    path: PathBuf,
    signer: BlsSigner,
    journal: Journal,
}

fn failure(cause: impl std::fmt::Display) -> ProviderError {
    ProviderError::Unavailable(format!("native worker state: {cause}"))
}

impl NativeWorker {
    /// Open an existing worker or create an independent key on first use.
    /// A missing key for an existing journal is an error, never a request to rotate.
    pub fn open(
        directory: &Path,
        keyring: &dyn keyring::Keyring,
        deployment: u64,
    ) -> Result<Self, ProviderError> {
        fs::create_dir_all(directory).map_err(failure)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            fs::set_permissions(directory, fs::Permissions::from_mode(0o700)).map_err(failure)?;
        }
        let mut options = OpenOptions::new();
        options.read(true).write(true).create(true).truncate(false);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let lock = options.open(directory.join("lock")).map_err(failure)?;
        lock.try_lock().map_err(failure)?;
        let path = directory.join("state.json");
        let (journal, signer) = match File::open(&path) {
            Ok(file) => {
                let mut bytes = Vec::new();
                file.take(MAX_JOURNAL_BYTES as u64 + 1)
                    .read_to_end(&mut bytes)
                    .map_err(failure)?;
                if bytes.len() > MAX_JOURNAL_BYTES {
                    return Err(failure("journal exceeds its byte limit"));
                }
                let journal: Journal = serde_json::from_slice(&bytes).map_err(failure)?;
                if journal.version != 1 || journal.deployment != deployment {
                    return Err(failure("journal version or deployment mismatch"));
                }
                let suffix = journal
                    .key_name
                    .strip_prefix("vera-worker-")
                    .ok_or_else(|| failure("invalid worker key name"))?;
                if suffix.len() != 32 || !suffix.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return Err(failure("invalid worker key name"));
                }
                let key = keyring.get(&journal.key_name).map_err(failure)?;
                let signer = BlsSigner::from_secret_bytes(&key, deployment, journal.next_sequence)
                    .map_err(failure)?;
                if let Some(wire) = &journal.pending {
                    if wire.len() > MAX_WIRE_BYTES {
                        return Err(failure("pending request exceeds its byte limit"));
                    }
                    let tx = NativeTx::decode_wire(wire).map_err(failure)?;
                    let expected = signer
                        .sign_native_tx_with_sequence(tx.target, tx.calldata, journal.next_sequence)
                        .map_err(failure)?;
                    if wire.as_ref() != expected {
                        return Err(failure("pending request does not match the worker state"));
                    }
                }
                (journal, signer)
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                let signer = BlsSigner::random(deployment).map_err(failure)?;
                let journal = Journal {
                    version: 1,
                    deployment,
                    key_name: format!("vera-worker-{}", hex::encode(rand::random::<[u8; 16]>())),
                    next_sequence: 0,
                    pending: None,
                };
                keyring
                    .set(
                        &journal.key_name,
                        &signer.secret_key_bytes().map_err(failure)?,
                    )
                    .map_err(failure)?;
                Self::persist(&path, &journal)?;
                (journal, signer)
            }
            Err(error) => return Err(failure(error)),
        };
        Ok(Self {
            _lock: lock,
            path,
            signer,
            journal,
        })
    }

    /// Stable signing identity, independent of the authorizing actor.
    pub fn did(&self) -> &str {
        self.signer.did()
    }

    /// Sequence reserved for the pending or next request.
    pub fn next_sequence(&self) -> u64 {
        self.journal.next_sequence
    }

    /// Exact signed bytes to resubmit or resolve after an interrupted request.
    pub fn pending(&self) -> Option<&[u8]> {
        self.journal.pending.as_ref().map(|wire| wire.as_ref())
    }

    /// Persist signed bytes before the caller can submit them.
    /// An identical retry returns the stored bytes; another request must wait.
    pub fn prepare(&mut self, target: Address, calldata: Bytes) -> Result<&[u8], ProviderError> {
        if let Some(wire) = &self.journal.pending {
            let tx = NativeTx::decode_wire(wire).map_err(failure)?;
            if tx.target != target || tx.calldata != calldata {
                return Err(failure(
                    "a different request is awaiting confirmed execution",
                ));
            }
        } else {
            if calldata.len() > MAX_WIRE_BYTES {
                return Err(failure("request exceeds its byte limit"));
            }
            let wire = self
                .signer
                .sign_native_tx_with_sequence(target, calldata, self.journal.next_sequence)
                .map_err(failure)?;
            if wire.len() > MAX_WIRE_BYTES {
                return Err(failure("signed request exceeds its byte limit"));
            }
            let mut next = self.journal.clone();
            next.pending = Some(wire.into());
            Self::persist(&self.path, &next)?;
            self.journal = next;
        }
        Ok(self.journal.pending.as_deref().expect("prepared request"))
    }

    /// Advance only after verifying execution for the exact pending submission.
    /// Both successful and rejected executions consume their reserved sequence.
    pub fn acknowledge(
        &mut self,
        response: &ReceiptResponse,
        trusted: &ConsensusPublicKey,
    ) -> Result<ExecutionReceipt, ProviderError> {
        let wire = self
            .pending()
            .ok_or_else(|| failure("no pending request"))?;
        let hash = NativeTx::decode_wire(wire).map_err(failure)?.tx_id().0;
        let receipt = response.verify(hash, trusted).map_err(failure)?.clone();
        let mut next = self.journal.clone();
        next.next_sequence = next
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| failure("sequence exhausted"))?;
        next.pending = None;
        Self::persist(&self.path, &next)?;
        self.journal = next;
        Ok(receipt)
    }

    fn persist(path: &Path, journal: &Journal) -> Result<(), ProviderError> {
        let directory = path
            .parent()
            .ok_or_else(|| failure("missing journal directory"))?;
        let bytes = serde_json::to_vec(journal).map_err(failure)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(failure("journal exceeds its byte limit"));
        }
        let mut file = tempfile::NamedTempFile::new_in(directory).map_err(failure)?;
        file.write_all(&bytes).map_err(failure)?;
        file.as_file().sync_all().map_err(failure)?;
        file.persist(path).map_err(|e| failure(e.error))?;
        #[cfg(unix)]
        File::open(directory)
            .and_then(|dir| dir.sync_all())
            .map_err(failure)?;
        Ok(())
    }
}
