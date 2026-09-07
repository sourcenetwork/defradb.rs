use super::*;
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;

async fn server(response: String) -> (HubRsClient, tokio::task::JoinHandle<Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let task = tokio::spawn(async move {
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
        serde_json::from_slice(&body).unwrap()
    });
    (HubRsClient::new(url, Duration::from_secs(2)).unwrap(), task)
}

fn http(body: &str) -> String {
    format!(
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
}

#[tokio::test]
async fn rpc_binds_response_and_requires_a_result() {
    for body in [
        json!({"jsonrpc":"2.0", "id":2, "result":null}),
        json!({"jsonrpc":"1.0", "id":1, "result":null}),
        json!({"jsonrpc":"2.0", "id":1}),
    ] {
        let (client, task) = server(http(&body.to_string())).await;
        let result = client.rpc::<Option<Value>>("test", json!([]), 4096).await;
        assert!(matches!(result, Err(ClientError::InvalidResponse(_))));
        assert_eq!(task.await.unwrap()["id"], 1);
    }
    let (client, task) = server(http(r#"{"jsonrpc":"2.0","id":1,"result":null}"#)).await;
    assert!(client
        .rpc::<Option<Value>>("test", json!([]), 4096)
        .await
        .unwrap()
        .is_none());
    task.await.unwrap();
}

#[tokio::test]
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
        task.await.unwrap();
    }
}

#[tokio::test]
async fn submission_id_must_match_locally_signed_bytes() {
    let signer = hub_client::BlsSigner::random(9001).unwrap();
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
        let request = task.await.unwrap();
        assert_eq!(request["method"], "hub_sendNativeTx");
        assert_eq!(request["params"], json!([Bytes::copy_from_slice(&wire)]));
    }
}

#[tokio::test]
async fn stalled_transport_times_out_and_transient_rpc_errors_are_retryable() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let client = HubRsClient::new(
        format!("http://{}", listener.local_addr().unwrap()),
        Duration::from_millis(100),
    )
    .unwrap();
    let (result, connection) = tokio::join!(
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
        task.await.unwrap();
    }
}
