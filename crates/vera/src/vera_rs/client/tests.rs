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
