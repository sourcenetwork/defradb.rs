//! License: AGPL-3.0-only OR LicenseRef-BSL-tests. Copyright 2026 Democratized Data Foundation.

use integration_test::actions::AddCollection;
use integration_test::test_case::TestCase;

#[test]
fn one_to_one_doc_relation_field_errors() {
    TestCase {
        id: integration_test::scenario_id!(),
        actions: vec![AddCollection {
            sdl: r#"
                    type Doc {
                        author: Author
                    }

                    type Author {
                        doc: Doc @primary
                    }
                "#
            .into(),
            // The `Author.doc` field results in a virtual id field named `_docID` being auto-added
            // this then clashes with the std `_docID` field (the litteral document id field).
            expected_error: Some("duplicate field. Name: _docID".to_string()),
            ..Default::default()
        }
        .into()],
        ..Default::default()
    }
    .execute();
}
