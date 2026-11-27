//! The server's record of one client signing session: which waves it asked
//! for, which blocks the client has signed, and whether it refused an answer.
//! One session task owns it, so it needs no synchronization.

use std::sync::Arc;

use rapidhash::{HashSetExt, RapidHashSet};
use serde::Deserialize;

/// Why the server refused a signing wave or an answer to one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SigningWaveError {
    /// The answer names a wave the client already answered.
    #[error("signing wave {wave} was already signed")]
    Resigned { wave: u64 },
    /// The answer names a wave the server never asked for.
    #[error("signing wave {wave} is outside the approved graph")]
    UnknownWave { wave: u64 },
    /// A signature does not verify over the block the server asked it for.
    #[error(
        "signature {index} of signing wave {wave} does not sign a block of the approved graph"
    )]
    OutsideGraph { wave: u64, index: usize },
    /// The answer has a different number of signatures than the wave has blocks.
    #[error("signing wave {wave} has {expected} blocks but the answer has {actual} signatures")]
    Incomplete {
        wave: u64,
        expected: usize,
        actual: usize,
    },
    /// The server asked again for a block the client already signed.
    #[error("block {index} of signing wave {wave} was already signed in this session")]
    BlockResigned { wave: u64, index: usize },
    /// The answer is not `{"wave": <n>, "signatures": [<hex>, …]}`.
    #[error("signing answer is malformed: {0}")]
    Malformed(String),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Answer {
    wave: u64,
    signatures: Vec<String>,
}

/// The waves of one session. A wave is numbered from 1 when the server opens
/// it; the client's answer names it. A refusal is final: every later wave
/// and answer of the session gets the same error.
pub(crate) struct SigningWaves {
    public: Arc<dyn crypto::PublicKey>,
    issued: u64,
    open: Option<Vec<Arc<[u8]>>>,
    signed: RapidHashSet<Arc<[u8]>>,
    refused: Option<SigningWaveError>,
}

impl SigningWaves {
    pub(crate) fn new(public: Arc<dyn crypto::PublicKey>) -> Self {
        Self {
            public,
            issued: 0,
            open: None,
            signed: RapidHashSet::new(),
            refused: None,
        }
    }

    /// Open the next wave and return its number. A block the client already
    /// signed in this session, or twice in this wave, is never asked again.
    pub(crate) fn open(&mut self, payloads: &[Arc<[u8]>]) -> Result<u64, SigningWaveError> {
        if let Some(refused) = &self.refused {
            return Err(refused.clone());
        }
        let wave = self.issued + 1;
        let mut requested = RapidHashSet::with_capacity(payloads.len());
        for (index, payload) in payloads.iter().enumerate() {
            if self.signed.contains(payload) || !requested.insert(payload) {
                return Err(self.refuse(SigningWaveError::BlockResigned { wave, index }));
            }
        }
        self.issued = wave;
        self.open = Some(payloads.to_vec());
        Ok(wave)
    }

    /// Accept the client's answer to the open wave and return its signatures,
    /// each verified over the block it was asked for, in the wave's order.
    pub(crate) fn answer(
        &mut self,
        answer: serde_json::Value,
    ) -> Result<Vec<Vec<u8>>, SigningWaveError> {
        let checked = self.check(answer);
        match checked {
            Ok(signatures) => Ok(signatures),
            Err(error) => {
                self.open = None;
                Err(self.refuse(error))
            }
        }
    }

    fn refuse(&mut self, error: SigningWaveError) -> SigningWaveError {
        self.refused.get_or_insert(error).clone()
    }

    fn check(&mut self, answer: serde_json::Value) -> Result<Vec<Vec<u8>>, SigningWaveError> {
        if let Some(refused) = &self.refused {
            return Err(refused.clone());
        }
        let answer: Answer = serde_json::from_value(answer)
            .map_err(|error| SigningWaveError::Malformed(error.to_string()))?;
        let wave = answer.wave;
        if wave == 0 || wave > self.issued {
            return Err(SigningWaveError::UnknownWave { wave });
        }
        if wave < self.issued || self.open.is_none() {
            return Err(SigningWaveError::Resigned { wave });
        }
        let payloads = self.open.take().unwrap_or_default();
        if answer.signatures.len() != payloads.len() {
            return Err(SigningWaveError::Incomplete {
                wave,
                expected: payloads.len(),
                actual: answer.signatures.len(),
            });
        }
        let signatures = answer
            .signatures
            .iter()
            .zip(&payloads)
            .enumerate()
            .map(|(index, (signature, payload))| {
                hex::decode(signature)
                    .ok()
                    .filter(|signature| matches!(self.public.verify(payload, signature), Ok(true)))
                    .ok_or(SigningWaveError::OutsideGraph { wave, index })
            })
            .collect::<Result<Vec<_>, _>>()?;
        self.signed.extend(payloads);
        Ok(signatures)
    }
}

#[cfg(test)]
#[path = "../tests/unit/client_signing_waves.rs"]
mod tests;
