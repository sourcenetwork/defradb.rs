//! The provider built on injected transports, next to the same provider built
//! from a URL, for what a node that serves nothing lets a test reach.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use acp_light_client::{AcpTransport, HeaderStream, TransportError};
use futures::future::BoxFuture;
use futures::FutureExt as _;
use serde_json::Value;

use super::{NativeWorker, ProviderError, VeraRsProvider};
use crate::tuning::AcpTuning;
use crate::vera_rs::client::tests::{Recorded, TRUSTED_KEY};

/// Nothing listens here, so the URL path meets a node that serves nothing.
const UNREACHABLE: &str = "http://127.0.0.1:1";

/// A light client transport to a node that serves nothing, counting what it
/// was asked.
#[derive(Default)]
struct Silent {
    calls: AtomicUsize,
    subscriptions: AtomicUsize,
}

impl AcpTransport for Silent {
    fn call(
        &self,
        method: &'static str,
        _params: Value,
        _max_bytes: usize,
    ) -> BoxFuture<'_, Result<Value, TransportError>> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        async move {
            Err(TransportError::other(format!(
                "{method}: node is not serving requests"
            )))
        }
        .boxed()
    }

    fn subscribe_headers(&self) -> BoxFuture<'_, Result<HeaderStream, TransportError>> {
        self.subscriptions.fetch_add(1, Ordering::Relaxed);
        async { Err(TransportError::other("node is not serving headers")) }.boxed()
    }
}

/// A fresh submission journal for every provider a test builds.
struct Workers {
    root: tempfile::TempDir,
    keys: keyring::FileKeyring,
    opened: usize,
}

impl Workers {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        let keys = keyring::FileKeyring::open(root.path().join("keys"), b"test").unwrap();
        Self {
            root,
            keys,
            opened: 0,
        }
    }

    fn next(&mut self) -> NativeWorker {
        self.opened += 1;
        let directory = self.root.path().join(self.opened.to_string());
        NativeWorker::open(&directory, &self.keys, 9001).unwrap()
    }
}

fn config_error(outcome: Result<VeraRsProvider, ProviderError>) -> String {
    match outcome {
        Err(ProviderError::Config(message)) => message,
        Err(other) => panic!("expected a configuration error, got {other}"),
        Ok(_) => panic!("expected a configuration error, got a provider"),
    }
}

#[nuclei::test]
async fn an_invalid_trusted_key_is_refused_before_a_transport_is_used() {
    let edge = edge::testing::edge();
    let tuning = AcpTuning::default();
    let mut workers = Workers::new();
    for key in ["", "not-hex", "00"] {
        let by_url = config_error(
            VeraRsProvider::new(
                UNREACHABLE.into(),
                key,
                &[42; 32],
                workers.next(),
                &tuning,
                None,
                &edge,
            )
            .await,
        );
        let rpc = Recorded::answering(Ok(Value::Null));
        let acp = Arc::new(Silent::default());
        let injected = config_error(
            VeraRsProvider::with_transport(
                rpc.clone(),
                acp.clone(),
                key,
                &[42; 32],
                workers.next(),
                &tuning,
                None,
                &edge,
            )
            .await,
        );
        assert!(injected.starts_with("invalid trusted consensus key: "));
        assert_eq!(injected, by_url);
        assert!(rpc.calls.pop().is_none());
        assert_eq!(acp.calls.load(Ordering::Relaxed), 0);
        assert_eq!(acp.subscriptions.load(Ordering::Relaxed), 0);
    }
}

#[nuclei::test]
async fn a_node_that_serves_no_header_fails_as_an_unreachable_url_does() {
    let edge = edge::testing::edge();
    let tuning = AcpTuning {
        receipt_timeout: Duration::from_millis(500),
        ..AcpTuning::default()
    };
    let mut workers = Workers::new();
    let by_url = config_error(
        VeraRsProvider::new(
            UNREACHABLE.into(),
            TRUSTED_KEY,
            &[42; 32],
            workers.next(),
            &tuning,
            None,
            &edge,
        )
        .await,
    );
    let rpc = Recorded::answering(Ok(Value::Null));
    let acp = Arc::new(Silent::default());
    let injected = config_error(
        VeraRsProvider::with_transport(
            rpc.clone(),
            acp.clone(),
            TRUSTED_KEY,
            &[42; 32],
            workers.next(),
            &tuning,
            None,
            &edge,
        )
        .await,
    );
    assert!(injected.starts_with("initial verified state: "));
    assert_eq!(injected, by_url);
    // The light client asked the injected transport for headers; with no
    // verified header, nothing was read or submitted.
    assert!(acp.subscriptions.load(Ordering::Relaxed) >= 1);
    assert_eq!(acp.calls.load(Ordering::Relaxed), 0);
    assert!(rpc.calls.pop().is_none());
}
