//! GraphQL's NoFragmentCycles rule: a document whose fragments spread each
//! other in a cycle is invalid, whether or not a field spreads into the cycle.
//! The message matches graphql-js and graphql-go.

use graphql_parser::query::{Definition, FragmentDefinition, Selection, SelectionSet};
use rapidhash::{HashMapExt, HashSetExt, RapidHashMap, RapidHashSet};

use crate::error::{QueryError, Result};

type Fragments<'d, 'a> = RapidHashMap<&'d str, &'d FragmentDefinition<'a, String>>;

/// Rejects the first fragment cycle in `definitions`, in document order.
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
    let mut walk = Walk {
        fragments: &fragments,
        visited: RapidHashSet::new(),
        on_path: RapidHashMap::new(),
        path: Vec::new(),
    };
    for fragment in ordered {
        if !walk.visited.contains(fragment.name.as_str()) {
            walk.detect(fragment)?;
        }
    }
    Ok(())
}

struct Walk<'w, 'd, 'a> {
    fragments: &'w Fragments<'d, 'a>,
    visited: RapidHashSet<&'d str>,
    /// Where each fragment on the current path starts in `path`.
    on_path: RapidHashMap<&'d str, usize>,
    /// The spreads followed from the walk's first fragment.
    path: Vec<&'d str>,
}

impl<'d, 'a> Walk<'_, 'd, 'a> {
    fn detect(&mut self, fragment: &'d FragmentDefinition<'a, String>) -> Result<()> {
        let name = fragment.name.as_str();
        self.visited.insert(name);
        let mut spreads = Vec::new();
        collect_spreads(&fragment.selection_set, &mut spreads);
        self.on_path.insert(name, self.path.len());
        for spread in spreads {
            let start = self.on_path.get(spread).copied();
            self.path.push(spread);
            match start {
                Some(start) => {
                    let via = &self.path[start..self.path.len() - 1];
                    let via = match via.is_empty() {
                        true => String::new(),
                        false => format!(" via {}", via.join(", ")),
                    };
                    return Err(QueryError::parse(format!(
                        "Cannot spread fragment \"{spread}\" within itself{via}."
                    )));
                }
                None => {
                    if let Some(next) = self.fragments.get(spread) {
                        if !self.visited.contains(spread) {
                            self.detect(next)?;
                        }
                    }
                }
            }
            self.path.pop();
        }
        self.on_path.remove(name);
        Ok(())
    }
}

fn collect_spreads<'d>(set: &'d SelectionSet<'_, String>, out: &mut Vec<&'d str>) {
    for selection in &set.items {
        match selection {
            Selection::Field(field) => collect_spreads(&field.selection_set, out),
            Selection::FragmentSpread(spread) => out.push(spread.fragment_name.as_str()),
            Selection::InlineFragment(inline) => collect_spreads(&inline.selection_set, out),
        }
    }
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
