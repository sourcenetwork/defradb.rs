use std::time::Duration;

use crate::error::Result;
use reqwest::Client;

/// Poll a node's health endpoint until it returns 200.
pub async fn health_check(client: &Client, url: &str, timeout: Duration) -> Result<()> {
    let health_url = format!("{}/health-check", url);
    let polling = async {
        loop {
            match client.get(&health_url).send().await {
                Ok(resp) if resp.status().is_success() => return,
                _ => {}
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    };

    match tokio::time::timeout(timeout, polling).await {
        Ok(()) => Ok(()),
        Err(_) => Err(crate::error::err!(
            "health check timed out for {}",
            health_url
        )),
    }
}

/// Run health checks on all URLs concurrently.
pub async fn health_check_all(client: &Client, urls: &[String], timeout: Duration) -> Result<()> {
    let checks: Vec<_> = urls
        .iter()
        .map(|url| health_check(client, url, timeout))
        .collect();

    let results = futures::future::join_all(checks).await;
    for result in results {
        result?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::sync::mpsc;
    use std::time::Instant;

    #[tokio::test]
    async fn timeout_bounds_a_response_that_never_arrives() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind stalled server");
        let address = listener.local_addr().expect("stalled server address");
        let (release, released) = mpsc::channel();
        let server = std::thread::spawn(move || {
            let (_stream, _) = listener.accept().expect("accept health request");
            let _ = released.recv_timeout(Duration::from_secs(3));
        });

        let started = Instant::now();
        let error = health_check(
            &Client::new(),
            &format!("http://{address}"),
            Duration::from_millis(200),
        )
        .await
        .expect_err("a stalled response must time out");

        assert!(error.to_string().contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(1));
        release.send(()).expect("release stalled server");
        server.join().expect("stalled server");
    }
}
