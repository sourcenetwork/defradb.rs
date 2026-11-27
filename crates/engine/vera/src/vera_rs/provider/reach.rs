//! Telling a read the chain refused from one that never reached it.
//!
//! The light client's errors carry no type that says which is which. A read
//! counts as not having reached the chain when its error holds a transport
//! failure that is no JSON-RPC answer, or an I/O error, or when the light
//! client has no fresh verified state left, which is how a chain that has
//! stopped answering shows after the freshness window. A read that was late
//! (overtaken, stale or busy after the waits of `verified`) did not get its
//! answer in time either, and counts the same. Any other failure is the
//! chain's answer or a proof that did not verify, and stays an error.
//!
//! The light client's HTTP transport reports its HTTP and JSON errors as
//! themselves, not as transport failures, and their types are not ones this
//! crate can name. So a request that timed out, or a body that is not JSON,
//! is recognised only through an I/O error in its chain, if it has one, or
//! once the verified state has gone stale.

use std::error::Error;

use acp_light_client::{AcpLightClient, TransportError};

use crate::provider::ProviderError;

fn transport_failure(error: &(dyn Error + 'static)) -> bool {
    let mut link = Some(error);
    while let Some(error) = link {
        if error.is::<std::io::Error>() {
            return true;
        }
        if let Some(transport) = error.downcast_ref::<TransportError>() {
            return !matches!(transport, TransportError::Rpc { .. });
        }
        link = error.source();
    }
    false
}

/// The error of a verified read of `what`: [`ProviderError::Unavailable`]
/// when the chain was not reached, [`ProviderError::Query`] otherwise.
pub(super) fn read_error<E>(light_client: &AcpLightClient, what: &str, error: E) -> ProviderError
where
    E: AsRef<dyn Error + Send + Sync + 'static> + std::fmt::Display,
{
    let message = format!("{what}: {error}");
    if transport_failure(error.as_ref())
        || super::verified::late(&error)
        || light_client.header_chain().fresh_state().is_err()
    {
        ProviderError::Unavailable(message)
    } else {
        ProviderError::Query(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Debug, thiserror::Error)]
    #[error("wrapped")]
    struct Wrapped(#[source] Box<dyn Error + Send + Sync>);

    #[test]
    fn a_transport_failure_is_told_from_an_answer() {
        let refused = std::io::Error::from(std::io::ErrorKind::ConnectionRefused);
        assert!(transport_failure(&Wrapped(Box::new(refused))));
        assert!(transport_failure(&TransportError::other(
            "connection closed"
        )));
        assert!(transport_failure(&Wrapped(Box::new(
            TransportError::ResponseTooLarge { method: "vera_x" }
        ))));

        let answered = TransportError::Rpc {
            method: "vera_x",
            code: -32000,
            message: "no such policy".into(),
        };
        assert!(!transport_failure(&Wrapped(Box::new(answered))));
        assert!(!transport_failure(&Wrapped(Box::new(std::fmt::Error))));
    }
}
