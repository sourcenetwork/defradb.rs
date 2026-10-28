use integration_test::export::WireStep;
use serde_json::Value;

/// Replaces each `document.parallel` / `action.parallel` step by its children
/// as steps of one async group. A group that is not shaped the way the
/// corpus writes them stays a single step.
pub fn expand(steps: Vec<WireStep>) -> Vec<WireStep> {
    steps.into_iter().flat_map(expand_one).collect()
}

fn expand_one(step: WireStep) -> Vec<WireStep> {
    if step.op != "document.parallel" && step.op != "action.parallel" {
        return vec![step];
    }
    match children(&step) {
        Some(children) => children,
        None => vec![step],
    }
}

fn children(step: &WireStep) -> Option<Vec<WireStep>> {
    let kids = step.args.get("children")?.as_array()?;
    let results = step
        .check
        .expect
        .as_ref()?
        .as_object()
        .filter(|m| m.len() == 1)?
        .get("results")?
        .as_array()
        .filter(|r| r.len() == kids.len())?;
    let plain = step.check.expect_error.is_none()
        && step.check.capture.is_empty()
        && step.check.unordered.is_empty()
        && step.check.unordered_documents.is_empty()
        && step.check.compare.is_none();
    if !plain || step.args.as_object()?.len() != 2 {
        return None;
    }
    kids.iter()
        .zip(results)
        .map(|(kid, expected)| child(kid, expected))
        .collect()
}

fn child(kid: &Value, expected: &Value) -> Option<WireStep> {
    let mut step: WireStep = serde_json::from_value(kid.clone()).ok()?;
    if step.check.expect.is_some() {
        return None;
    }
    step.check.expect = Some(expected.clone());
    step.envelope = true;
    Some(step)
}
