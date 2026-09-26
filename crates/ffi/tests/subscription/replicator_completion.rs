use events::{Message, ReplicatorCompletedData};
use serde_json::json;

use super::create::message_to_json;

#[test]
fn replicator_completion_preserves_outcomes_and_legacy_shape() {
    for (skipped, error) in [(false, None), (true, None), (false, Some("replay failed"))] {
        let message = Message::replicator_completed_with_data(ReplicatorCompletedData {
            peer_id: "peer-a".into(),
            collections: vec!["Note".into(), "User".into()],
            skipped,
            error: error.map(str::to_owned),
        });
        let value: serde_json::Value = serde_json::from_str(&message_to_json(&message)).unwrap();
        assert_eq!(
            value,
            json!({
                "type": "replicator_completed",
                "peer_id": "peer-a",
                "collections": ["Note", "User"],
                "skipped": skipped,
                "error": error,
            })
        );
    }
    let legacy: serde_json::Value =
        serde_json::from_str(&message_to_json(&Message::replicator_completed())).unwrap();
    assert_eq!(legacy, json!({"type": "replicator_completed"}));
}
