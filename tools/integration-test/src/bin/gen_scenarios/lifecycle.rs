use integration_test::export::WireStep;
use serde_json::Value;

const DB: &str = "db";

fn node(step: &WireStep) -> &str {
    step.args.get("node").and_then(Value::as_str).unwrap_or(DB)
}

fn is_empty_object(value: &Option<Value>) -> bool {
    value
        .as_ref()
        .and_then(Value::as_object)
        .is_some_and(|m| m.is_empty())
}

/// Drops the implicit lifecycle the exporter writes back: a first plain
/// `node.open` of `db`, and a trailing run of `node.close` steps that closes
/// every opened node. Returns whether the scenario never opens `db`.
pub fn strip_implicit(steps: &mut Vec<WireStep>) -> bool {
    let mut opened: Vec<String> = Vec::new();
    for step in steps.iter().filter(|s| s.op.starts_with("node.open")) {
        if !opened.iter().any(|n| n == node(step)) {
            opened.push(node(step).to_owned());
        }
    }
    let first_is_db_open = steps.first().is_some_and(|s| {
        s.op == "node.open"
            && node(s) == DB
            && is_empty_object(&s.check.expect)
            && s.check.expect_error.is_none()
    });
    if first_is_db_open {
        steps.remove(0);
    }
    let trailing = steps
        .iter()
        .rev()
        .take_while(|s| s.op == "node.close")
        .count();
    let closed: Vec<&str> = steps[steps.len() - trailing..].iter().map(node).collect();
    let mut sorted_closed = closed.clone();
    sorted_closed.sort_unstable();
    sorted_closed.dedup();
    let mut sorted_opened: Vec<&str> = opened.iter().map(String::as_str).collect();
    sorted_opened.sort_unstable();
    let covers = trailing == sorted_opened.len() && sorted_closed == sorted_opened;
    if covers && trailing > 0 {
        let keep = steps.len() - trailing;
        steps.truncate(keep);
    }
    !opened.iter().any(|n| n == DB)
}
