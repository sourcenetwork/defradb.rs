use super::*;
use alloy_primitives::Address;
use nuclei::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use nuclei::net::TcpListener;
use serde_json::{json, Value};

async fn server(response: String) -> (VeraRsClient, nuclei::Task<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.get_ref().local_addr().unwrap());
    let task = nuclei::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let mut stream = BufReader::new(stream);
        let mut length = 0;
        loop {
            let mut line = String::new();
            assert_ne!(stream.read_line(&mut line).await.unwrap(), 0);
            if line == "\r\n" {
                break;
            }
            if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        assert!(length <= 65536);
        let mut body = vec![0; length];
        stream.read_exact(&mut body).await.unwrap();
        stream
            .get_mut()
            .write_all(response.as_bytes())
            .await
            .unwrap();
        stream.get_mut().flush().await.unwrap();
        serde_json::from_slice(&body).unwrap()
    });
    (
        VeraRsClient::new(url, Duration::from_secs(2), &edge::testing::edge()).unwrap(),
        task,
    )
}

fn http(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[nuclei::test]
async fn rpc_binds_response_and_requires_a_result() {
    for body in [
        json!({"jsonrpc":"2.0", "id":2, "result":null}),
        json!({"jsonrpc":"1.0", "id":1, "result":null}),
        json!({"jsonrpc":"2.0", "id":1}),
    ] {
        let (client, task) = server(http(&body.to_string())).await;
        let result = client.rpc::<Option<Value>>("test", json!([]), 4096).await;
        assert!(matches!(result, Err(ClientError::InvalidResponse(_))));
        assert_eq!(task.join().await.unwrap()["id"], 1);
    }
    let (client, task) = server(http(r#"{"jsonrpc":"2.0","id":1,"result":null}"#)).await;
    assert!(client
        .rpc::<Option<Value>>("test", json!([]), 4096)
        .await
        .unwrap()
        .is_none());
    task.join().await.unwrap();
}

#[nuclei::test]
async fn rpc_enforces_declared_and_streamed_byte_limits() {
    for response in [
        http(&" ".repeat(65)),
        format!("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n41\r\n{}\r\n0\r\n\r\n", " ".repeat(65)),
    ] {
        let (client, task) = server(response).await;
        assert!(matches!(
            client.rpc::<Value>("test", json!([]), 64).await,
            Err(ClientError::InvalidResponse("response exceeds byte limit"))
        ));
        task.join().await.unwrap();
    }
}

#[nuclei::test]
async fn submission_id_must_match_locally_signed_bytes() {
    let signer = vera_client::BlsSigner::random(9001).unwrap();
    let wire = signer
        .sign_native_tx_with_sequence(Address::ZERO, Bytes::new(), 0)
        .unwrap();
    let hash = NativeTx::decode_wire(&wire).unwrap().tx_id().0;
    for returned in [B256::ZERO, hash] {
        let body = json!({"jsonrpc":"2.0", "id":1, "result":returned});
        let (client, task) = server(http(&body.to_string())).await;
        let result = client.send(&wire).await;
        if returned == hash {
            assert_eq!(result.unwrap(), hash);
        } else {
            assert!(matches!(
                result,
                Err(ClientError::InvalidResponse("submission ID mismatch"))
            ));
        }
        let request = task.join().await.unwrap();
        assert_eq!(request["method"], "vera_sendNativeTx");
        assert_eq!(request["params"], json!([Bytes::copy_from_slice(&wire)]));
    }
}

#[nuclei::test]
async fn stalled_transport_times_out_and_transient_rpc_errors_are_retryable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = VeraRsClient::new(
        format!("http://{}", listener.get_ref().local_addr().unwrap()),
        Duration::from_millis(100),
        &edge::testing::edge(),
    )
    .unwrap();
    let (result, connection) = nuclei::join!(
        client.rpc::<Value>("test", json!([]), 64),
        listener.accept()
    );
    let error = result.unwrap_err();
    assert!(matches!(&error, ClientError::Http(error) if error.is_timeout()));
    assert!(error.retryable());
    drop(connection.unwrap());
    for (code, retryable) in [(-32000, true), (-32002, true), (-32602, false)] {
        let body = json!({"jsonrpc":"2.0", "id":1, "error":{"code":code,"message":"unavailable"}});
        let (client, task) = server(http(&body.to_string())).await;
        let error = client
            .rpc::<Value>("test", json!([]), 4096)
            .await
            .unwrap_err();
        assert_eq!(error.retryable(), retryable);
        task.join().await.unwrap();
    }
}

/// A well-formed consensus key that signed nothing these tests return.
const TRUSTED_KEY: &str = "92b050b6fbe80695b5d56835e978918e37c8707a7fad09a01ae782d4c3170c9baa4c2c196b36eac6b78ceb210b287aeb0727ef1c60e48042142f7bcc8b6382305cd50c5a4542c44ec72a4de6640c194f8ef36bea1dbed168ab6fd8681d910d55";

/// Answers every call with a fixed result and records what it was asked.
struct Recorded {
    result: Result<Value, (bool, &'static str)>,
    calls: regolith::sync::SegQueue<(String, Value, usize)>,
}

impl Recorded {
    fn answering(result: Result<Value, (bool, &'static str)>) -> Arc<Self> {
        Arc::new(Self {
            result,
            calls: regolith::sync::SegQueue::new(),
        })
    }

    fn only_call(&self) -> (String, Value, usize) {
        let call = self.calls.pop().expect("one call");
        assert!(self.calls.pop().is_none(), "more than one call");
        call
    }
}

impl RpcTransport for Recorded {
    fn call<'a>(
        &'a self,
        method: &'a str,
        params: Value,
        max_bytes: usize,
    ) -> MaybeBoxFuture<'a, Result<Value, ClientError>> {
        self.calls.push((method.to_owned(), params, max_bytes));
        Box::pin(async move {
            self.result
                .clone()
                .map_err(|(retryable, message)| ClientError::Transport {
                    retryable,
                    message: message.to_owned(),
                })
        })
    }
}

#[nuclei::test]
async fn injected_transport_keeps_the_submission_and_receipt_checks() {
    let signer = vera_client::BlsSigner::random(9001).unwrap();
    let wire = signer
        .sign_native_tx_with_sequence(Address::ZERO, Bytes::new(), 0)
        .unwrap();
    let hash = NativeTx::decode_wire(&wire).unwrap().tx_id().0;
    for returned in [B256::ZERO, hash] {
        let transport = Recorded::answering(Ok(json!(returned)));
        let client = VeraRsClient::with_transport(transport.clone());
        let result = client.send(&wire).await;
        if returned == hash {
            assert_eq!(result.unwrap(), hash);
        } else {
            assert!(matches!(
                result,
                Err(ClientError::InvalidResponse("submission ID mismatch"))
            ));
        }
        assert_eq!(
            transport.only_call(),
            (
                "vera_sendNativeTx".to_owned(),
                json!([Bytes::copy_from_slice(&wire)]),
                4096
            )
        );
    }

    let trusted = {
        use commonware_codec::DecodeExt as _;
        ConsensusPublicKey::decode(hex::decode(TRUSTED_KEY).unwrap()).unwrap()
    };
    let absent = Recorded::answering(Ok(Value::Null));
    let client = VeraRsClient::with_transport(absent.clone());
    assert!(client.receipt(hash, &trusted).await.unwrap().is_none());
    assert_eq!(
        absent.only_call(),
        (
            "vera_getReceiptProof".to_owned(),
            json!([hash]),
            RECEIPT_RESPONSE_BYTES
        )
    );

    // A receipt the trusted key did not certify is refused, whatever carried it.
    let uncertified = json!({
        "revision": {
            "block_hash": format!("{:#x}", B256::ZERO),
            "parent_hash": format!("{:#x}", B256::ZERO),
            "height": 1,
            "timestamp": 1,
            "state_root": format!("{:#x}", B256::ZERO),
            "module_state_root": format!("{:#x}", B256::ZERO),
            "epoch": 0,
            "view": 1,
            "parent_view": 0,
            "block": "0x00",
            "finalization": "0x00",
            "epoch_material": "0x00",
        },
        "gas_limit": 0,
        "receipts": [],
    });
    let client = VeraRsClient::with_transport(Recorded::answering(Ok(uncertified)));
    assert!(matches!(
        client.receipt(hash, &trusted).await,
        Err(ClientError::Receipt(_))
    ));
}

#[nuclei::test]
async fn transport_failures_say_whether_the_call_may_be_sent_again() {
    for retryable in [true, false] {
        let client = VeraRsClient::with_transport(Recorded::answering(Err((
            retryable,
            "node is not serving requests",
        ))));
        let error = client
            .rpc::<Value>("test", json!([]), 4096)
            .await
            .unwrap_err();
        assert!(matches!(&error, ClientError::Transport { .. }));
        assert_eq!(error.retryable(), retryable);
    }
}
