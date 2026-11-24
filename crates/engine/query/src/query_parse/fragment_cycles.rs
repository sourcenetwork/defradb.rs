//! GraphQL's NoFragmentCycles rule: a document whose fragments spread each
//! other in a cycle is invalid, whether or not a field spreads into the cycle.
//! The message matches graphql-js and graphql-go.

use graphql_parser::query::{Definition, FragmentDefinition, Selection, SelectionSet};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::error::{QueryError, Result};

type Fragments<'d, 'a> = RapidHashMap<&'d str, &'d FragmentDefinition<'a, String>>;

/// Rejects the first fragment cycle in `definitions`, in document order.
///
/// The walk keeps its own stack, so a long chain of fragments costs heap,
/// not thread stack: a test thread on macOS has a small stack, and a chain
/// thousands of fragments long must be refused by the depth limit later,
/// never by a stack overflow here.
pub(crate) fn check_fragment_cycles<'a>(definitions: &[Definition<'a, String>]) -> Result<()> {
    let ordered: Vec<_> = definitions
        .iter()
        .filter_map(|definition| match definition {
            Definition::Fragment(fragment) => Some(fragment),
            _ => None,
        })
        .collect();
    let fragments: Fragments<'_, 'a> = ordered
        .iter()
        .map(|fragment| (fragment.name.as_str(), *fragment))
        .collect();
    let mut visited: RapidHashSet<&str> = RapidHashSet::new();
    for fragment in ordered {
        if !visited.contains(fragment.name.as_str()) {
            detect(&fragments, &mut visited, fragment)?;
        }
    }
    Ok(())
}

/// A fragment being walked: its name, the spreads it holds, and the next one
/// to follow.
struct Frame<'d> {
    name: &'d str,
    spreads: Vec<&'d str>,
    next: usize,
}

impl<'d> Frame<'d> {
    fn new(fragment: &'d FragmentDefinition<'_, String>) -> Self {
        Self {
            name: fragment.name.as_str(),
            spreads: spreads_of(&fragment.selection_set),
            next: 0,
        }
    }
}

/// graphql-js's `detectCycleRecursive`, with an explicit stack of frames in
/// place of recursion. `path` holds the spreads followed from `root`, and
/// `on_path` where each fragment on the current path starts in it.
fn detect<'d>(
    fragments: &Fragments<'d, '_>,
    visited: &mut RapidHashSet<&'d str>,
    root: &'d FragmentDefinition<'_, String>,
) -> Result<()> {
    let mut path: Vec<&str> = Vec::new();
    let mut on_path: RapidHashMap<&str, usize> = RapidHashMap::new();
    visited.insert(root.name.as_str());
    on_path.insert(root.name.as_str(), 0);
    let mut stack = vec![Frame::new(root)];
    while let Some(frame) = stack.last_mut() {
        let Some(&spread) = frame.spreads.get(frame.next) else {
            // Every spread of this fragment is done: it leaves the path, and
            // so does the spread that led to it.
            on_path.remove(frame.name);
            stack.pop();
            if !stack.is_empty() {
                path.pop();
            }
            continue;
        };
        frame.next += 1;
        let start = on_path.get(spread).copied();
        path.push(spread);
        if let Some(start) = start {
            let via = &path[start..path.len() - 1];
            let via = match via.is_empty() {
                true => String::new(),
                false => format!(" via {}", via.join(", ")),
            };
            return Err(QueryError::parse(format!(
                "Cannot spread fragment \"{spread}\" within itself{via}."
            )));
        }
        match fragments.get(spread) {
            Some(next) if !visited.contains(spread) => {
                visited.insert(spread);
                on_path.insert(spread, path.len());
                stack.push(Frame::new(next));
            }
            _ => {
                path.pop();
            }
        }
    }
    Ok(())
}

/// The fragment spreads in `set`, in document order, nested ones included,
/// read without recursion.
fn spreads_of<'d>(set: &'d SelectionSet<'_, String>) -> Vec<&'d str> {
    let mut spreads = Vec::new();
    let mut stack = vec![set.items.iter()];
    while let Some(items) = stack.last_mut() {
        match items.next() {
            None => {
                stack.pop();
            }
            Some(Selection::Field(field)) => stack.push(field.selection_set.items.iter()),
            Some(Selection::FragmentSpread(spread)) => spreads.push(spread.fragment_name.as_str()),
            Some(Selection::InlineFragment(inline)) => {
                stack.push(inline.selection_set.items.iter())
            }
        }
    }
    spreads
}

#[cfg(test)]
mod tests {
    use super::*;

    fn check(query: &str) -> std::result::Result<(), String> {
        let doc = graphql_parser::parse_query::<String>(query).unwrap();
        check_fragment_cycles(&doc.definitions).map_err(|e| e.to_string())
    }

    #[test]
    fn names_the_cycle_like_graphql_js() {
        let two =
            check("query { U { ...A } } fragment A on U { n ...B } fragment B on U { n ...A }");
        assert!(two
            .unwrap_err()
            .contains(r#"Cannot spread fragment "A" within itself via B."#));
        let three = check(
            "fragment A on U { ...B } fragment B on U { ...C } fragment C on U { x { ...A } }",
        );
        assert!(three
            .unwrap_err()
            .contains(r#"Cannot spread fragment "A" within itself via B, C."#));
        let own = check("fragment A on U { n ...A }");
        assert!(own
            .unwrap_err()
            .contains(r#"Cannot spread fragment "A" within itself."#));
    }

    #[test]
    fn shared_fragments_without_a_cycle_pass() {
        check("query { U { ...A ...B } } fragment A on U { ...C } fragment B on U { ...C } fragment C on U { n }")
            .unwrap();
        check("query { U { ...Missing } }").unwrap();
    }
}
