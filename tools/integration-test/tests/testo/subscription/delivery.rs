//! The update journal's delivery contract (`defra.events.delivery`): what a
//! reader of `_documentUpdates` can rely on across a reopen, a P2P merge and
//! a cursor it should not hold, and which arrivals a reader of
//! `_documentArrivals` is shown (`defra.events.arrivals`). Rust-only: Go has
//! neither journal.

mod arrivals;
mod beyond_head;
mod merge_once;
mod resume;
mod visibility;

use integration_test::actions::Request;
use integration_test::scenario::{Common, Text};

/// `_documentUpdates` of `collection` after the cursor `after` (plain text or
/// a capture), on `node`.
fn updates(
    collection: &str,
    after: Text,
    fields: &str,
    node: Option<&str>,
    expected_data: serde_json::Value,
    common: Common,
) -> Request {
    Request {
        request: Text::concat([
            Text::from(format!(
                r#"query {{ _documentUpdates(collection: "{collection}", after: ""#
            )),
            after,
            Text::from(format!(r#"") {{ {fields} }} }}"#)),
        ]),
        expected_data,
        common: Common {
            node: node.map(Into::into),
            ..common
        },
        ..Default::default()
    }
}
