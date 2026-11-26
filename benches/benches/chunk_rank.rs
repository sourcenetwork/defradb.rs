//! Ranking articles by their best chunk, unrouted and through the chunk vector index.
//!
//! `cargo bench --profile release-fast -p benches --bench chunk_rank`
//! Each run first prints the explain counters of both queries.

use std::hint::black_box;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::Arc;

use criterion::{criterion_group, criterion_main, Criterion};
use db::{AutoCommitMutator, DbCollectionProvider, DbTransactionRegistry, LensedAutoCommitFetcher};
use query::{QueryExecutor, QueryRequest, QueryRunner};
use serde_json::{json, Value};

mod common;

const ARTICLES: usize = 200;
const PARAGRAPHS: usize = 20;
const DIMENSIONS: usize = 16;
const LIMIT: usize = 10;

type Runner = QueryRunner<
    LensedAutoCommitFetcher<storage::RegolithStore>,
    DbTransactionRegistry<storage::RegolithStore>,
>;

/// A deterministic vector for `text`, so every run ranks the same articles.
fn vector_of(text: &str) -> Vec<f64> {
    let mut state = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    });
    (0..DIMENSIONS)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state % 2000) as f64 / 1000.0 - 1.0
        })
        .collect()
}

/// An embedding provider on a plain thread, answering `/embeddings` with [`vector_of`].
fn start_provider() -> SocketAddr {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    std::thread::spawn(move || {
        for stream in listener.incoming().flatten() {
            std::thread::spawn(move || serve(stream));
        }
    });
    address
}

fn serve(stream: TcpStream) {
    let Ok(mut writer) = stream.try_clone() else {
        return;
    };
    let mut reader = BufReader::new(stream);
    while let Some(body) = read_body(&mut reader) {
        let inputs: Vec<String> = match &body["input"] {
            Value::String(text) => vec![text.clone()],
            other => serde_json::from_value(other.clone()).unwrap_or_default(),
        };
        let data: Vec<Value> = inputs
            .iter()
            .enumerate()
            .map(|(index, text)| json!({"index": index, "embedding": vector_of(text)}))
            .collect();
        let body = json!({ "data": data }).to_string();
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\n\r\n{body}",
            body.len()
        );
        if writer.write_all(response.as_bytes()).is_err() {
            return;
        }
    }
}

fn read_body(reader: &mut BufReader<TcpStream>) -> Option<Value> {
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
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
    }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
    serde_json::from_slice(&body).ok()
}

/// Article `n`'s body: one paragraph per chunk, escaped for a GraphQL string.
fn body(n: usize) -> String {
    (0..PARAGRAPHS)
        .map(|i| format!("Article {n} paragraph {i} tells a story."))
        .collect::<Vec<_>>()
        .join("\\n\\n")
}

async fn run(runner: &Runner, query: &str) -> Value {
    let response = runner.execute(QueryRequest::new(query)).await;
    assert!(!response.has_errors(), "{:?}", response.errors);
    response.data.expect("data")
}

async fn fixture() -> Runner {
    let provider = start_provider();
    let db = Arc::new(db::DB::new(storage::RegolithStore::in_memory().unwrap()).unwrap());
    let sdl = format!(
        r#"type Article {{
            body: String @chunk(maxChars: 64)
            body_v: [Float32!] @embedding(fields: ["body"], provider: "openai", model: "bench", url: "http://{provider}") @index(vector: {{dimensions: {DIMENSIONS}}})
        }}"#
    );
    db.add_collection(&defra_core::RequestContext::ambient(), &sdl)
        .await
        .unwrap();
    let runner = QueryRunner::with_arc_registry_and_provider(
        LensedAutoCommitFetcher::new(Arc::clone(&db)),
        DbCollectionProvider::new_arc(Arc::clone(&db)),
        Arc::new(DbTransactionRegistry::new(Arc::clone(&db))),
    )
    .with_mutator(Arc::new(AutoCommitMutator::new(Arc::clone(&db))));
    for batch in (0..ARTICLES).collect::<Vec<_>>().chunks(20) {
        let input: Vec<String> = batch
            .iter()
            .map(|n| format!(r#"{{body: "{}"}}"#, body(*n)))
            .collect();
        run(
            &runner,
            &format!(
                "mutation {{ add_Article(input: [{}]) {{ _docID }} }}",
                input.join(", ")
            ),
        )
        .await;
    }
    runner
}

fn ranking(arguments: &str) -> String {
    let vector = vector_of("the query");
    format!(
        "query {{ Article(order: {{_alias: {{sim: DESC}}}}{arguments}) {{ _docID sim: SIMILARITY(body_v: {{vector: {vector:?}}}) }} }}"
    )
}

/// The read counters of every plan node that reports them.
fn counters(plan: &Value, found: &mut Vec<Value>) {
    match plan {
        Value::Object(node) => {
            if node.contains_key("docFetches") {
                let picked: serde_json::Map<String, Value> = node
                    .iter()
                    .filter(|(_, value)| !value.is_object() && !value.is_array())
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect();
                found.push(Value::Object(picked));
            }
            node.values().for_each(|value| counters(value, found));
        }
        Value::Array(items) => items.iter().for_each(|value| counters(value, found)),
        _ => {}
    }
}

fn chunk_rank(c: &mut Criterion) {
    let runner = workers::block_on(fixture()).unwrap();
    let cases = [
        ("unrouted".to_string(), String::new()),
        (format!("routed_limit_{LIMIT}"), format!(", limit: {LIMIT}")),
    ];
    let mut group = c.benchmark_group("chunk_rank");
    group.sample_size(20);
    for (name, arguments) in &cases {
        let query = ranking(arguments);
        let explain = query.replacen("query", "query @explain(type: execute)", 1);
        let plan = workers::block_on(run(&runner, &explain)).unwrap();
        let mut found = Vec::new();
        counters(&plan, &mut found);
        eprintln!("chunk_rank {name} {ARTICLES}x{PARAGRAPHS} explain: {found:?}");
        let rows = workers::block_on(run(&runner, &query)).unwrap();
        let rows = rows["Article"].as_array().map_or(0, Vec::len);
        eprintln!("chunk_rank {name} rows: {rows}");
        group.bench_function(name.as_str(), |b| {
            b.iter(|| {
                let data = workers::block_on(run(&runner, black_box(&query))).unwrap();
                black_box(data)
            })
        });
    }
    group.finish();
}

criterion_group!(benches, chunk_rank);
criterion_main!(benches);
