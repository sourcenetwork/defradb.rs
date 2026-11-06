//! A local embedding model for tests, and the action that reads what it embedded.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};

use regolith::sync::{Atom, OnceCell};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use testo::prelude::*;

use crate::scenario::{Common, Failure, Op};
use crate::test_case::State;

/// The capture an `@embedding` directive takes its arguments from:
/// `provider: "openai", model: "...", url: "..."`.
pub const EMBEDDING_CAPTURE: &str = "embedding";
/// The words the model's vectors count, one dimension each.
pub const EMBEDDING_WORDS: [&str; 3] = ["rivers", "engines", "music"];
const MODEL_CAPTURE: &str = "embeddingModel";
const SEEN_CAPTURE: &str = "embeddingSeen";

/// What the model answers for `text`: how often each of [`EMBEDDING_WORDS`]
/// occurs in it.
pub fn embedding_of(text: &str) -> Vec<f64> {
    EMBEDDING_WORDS
        .iter()
        .map(|word| text.matches(word).count() as f64)
        .collect()
}

/// An OpenAI-compatible model on a local port that records every text it
/// embeds under the model name it was asked with.
struct Model {
    url: String,
    embedded: Atom<Vec<(String, String)>>,
    runs: AtomicU64,
}

fn model() -> &'static Model {
    static MODEL: OnceCell<Model> = OnceCell::new();
    // Two tests racing here bind two ports; every caller gets the winner's.
    MODEL.get_or_init_racy(|| {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind the embedding model");
        let url = format!("http://{}", listener.local_addr().expect("model address"));
        std::thread::spawn(move || {
            for stream in listener.incoming().flatten() {
                std::thread::spawn(move || answer(stream));
            }
        });
        Model {
            url,
            embedded: Atom::new(Vec::new()),
            runs: AtomicU64::new(0),
        }
    })
}

fn answer(mut stream: TcpStream) {
    let mut reader = BufReader::new(stream.try_clone().expect("clone the stream"));
    let mut length = 0;
    let mut line = String::new();
    while reader.read_line(&mut line).is_ok_and(|read| read > 0) {
        let header = line.trim().to_ascii_lowercase();
        if header.is_empty() {
            break;
        }
        if let Some(value) = header.strip_prefix("content-length:") {
            length = value.trim().parse().unwrap_or(0);
        }
        line.clear();
    }
    let mut body = vec![0; length];
    if reader.read_exact(&mut body).is_err() {
        return;
    }
    let request: Value = serde_json::from_slice(&body).unwrap_or_default();
    let name = request["model"].as_str().unwrap_or_default().to_owned();
    let texts: Vec<String> = match &request["input"] {
        Value::String(text) => vec![text.clone()],
        other => serde_json::from_value(other.clone()).unwrap_or_default(),
    };
    model().embedded.rcu(|embedded| {
        let mut embedded = embedded.clone();
        embedded.extend(texts.iter().map(|text| (name.clone(), text.clone())));
        embedded
    });
    let data: Vec<Value> = texts
        .iter()
        .enumerate()
        .map(|(index, text)| json!({ "index": index, "embedding": embedding_of(text) }))
        .collect();
    let body = json!({ "data": data }).to_string();
    let _ = write!(
        stream,
        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    );
}

/// The test's embedding model. The first run of this action in a test gives
/// the test a model of its own and captures [`EMBEDDING_CAPTURE`]; every run
/// returns `{"embedded": [...]}`, the texts the model embedded for the test
/// since the run before.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EmbeddingModel {
    #[serde(default, skip_serializing_if = "Value::is_null")]
    pub expected_data: Value,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected_error: Option<String>,
    #[serde(flatten)]
    pub common: Common,
}

impl Op for EmbeddingModel {
    fn common(&self) -> &Common {
        &self.common
    }

    fn expected_data(&self) -> &Value {
        &self.expected_data
    }

    fn expected_error(&self) -> Option<&str> {
        self.expected_error.as_deref()
    }

    fn run_mut(&self, state: &mut State) -> Result<Value, Failure> {
        self.common.plain(Self::name())?;
        let model = model();
        if !state.captures.contains_key(EMBEDDING_CAPTURE) {
            // A model per run, so another run's cached vectors never hide a call.
            let name = format!("test-model-{}", model.runs.fetch_add(1, Ordering::Relaxed));
            state.captures.insert(
                EMBEDDING_CAPTURE.to_owned(),
                json!(format!(
                    r#"provider: "openai", model: "{name}", url: "{}""#,
                    model.url
                )),
            );
            state.captures.insert(MODEL_CAPTURE.to_owned(), json!(name));
            state.captures.insert(SEEN_CAPTURE.to_owned(), json!(0));
        }
        let name = state.captures[MODEL_CAPTURE]
            .as_str()
            .unwrap_or_default()
            .to_owned();
        let seen = state.captures[SEEN_CAPTURE].as_u64().unwrap_or(0) as usize;
        let texts: Vec<String> = model
            .embedded
            .load_clone()
            .into_iter()
            .filter(|(model, _)| *model == name)
            .map(|(_, text)| text)
            .collect();
        state
            .captures
            .insert(SEEN_CAPTURE.to_owned(), json!(texts.len()));
        Ok(json!({ "embedded": texts[seen.min(texts.len())..] }))
    }
}

impl Action for EmbeddingModel {
    type TState = State;

    fn execute(&self, state: &mut State) {
        crate::scenario::execute(self, state);
    }
}
