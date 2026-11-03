use std::cell::{Cell, RefCell};

use chrono::Utc;
use serde_json::{json, Map, Value};

use super::branchable::branchable_create_graph;
use super::explain::{match_explain_graph, match_explain_scan};
use super::scalar::{
    match_current_timestamp, match_length, match_nonempty, match_similarity, not_plain_cbor_string,
    valid_doc_id,
};
use super::{contains_matcher, Bindings, Options};
use crate::scenario::number::json_eq;
use crate::scenario::ScenarioError;

const MAX_VISITS: u64 = 1_000_000;

type Next<'n> = &'n dyn Fn(Bindings) -> bool;

pub fn match_relational(
    expected: &Value,
    actual: &Value,
    options: Options<'_>,
    bindings: &mut Bindings,
) -> Result<bool, ScenarioError> {
    let engine = Engine {
        options,
        visits: Cell::new(0),
        exhausted: Cell::new(false),
    };
    let accepted: RefCell<Option<Bindings>> = RefCell::new(None);
    let record = |state: Bindings| {
        *accepted.borrow_mut() = Some(state);
        true
    };
    let matched = engine.visit(
        expected,
        actual,
        "",
        false,
        false,
        bindings.clone(),
        &record,
    );
    if engine.exhausted.get() {
        return Err(ScenarioError::Exhausted);
    }
    if matched {
        if let Some(state) = accepted.into_inner() {
            *bindings = state;
        }
    }
    Ok(matched)
}

struct Engine<'a> {
    options: Options<'a>,
    visits: Cell<u64>,
    exhausted: Cell<bool>,
}

fn escape(key: &str) -> String {
    key.replace('~', "~0").replace('/', "~1")
}

