//! A create in an explicit transaction embeds its chunks before the
//! statement runs again (`defra.chunk.embed.transaction`).

use integration_test::actions::{Request, TxnBegin, TxnCommit};
use integration_test::test_case::TestCase;

use super::embed_support::{
    add_embedded_collection, chunks_have_vectors, chunks_of, embedding_input, model_embedded,
};
use super::support::{body, gql};

const PARAGRAPHS: usize = 3;

#[test]
#[mantra_macros::req_test("defra.chunk.embed.transaction")]
fn an_explicit_transaction_embeds_its_chunks() {
    let chunks = chunks_of(PARAGRAPHS, "engines");
    let inputs: Vec<String> = chunks.iter().map(|chunk| embedding_input(chunk)).collect();
    TestCase::from_actions(vec![
        model_embedded(&[]),
        add_embedded_collection(None),
        TxnBegin::default().into(),
        Request {
            request: format!(
                r#"mutation {{ add_Article(input: {{title: "t", body: "{}"}}) {{ _docID }} }}"#,
                gql(&body(PARAGRAPHS, "engines"))
            )
            .into(),
            txn: Some(0),
            ..Default::default()
        }
        .into(),
        TxnCommit {
            txn: 0,
            ..Default::default()
        }
        .into(),
        model_embedded(&inputs),
        chunks_have_vectors(&chunks, None),
    ])
    .execute();
}
