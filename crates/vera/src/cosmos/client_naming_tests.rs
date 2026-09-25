use super::VeraClient;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

#[tokio::test]
async fn lcd_requests_use_vera_routes() {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = format!("http://{}", listener.local_addr().unwrap());
    let server = tokio::spawn(async move {
        for path in [
            "/sourcenetwork/vera/acp/policy/policy",
            "/sourcenetwork/vera/acp/object_owner/policy/document/doc",
        ] {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            loop {
                let mut buffer = [0; 1024];
                let read = stream.read(&mut buffer).await.unwrap();
                assert_ne!(read, 0, "connection closed before request headers");
                request.extend_from_slice(&buffer[..read]);
                if request.windows(4).any(|part| part == b"\r\n\r\n") {
                    break;
                }
            }
            assert!(String::from_utf8(request)
                .unwrap()
                .starts_with(&format!("GET {path} HTTP/1.1\r\n")));
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}").await.unwrap();
        }
    });
    let client = VeraClient::new(
        address.clone(),
        address.clone(),
        address,
        Duration::from_secs(2),
    )
    .unwrap();
    assert!(client.query_policy("policy").await.unwrap().is_none());
    assert_eq!(
        client
            .query_object_owner("policy", "document", "doc")
            .await
            .unwrap(),
        (false, String::new())
    );
    tokio::time::timeout(Duration::from_secs(3), server)
        .await
        .unwrap()
        .unwrap();
}
