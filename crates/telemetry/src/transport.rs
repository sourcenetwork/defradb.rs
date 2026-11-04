//! The OTLP exporters' HTTP client. Each export goes out on nuclei sockets
//! through `net`'s client, as a task of the network runtime, so the SDK's
//! exporter threads never drive a socket themselves.

use std::fmt;
use std::time::Duration;

use edge::Edge;
use opentelemetry_http::{Bytes, HttpClient, HttpError, Request, Response};

#[derive(Clone)]
pub(crate) struct OtlpHttp {
    client: net::http::Client,
    edge: Edge,
}

impl OtlpHttp {
    pub(crate) fn new(edge: Edge, timeout: Duration) -> Result<Self, net::http::Error> {
        Ok(Self {
            client: net::http::Client::builder(edge.clone())
                .timeout(timeout)
                .build()?,
            edge,
        })
    }
}

impl fmt::Debug for OtlpHttp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("OtlpHttp").finish_non_exhaustive()
    }
}

#[async_trait::async_trait]
impl HttpClient for OtlpHttp {
    async fn send_bytes(&self, request: Request<Bytes>) -> Result<Response<Bytes>, HttpError> {
        let client = self.client.clone();
        let sent = self.edge.spawn(async move {
            let (parts, body) = request.into_parts();
            let response = client
                .request(parts.method, parts.uri.to_string())
                .headers(parts.headers)
                .body(body)
                .send()
                .await?;
            let mut answer = Response::new(Bytes::new());
            *answer.status_mut() = response.status();
            *answer.headers_mut() = response.headers().clone();
            *answer.body_mut() = response.bytes().await?;
            Ok::<_, net::http::Error>(answer)
        });
        Ok(sent.await??)
    }
}
