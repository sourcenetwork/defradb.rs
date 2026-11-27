//! A filtered peer gets no relayed chunks (`defra.chunk.replicate.filtered.relay`).

use integration_test::actions::{Actions, AddReplicator};
use serde_json::{json, Value};

use super::{
    article_on, articles_are, body, chunks_are, connect, filtered, later_body, node_with,
    public_body, restart, secret_body, SCHEMA,
};
use crate::chunk::support::on;

fn replicator(from: &str, to: &str, filter: Value) -> Actions {
    AddReplicator {
        target: to.into(),
        collections: vec!["Article".into()],
        filter,
        common: on(Some(from)),
        ..Default::default()
    }
    .into()
}

/// `db0` pushes every article to `db1`, and `db1` only the public ones to
/// `db2`. `db1` restarts on its data when `persistent`.
fn relay(persistent: bool) -> Vec<Actions> {
    let mut actions: Vec<Actions> = [("db0", false), ("db1", persistent), ("db2", false)]
        .into_iter()
        .flat_map(|(name, persistent)| node_with(name, persistent, SCHEMA))
        .collect();
    actions.extend([
        replicator("db0", "db1", Value::Null),
        replicator("db1", "db2", json!({ "kind": { "_eq": "public" } })),
    ]);
    actions
}

/// A secret article goes from `db0` to `db1`, which then holds the articles
/// of `on_db1`. A public one written on `db1` after it is the last push to
/// `db2`, so `db2` then shows exactly the public articles of `on_db2`.
fn secret_then_public(
    secret: (&str, &str),
    public: (&str, &str),
    on_db1: &[(&str, &str)],
    on_db2: &[&str],
) -> Vec<Actions> {
    let kinds: Vec<&str> = on_db1.iter().map(|(kind, _)| *kind).collect();
    let texts: Vec<&str> = on_db1.iter().map(|(_, text)| *text).collect();
    let publics = vec!["public"; on_db2.len()];
    vec![
        article_on("db0", secret.0, "secret", secret.1),
        chunks_are("db1", &texts),
        articles_are("db1", &kinds),
        article_on("db1", public.0, "public", public.1),
        chunks_are("db2", on_db2),
        articles_are("db2", &publics),
    ]
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.relay")]
fn a_filtered_peer_gets_no_relayed_chunks() {
    let (secret, public) = (secret_body(), public_body());
    let mut actions = relay(false);
    actions.extend(secret_then_public(
        ("secret", &secret),
        ("public", &public),
        &[("secret", &secret)],
        &[&public],
    ));
    filtered(actions).execute();
}

#[test]
#[mantra_macros::req_test("defra.chunk.replicate.filtered.relay")]
fn a_restarted_relay_still_forwards_no_chunks() {
    let (secret, public) = (secret_body(), public_body());
    let (secret_after, public_after) = (body(2, "ponds"), later_body());
    let mut actions = relay(true);
    actions.extend(secret_then_public(
        ("secret", &secret),
        ("public", &public),
        &[("secret", &secret)],
        &[&public],
    ));
    actions.extend(restart("db1"));
    actions.extend([connect("db0", "db1"), connect("db1", "db2")]);
    actions.extend(secret_then_public(
        ("secret_after", &secret_after),
        ("public_after", &public_after),
        &[
            ("secret", &secret),
            ("public", &public),
            ("secret", &secret_after),
        ],
        &[&public, &public_after],
    ));
    filtered(actions).execute();
}
