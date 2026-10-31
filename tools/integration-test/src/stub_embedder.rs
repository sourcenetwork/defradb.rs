//! A deterministic embedding provider for tests: answers the Ollama
//! (`{"model", "prompt"}` → `{"embedding"}`) and OpenAI-compatible
//! (`{"model", "input"}` → `{"data": [{"embedding"}]}`) `/embeddings` calls
//! the node's embedding client makes, with a vector drawn from a hash of the
//! text. Same text, same vector, so tests run without a model server.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};

use regolith::sync::Lazy;
use serde_json::{json, Value};

/// The endpoints the corpus's `@embedding` directives name, and the stub
/// paths they become.
const PROVIDERS: [(&str, &str); 2] = [
    ("http://localhost:11434/api", "/api"),
    ("https://api.openai.com/v1", "/v1"),
];

static BASE: Lazy<String> = Lazy::new(start);

/// `sdl` with every provider URL pointed at the stub.
pub fn route(sdl: &str) -> String {
    PROVIDERS.iter().fold(sdl.to_owned(), |sdl, (real, path)| {
        sdl.replace(real, &format!("{}{path}", *BASE))
    })
}

fn start() -> String {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind the stub embedder");
    let address = listener.local_addr().expect("stub embedder address");
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
    });
    format!("http://{address}")
}

fn serve(stream: TcpStream) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    while let Some((path, body)) = read_request(&mut reader) {
        let (status, body) = match (path.ends_with("/embeddings"), body) {
            (true, Some(body)) => ("200 OK", answer(&body).to_string()),
            (false, _) => ("404 Not Found", String::new()),
            (true, None) => ("400 Bad Request", String::new()),
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

/// The provider's answer to one request body.
fn answer(body: &Value) -> Value {
    let dimensions = match body["model"].as_str() {
        Some("text-embedding-3-small") => 1536,
        _ => 768,
    };
    if let Some(prompt) = body["prompt"].as_str() {
        return json!({ "embedding": vector(prompt, dimensions) });
    }
    let texts: Vec<&str> = match &body["input"] {
        Value::String(text) => vec![text],
        Value::Array(texts) => texts.iter().filter_map(Value::as_str).collect(),
        _ => Vec::new(),
    };
    let data: Vec<Value> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "index": index, "embedding": vector(text, dimensions) }))
        .collect();
    json!({ "data": data })
}

/// `dimensions` values in [-1, 1), from FNV-1a of `text` fed through
/// splitmix64.
pub fn vector(text: &str, dimensions: usize) -> Vec<f64> {
    let mut state = text.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    (0..dimensions)
        .map(|_| {
            state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
            let mut z = state;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
            z ^= z >> 31;
            (z >> 11) as f64 / (1_u64 << 53) as f64 * 2.0 - 1.0
        })
        .collect()
}

fn read_request(reader: &mut BufReader<TcpStream>) -> Option<(String, Option<Value>)> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let path = line.split_whitespace().nth(1)?.to_owned();
    let mut length = 0;
    loop {
        line.clear();
        reader.read_line(&mut line).ok()?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        if let Some((key, value)) = header.split_once(':') {
            if key.eq_ignore_ascii_case("content-length") {
                length = value.trim().parse().ok()?;
            }
        }
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    Some((path, serde_json::from_slice(&body).ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_same_vector() {
        assert_eq!(vector("John", 768), vector("John", 768));
        assert_ne!(vector("John", 768), vector("Fred", 768));
        assert!(vector("", 4).iter().all(|v| (-1.0..1.0).contains(v)));
    }

    #[test]
    fn answers_both_provider_shapes() {
        let ollama = answer(&json!({ "model": "nomic-embed-text", "prompt": "a" }));
        assert_eq!(ollama["embedding"].as_array().unwrap().len(), 768);
        let openai = answer(&json!({ "model": "text-embedding-3-small", "input": ["a", "b"] }));
        assert_eq!(
            openai["data"][1]["embedding"].as_array().unwrap().len(),
            1536
        );
    }

    #[test]
    fn routes_the_corpus_urls() {
        let sdl = route(r#"url: "http://localhost:11434/api""#);
        assert!(sdl.starts_with(r#"url: "http://127.0.0.1:"#) && sdl.ends_with(r#"/api""#));
    }
}
