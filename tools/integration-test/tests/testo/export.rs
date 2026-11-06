use integration_test::actions::AddDoc;
use integration_test::export::{from_wire, to_wire, v, WireStep};
use integration_test::m;
use integration_test::scenario::{captures, Common};
use serde_json::json;

#[test]
fn action_exports_in_the_corpus_shape_and_maps_retry_twins() {
    let built = AddDoc {
        collection: "Users".to_owned(),
        doc_map: Some(json!({"name": "Fred"})),
        expected_data: json!({"docIDs": [m::cid()]}),
        common: Common {
            retry: true,
            capture: captures([("id", "/docIDs/0")]),
            unordered: vec!["/a".into()],
            ..Default::default()
        },
        ..Default::default()
    };
    let text = serde_json::to_value(to_wire(&built, &[v("document.create")])).expect("serialize");
    assert_eq!(text["op"], "document.createWithRetry");
    assert_eq!(text["args"]["collection"], "Users");
    assert_eq!(text["capture"]["id"], "/docIDs/0");
    assert_eq!(text["unordered"], json!(["/a"]));
    let wire: WireStep = serde_json::from_value(text).expect("parse");
    assert_eq!(
        from_wire::<AddDoc>(&wire, &[v("document.create")]),
        Ok(built)
    );
}
