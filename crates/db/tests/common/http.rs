//! A blocking JSON-over-HTTP/1.1 server on plain std threads, for tests whose
//! code under test makes network requests (embedding providers). It owns no
//! runtime: the network side of the request runs wherever the code under test
//! puts it.

#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use serde_json::Value;

/// One request the server received.
pub struct Request {
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: Value,
}

impl Request {
    /// The value of header `name`, matched case-insensitively.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(key, _)| key.eq_ignore_ascii_case(name))
            .map(|(_, value)| value.as_str())
    }
}

/// A running server; dropped, it stops accepting.
pub struct HttpServer {
    address: SocketAddr,
    stop: Arc<AtomicBool>,
    accept: Option<std::thread::JoinHandle<()>>,
}

impl HttpServer {
    /// Answers every request with `handler`'s JSON. A handler that panics
    /// answers 500.
    pub fn start<H>(handler: H) -> Self
    where
        H: Fn(Request) -> Value + Send + Sync + 'static,
    {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let handler = Arc::new(handler);
        let accept = {
            let stop = Arc::clone(&stop);
            std::thread::spawn(move || {
                for stream in listener.incoming() {
                    if stop.load(Ordering::SeqCst) {
                        return;
                    }
                    let Ok(stream) = stream else { continue };
                    let handler = Arc::clone(&handler);
                    std::thread::spawn(move || serve_connection(stream, &*handler));
                }
            })
        };
        Self {
            address,
            stop,
            accept: Some(accept),
        }
    }

    /// `http://host:port`, no trailing slash.
    pub fn url(&self) -> String {
        format!("http://{}", self.address)
    }
}

impl Drop for HttpServer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.address);
        if let Some(accept) = self.accept.take() {
            let _ = accept.join();
        }
    }
}

fn serve_connection<H>(stream: TcpStream, handler: &H)
where
    H: Fn(Request) -> Value,
{
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    while let Some(request) = read_request(&mut reader) {
        let (status, body) = match catch_unwind(AssertUnwindSafe(|| handler(request))) {
            Ok(body) => ("200 OK", body.to_string()),
            Err(_) => ("500 Internal Server Error", String::new()),
        };
        let response = format!(
            "HTTP/1.1 {status}\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<Request> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let path = line.split_whitespace().nth(1)?.to_string();
    let mut headers = Vec::new();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let line = line.trim_end();
        if line.is_empty() {
            break;
        }
        let (key, value) = line.split_once(':')?;
        if key.eq_ignore_ascii_case("content-length") {
            length = value.trim().parse().ok()?;
        }
        headers.push((key.to_string(), value.trim().to_string()));
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some(Request {
        path,
        headers,
        body: serde_json::from_slice(&body).unwrap_or(Value::Null),
    })
}