impl Engine<'_> {
    #[allow(clippy::too_many_arguments)]
    fn visit(
        &self,
        want: &Value,
        got: &Value,
        path: &str,
        mut documents: bool,
        force_unordered: bool,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        self.visits.set(self.visits.get() + 1);
        if self.visits.get() > MAX_VISITS {
            self.exhausted.set(true);
            return false;
        }
        if self
            .options
            .unordered_documents
            .iter()
            .any(|root| root == path)
        {
            documents = true;
        }
        match want {
            Value::Object(w) => match w.get("$match").and_then(Value::as_str) {
                Some(kind) => self.visit_matcher(
                    kind,
                    w,
                    got,
                    path,
                    documents,
                    force_unordered,
                    current,
                    next,
                ),
                None => self.exact_object(w, got, path, documents, current, next),
            },
            Value::Array(w) => {
                self.visit_array(w, got, path, documents, force_unordered, current, next)
            }
            _ => json_eq(want, got) && next(current),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_matcher(
        &self,
        kind: &str,
        w: &Map<String, Value>,
        got: &Value,
        path: &str,
        documents: bool,
        force_unordered: bool,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        let string_of = |key: &str| w.get(key).and_then(Value::as_str).unwrap_or_default();
        match kind {
            "optional" => self.visit(
                &w["value"],
                got,
                path,
                documents,
                force_unordered,
                current,
                next,
            ),
            "anyOf" => w["of"].as_array().is_some_and(|alternatives| {
                alternatives.iter().any(|alternative| {
                    self.visit(
                        alternative,
                        got,
                        path,
                        documents,
                        force_unordered,
                        current.clone(),
                        next,
                    )
                })
            }),
            "explainPattern" | "explainTarget" => match_explain_graph(w, got) && next(current),
            "similarityScore" => match_similarity(w, got) && next(current),
            "currentTimestamp" => match_current_timestamp(got, Utc::now()) && next(current),
            "nonemptyString" => got.as_str().is_some_and(|s| !s.is_empty()) && next(current),
            "nonempty" => match_nonempty(got) && next(current),
            "length" => match_length(&w["value"], got) && next(current),
            "substring" => {
                got.as_str().is_some_and(|s| s.contains(string_of("value"))) && next(current)
            }
            "explainScan" => match_explain_scan(w, got) && next(current),
            "introspectionTypes" => introspection_types(got) && next(current),
            "sortedStrings" => sorted_strings(string_of("field"), got) && next(current),
            "docID" => valid_doc_id(got) && next(current),
            "cid" => got.as_str().is_some_and(|s| cid::Cid::try_from(s).is_ok()) && next(current),
            "notPlainCBORString" => {
                w.get("value")
                    .and_then(Value::as_str)
                    .is_some_and(|value| not_plain_cbor_string(got, value))
                    && next(current)
            }
            "firstWhere" => self.first_where(w, got, path, current, next),
            "object" => self.object_subset(w, got, path, documents, current, next),
            "contains" => {
                let Some(values) = got.as_array() else {
                    return false;
                };
                let items = w["items"].as_array().map_or(&[][..], Vec::as_slice);
                items
                    .iter()
                    .all(|item| values.iter().any(|value| json_eq(item, value)))
                    && next(current)
            }
            "unordered" => self.visit(&w["items"], got, path, documents, true, current, next),
            "branchableCreateGraph" => match branchable_create_graph(got) {
                Some(graph) => {
                    let same = json!({"$match": "same", "name": w["name"]});
                    self.visit(
                        &same,
                        &graph,
                        path,
                        documents,
                        force_unordered,
                        current,
                        next,
                    )
                }
                None => false,
            },
            "same" => {
                let name = string_of("name");
                if let Some(old) = current.same.get(name) {
                    return json_eq(old, got) && next(current);
                }
                let mut candidate = current;
                candidate.same.insert(name.to_owned(), got.clone());
                next(candidate)
            }
            "unique" => {
                let name = string_of("name");
                let seen = current.unique.get(name).map_or(&[][..], Vec::as_slice);
                if seen.iter().any(|old| json_eq(old, got)) {
                    return false;
                }
                let mut candidate = current.clone();
                candidate
                    .unique
                    .entry(name.to_owned())
                    .or_default()
                    .push(got.clone());
                next(candidate)
            }
            "all" => {
                let clauses = w["of"].as_array().map_or(&[][..], Vec::as_slice);
                self.all(clauses, 0, got, path, documents, current, next)
            }
            _ => self.exact_object(w, got, path, documents, current, next),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn all(
        &self,
        clauses: &[Value],
        i: usize,
        got: &Value,
        path: &str,
        documents: bool,
        state: Bindings,
        next: Next<'_>,
    ) -> bool {
        if i == clauses.len() {
            return next(state);
        }
        let rest = |n: Bindings| self.all(clauses, i + 1, got, path, documents, n, next);
        self.visit(&clauses[i], got, path, documents, false, state, &rest)
    }

    fn first_where(
        &self,
        w: &Map<String, Value>,
        got: &Value,
        path: &str,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        let Some(rows) = got.as_array() else {
            return false;
        };
        if !rows.iter().all(Value::is_object) {
            return false;
        }
        let field = w["field"].as_str().unwrap_or_default();
        for (i, row) in rows.iter().enumerate() {
            if row
                .get(field)
                .is_some_and(|value| json_eq(value, &w["value"]))
            {
                return self.visit(
                    &w["expect"],
                    row,
                    &format!("{path}/{i}"),
                    false,
                    false,
                    current,
                    next,
                );
            }
        }
        false
    }

    fn object_subset(
        &self,
        w: &Map<String, Value>,
        got: &Value,
        path: &str,
        documents: bool,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        let (Some(actual), Some(fields)) = (got.as_object(), w["fields"].as_object()) else {
            return false;
        };
        let mut selected = Map::new();
        for key in fields.keys() {
            match actual.get(key) {
                Some(value) => {
                    selected.insert(key.clone(), value.clone());
                }
                None if is_optional(&fields[key]) => {}
                None => return false,
            }
        }
        self.exact_object(
            fields,
            &Value::Object(selected),
            path,
            documents,
            current,
            next,
        )
    }

    fn exact_object(
        &self,
        w: &Map<String, Value>,
        got: &Value,
        path: &str,
        documents: bool,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        let Some(g) = got.as_object() else {
            return false;
        };
        if g.keys().any(|key| !w.contains_key(key)) {
            return false;
        }
        let mut keys: Vec<&String> = Vec::new();
        for (key, expected) in w {
            if !g.contains_key(key) {
                if is_optional(expected) {
                    continue;
                }
                return false;
            }
            keys.push(key);
        }
        keys.sort_by(|a, b| {
            contains_matcher(&w[*a])
                .cmp(&contains_matcher(&w[*b]))
                .then_with(|| a.cmp(b))
        });
        self.each_key(w, g, &keys, 0, path, documents, current, next)
    }

    #[allow(clippy::too_many_arguments)]
    fn each_key(
        &self,
        w: &Map<String, Value>,
        g: &Map<String, Value>,
        keys: &[&String],
        i: usize,
        path: &str,
        documents: bool,
        state: Bindings,
        next: Next<'_>,
    ) -> bool {
        if i == keys.len() {
            return next(state);
        }
        let key = keys[i];
        let child_path = format!("{path}/{}", escape(key));
        let rest = |n: Bindings| self.each_key(w, g, keys, i + 1, path, documents, n, next);
        self.visit(
            &w[key],
            &g[key],
            &child_path,
            documents,
            false,
            state,
            &rest,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn visit_array(
        &self,
        w: &[Value],
        got: &Value,
        path: &str,
        documents: bool,
        force_unordered: bool,
        current: Bindings,
        next: Next<'_>,
    ) -> bool {
        let Some(g) = got.as_array() else {
            return false;
        };
        if w.len() != g.len() {
            return false;
        }
        let mut unordered =
            force_unordered || self.options.unordered.iter().any(|root| root == path);
        if documents {
            let all_objects = w.iter().chain(g).all(Value::is_object);
            unordered = unordered || all_objects;
        }
        let used = RefCell::new(vec![false; g.len()]);
        self.each_item(w, g, 0, path, documents, unordered, &used, current, next)
    }

    #[allow(clippy::too_many_arguments)]
    fn each_item(
        &self,
        w: &[Value],
        g: &[Value],
        i: usize,
        path: &str,
        documents: bool,
        unordered: bool,
        used: &RefCell<Vec<bool>>,
        state: Bindings,
        next: Next<'_>,
    ) -> bool {
        if i == w.len() {
            return next(state);
        }
        let child_path = format!("{path}/{i}");
        let rest =
            |n: Bindings| self.each_item(w, g, i + 1, path, documents, unordered, used, n, next);
        if !unordered {
            return self.visit(&w[i], &g[i], &child_path, documents, false, state, &rest);
        }
        for (j, candidate) in g.iter().enumerate() {
            if used.borrow()[j] || self.exhausted.get() {
                continue;
            }
            used.borrow_mut()[j] = true;
            let matched = self.visit(
                &w[i],
                candidate,
                &child_path,
                documents,
                false,
                state.clone(),
                &rest,
            );
            used.borrow_mut()[j] = false;
            if matched {
                return true;
            }
        }
        false
    }
}

fn is_optional(value: &Value) -> bool {
    value.get("$match").and_then(Value::as_str) == Some("optional")
}

fn introspection_types(got: &Value) -> bool {
    let Some(types) = got.as_array() else {
        return false;
    };
    types.iter().all(|item| {
        let Some(kind) = item.get("kind").and_then(Value::as_str) else {
            return false;
        };
        let non_null = |key: &str| item.get(key).is_some_and(|v| !v.is_null());
        !(kind == "OBJECT" && !non_null("fields"))
            && !(kind == "INPUT_OBJECT" && !non_null("inputFields"))
    })
}

fn sorted_strings(field: &str, got: &Value) -> bool {
    let Some(rows) = got.as_array() else {
        return false;
    };
    let mut previous: Option<&str> = None;
    for row in rows {
        let Some(value) = row.get(field).and_then(Value::as_str) else {
            return false;
        };
        if previous.is_some_and(|p| value < p) {
            return false;
        }
        previous = Some(value);
    }
    true
}
