//! Coverage matrix over the registry.
//!
//! The "diff" the harness defends: the modeled families are the families of
//! `registry::properties`, and [`families_table`] prints them with their
//! tools and crates on demand (`cargo run -p conformance --example
//! families`), instead of a hand-maintained README table. A family that
//! lands without naming its crates fails
//! `every_family_names_its_crates`; a Contract or Behavioral tier with no
//! test behind it fails `every_bound_tier_names_its_tests`. A `.cfg` that is
//! deleted or renamed while a registry `model_ref` still names it fails
//! `every_registry_cfg_exists` (run-all.sh discovers its configs, so a stale
//! name would otherwise stop failing anything); the cfgs no registry entry
//! names are the remaining gap, tracked by follow-up #3331.
//!
//! The checks read the tree, not the built binary, so no test here runs TLC.

use crate::registry::{properties, Axis, Property, Tier};

/// The modeled families: each distinct `family` in the registry, in first-seen
/// order.
pub fn families() -> Vec<&'static str> {
    let mut families: Vec<&'static str> = Vec::new();
    for property in properties() {
        if !families.contains(&property.family) {
            families.push(property.family);
        }
    }
    families
}

pub fn properties_for(family: &'static str) -> impl Iterator<Item = &'static Property> {
    properties().filter(move |p| p.family == family)
}

pub fn count(tier: Tier) -> usize {
    properties().filter(|p| p.tiers.contains(&tier)).count()
}

/// The *Modeled families* table: one row per registry family, in file-name
/// order. The Tool column is generated, not hand-written: a family that has
/// properties on both axes reads `TLA+ / Lean`, one axis reads that tool.
pub fn families_table() -> String {
    let mut table = String::from("| Family | Tool | Crates it covers |\n|---|---|---|\n");
    for family in crate::registry::FAMILIES {
        let tool = match (
            family.properties.iter().any(|p| p.axis == Axis::Tla),
            family.properties.iter().any(|p| p.axis == Axis::Lean),
        ) {
            (true, true) => "TLA+ / Lean",
            (true, false) => "TLA+",
            (false, true) => "Lean",
            (false, false) => unreachable!("every property has an axis"),
        };
        table.push_str(&format!(
            "| {} | {tool} | {} |\n",
            family.properties[0].family, family.crates
        ));
    }
    table
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_has_a_tier() {
        let untiered: Vec<_> = properties().filter(|p| p.tiers.is_empty()).collect();
        assert!(
            untiered.is_empty(),
            "properties with no tier: {:?}",
            untiered.iter().map(|p| p.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_bound_tier_names_its_tests() {
        let root = crate::anchor::repository_root();
        let unbound: Vec<String> = properties()
            .filter(|p| p.tiers.iter().any(|&t| t != Tier::Boundary) || !p.bound_by.is_empty())
            .filter_map(|p| {
                let problems = crate::anchor::problems(&root, p.bound_by);
                if p.bound_by.is_empty() {
                    Some(format!("{}: no bound_by", p.name))
                } else if !problems.is_empty() {
                    Some(format!("{}: {}", p.name, problems.join("; ")))
                } else {
                    None
                }
            })
            .collect();
        assert!(
            unbound.is_empty(),
            "Contract or Behavioral properties whose tests are missing:\n{}",
            unbound.join("\n")
        );
    }

    #[test]
    fn every_family_file_holds_one_family() {
        for family in crate::registry::FAMILIES {
            let first = family
                .properties
                .first()
                .expect("a family file with no property");
            assert!(
                family.properties.iter().all(|p| p.family == first.family),
                "{} shares a file",
                first.family
            );
        }
        let names: Vec<_> = crate::registry::FAMILIES
            .iter()
            .map(|family| family.properties[0].family)
            .collect();
        assert_eq!(
            names.len(),
            families().len(),
            "a family is split over two files"
        );
    }

    #[test]
    fn every_registry_cfg_exists() {
        let root = crate::anchor::repository_root();
        let cfgs = top_level_cfgs(&root);
        let unmatched: Vec<String> = properties()
            .flat_map(|p| model_cfgs(p.model_ref).map(move |name| (p, name)))
            .filter_map(|(p, name)| {
                (!cfgs.iter().any(|cfg| wildcard_matches(cfg, &name)))
                    .then(|| format!("{}: {}", p.family, name))
            })
            .collect();
        assert!(
            unmatched.is_empty(),
            "registry cfgs that no file in proofs/tla matches:\n{}",
            unmatched.join("\n")
        );
    }

    /// Every `.cfg` name `model_ref` names, with `{a,b,…}` alternatives
    /// expanded and `*` kept as a wildcard for the match.
    fn model_cfgs(model_ref: &str) -> impl Iterator<Item = String> {
        cfg_tokens(model_ref).flat_map(expand_alternatives)
    }

    /// The names matching `[A-Za-z0-9_*{},]+\.cfg` in `model_ref`: maximal
    /// runs of those characters directly followed by the literal `.cfg`.
    fn cfg_tokens(model_ref: &str) -> impl Iterator<Item = String> {
        let chars: Vec<char> = model_ref.chars().collect();
        let in_run =
            |c: char| c.is_ascii_alphanumeric() || matches!(c, '_' | '*' | '{' | '}' | ',');
        let mut tokens = Vec::new();
        let mut run: Option<(usize, usize)> = None;
        for (at, &c) in chars.iter().enumerate() {
            match (run, in_run(c)) {
                (None, true) => run = Some((at, at + 1)),
                (Some((start, _)), true) => run = Some((start, at + 1)),
                (Some((start, end)), false) => {
                    end_with_cfg(&chars, start, end, &mut tokens);
                    run = None;
                }
                (None, false) => {}
            }
        }
        if let Some((start, end)) = run {
            end_with_cfg(&chars, start, end, &mut tokens);
        }
        tokens.into_iter()
    }

    /// The in-class run `chars[start..end]` pushed to `tokens` when the
    /// literal `.cfg` follows it.
    fn end_with_cfg(chars: &[char], start: usize, end: usize, tokens: &mut Vec<String>) {
        if chars[end..].starts_with(&['.', 'c', 'f', 'g']) {
            let mut text: String = chars[start..end].iter().collect();
            text.push_str(".cfg");
            tokens.push(text);
        }
    }

    /// The brace alternatives of `name` expanded; several braces may appear,
    /// and an alternative may itself hold a brace.
    fn expand_alternatives(name: String) -> Vec<String> {
        let Some(open) = name.find('{') else {
            return vec![name];
        };
        let Some(at) = name[open..].find('}').map(|at| open + at) else {
            return vec![name];
        };
        let (prefix, suffix) = (&name[..open], &name[at + 1..]);
        let mut expanded = Vec::new();
        for alternative in name[open + 1..at].split(',') {
            expanded.extend(expand_alternatives(format!(
                "{prefix}{alternative}{suffix}"
            )));
        }
        expanded
    }

    /// Whether `file` matches `name`, whose `*`s each stand for any run of
    /// characters.
    fn wildcard_matches(file: &str, name: &str) -> bool {
        if !name.contains('*') {
            return file == name;
        }
        let literals: Vec<&str> = name.split('*').collect();
        let (first, inner, last) = (
            literals[0],
            &literals[1..literals.len() - 1],
            literals[literals.len() - 1],
        );
        let Some(rest) = file.strip_prefix(first) else {
            return false;
        };
        let mut rest = rest;
        for literal in inner {
            match rest.find(literal) {
                Some(found) => rest = &rest[found + literal.len()..],
                None => return false,
            }
        }
        rest.ends_with(last)
    }

    /// The names of every top-level `*.cfg` in `proofs/tla/`, sorted.
    fn top_level_cfgs(root: &std::path::Path) -> Vec<String> {
        let entries = std::fs::read_dir(root.join("proofs").join("tla"))
            .expect("proofs/tla is a readable directory");
        let mut cfgs: Vec<String> = entries
            .flatten()
            .map(|entry| entry.file_name())
            .filter_map(|name| name.to_str().map(str::to_owned))
            .filter(|name| name.ends_with(".cfg"))
            .collect();
        cfgs.sort();
        cfgs
    }

    #[test]
    fn every_family_names_its_crates() {
        let nameless: Vec<&str> = crate::registry::FAMILIES
            .iter()
            .filter(|family| family.crates.trim().is_empty())
            .map(|family| family.properties[0].family)
            .collect();
        assert!(
            nameless.is_empty(),
            "families whose CRATES is empty or whitespace: {nameless:?}"
        );
    }
}
