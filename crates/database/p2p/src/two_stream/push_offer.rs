//! A PushLog signed once for Go's protocol and once for Rust's.

use libp2p::identity::Keypair;
use uuid::Uuid;

use crate::error::Result;
use crate::message::PushLogRequest;
use crate::signing::sign_message;

/// A signed PushLog in the form each receiver can verify.
///
/// Go checks a PushLog's signature over the fields it knows, so a request
/// carrying a field Go does not know fails that check and Go never answers.
/// `go_compatible` carries Go's fields only and goes out on Go's request
/// protocol; `extended` adds the explicit replay capability and goes out only
/// once the receiver negotiates the Rust-only retry protocol. Both share one
/// message ID, so the reply matches whichever was sent.
#[derive(Debug, Clone)]
pub struct PushLogOffer {
    go_compatible: PushLogRequest,
    extended: Option<PushLogRequest>,
}

impl PushLogOffer {
    /// Sign `request` as Go's protocol takes it and, when it carries Rust-only
    /// fields, as it stands.
    pub fn sign(keypair: &Keypair, mut request: PushLogRequest) -> Result<Self> {
        if request.message_id.is_empty() {
            request.message_id = Uuid::new_v4().to_string();
        }
        let mut go_compatible = request.clone();
        go_compatible.explicit_replay_capability = None;
        go_compatible.supports_same_stream_reply = false;
        go_compatible.supports_retry_after = false;
        sign_message(keypair, &mut go_compatible)?;
        let extended = if request.explicit_replay_capability.is_some() {
            sign_message(keypair, &mut request)?;
            Some(request)
        } else {
            None
        };
        Ok(Self {
            go_compatible,
            extended,
        })
    }

    /// The request to write on a stream that negotiated Rust's retry protocol
    /// (`rust_protocol`) or fell back to Go's.
    pub fn for_protocol(self, rust_protocol: bool) -> PushLogRequest {
        match self.extended {
            Some(extended) if rust_protocol => extended,
            _ => self.go_compatible,
        }
    }

    /// The message ID every form of the request carries.
    pub fn message_id(&self) -> &str {
        &self.go_compatible.message_id
    }
}
