//! A library policy whose `book` resource gates creates on `author->publish`,
//! and the actions that add it and a schema naming it.

use integration_test::actions::Actions;
use integration_test::actions::{AddCollection, AddDACPolicy};
use integration_test::m;
use integration_test::scenario::{captures, Common, Text};
use integration_test::ClientType;
use serde_json::json;
use testo::action::ActionInstance;

/// One action of a test.
pub type Action = ActionInstance<Actions>;

/// The clients that run these tests. Go has no `@policy(relation:)`.
pub const CLIENTS: &[ClientType] = &[ClientType::InProcess, ClientType::RustHttp, ClientType::Cli];

/// The library policy; `book.author` admits exactly `author`.
pub const LIBRARY: &str = r#"
name: library
resources:
- name: author
  permissions:
  - name: publish
    expr: publisher
  - name: read
  - name: update
  - name: delete
  relations:
  - name: publisher
    types:
    - actor
- name: book
  permissions:
  - name: read
    expr: author->publish
  - name: update
  - name: delete
  relations:
  - name: author
    types:
    - author
  - name: editor
    types:
    - actor
  collection:
    create: author->publish
"#;

/// Add `policy`, capturing its ID as `name`.
pub fn add_policy(policy: &str, name: &str) -> Action {
    AddDACPolicy {
        policy: policy.into(),
        expected_data: json!({ "policyID": m::nonempty_string() }),
        common: Common {
            identity_seed: Some(1),
            capture: captures([(name, "/policyID")]),
            ..Default::default()
        },
        ..Default::default()
    }
    .into()
}

/// `sdl` with every `{name}` replaced by the policy ID captured as `name`.
pub fn sdl(template: &str, names: &[&str]) -> Text {
    let mut parts = Vec::new();
    let mut rest = template;
    while let Some((start, name)) = names
        .iter()
        .filter_map(|name| rest.find(&format!("{{{name}}}")).map(|at| (at, *name)))
        .min_by_key(|(at, _)| *at)
    {
        parts.push(Text::from(&rest[..start]));
        parts.push(Text::reference(name));
        rest = &rest[start + name.len() + 2..];
    }
    parts.push(Text::from(rest));
    Text::concat(parts)
}

/// Add the collections `template` defines, expecting `error` if given.
pub fn add_collection(template: &str, names: &[&str], error: Option<&str>) -> Action {
    AddCollection {
        sdl: sdl(template, names),
        expected_error: error.map(Into::into),
        ..Default::default()
    }
    .into()
}
