//! Coverage matrix over the registry.
//!
//! The "diff" the harness defends: the modeled families are the families of
//! `registry::PROPERTIES`, and `proofs/README.md`'s *Modeled families* table
//! lists exactly those. A model that lands without a conformance binding, or a
//! binding without its README row, fails `readme_lists_exactly_the_modeled_families`.
//! A Contract or Behavioral tier with no test behind it fails
//! `every_bound_tier_names_its_tests`.

use crate::registry::{Property, Tier, PROPERTIES};

/// The modeled families: each distinct `family` in `PROPERTIES`, in first-seen order.
pub fn families() -> Vec<&'static str> {
    let mut families: Vec<&'static str> = Vec::new();
    for property in PROPERTIES {
        if !families.contains(&property.family) {
            families.push(property.family);
        }
    }
    families
}

pub fn properties_for(family: &'static str) -> impl Iterator<Item = &'static Property> {
    PROPERTIES.iter().filter(move |p| p.family == family)
}

pub fn count(tier: Tier) -> usize {
    PROPERTIES
        .iter()
        .filter(|p| p.tiers.contains(&tier))
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_property_has_a_tier() {
        let untiered: Vec<_> = PROPERTIES.iter().filter(|p| p.tiers.is_empty()).collect();
        assert!(
            untiered.is_empty(),
            "properties with no tier: {:?}",
            untiered.iter().map(|p| p.name).collect::<Vec<_>>()
        );
    }

    #[test]
    fn every_bound_tier_names_its_tests() {
        let root = crate::anchor::repository_root();
        let unbound: Vec<String> = PROPERTIES
            .iter()
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
    fn readme_lists_exactly_the_modeled_families() {
        let section = include_str!("../README.md")
            .split("### Modeled families")
            .nth(1)
            .and_then(|rest| rest.split("\n###").next())
            .expect("README has a `### Modeled families` section");
        let rows: Vec<&str> = section
            .lines()
            .filter_map(|line| line.strip_prefix("| "))
            .filter_map(|line| line.split(" |").next())
            .filter(|&family| family != "Family")
            .collect();
        let families = families();
        let missing: Vec<_> = families
            .iter()
            .filter(|&family| !rows.contains(family))
            .collect();
        let extra: Vec<_> = rows
            .iter()
            .filter(|&family| !families.contains(family))
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "README Modeled families table: registry families with no row {missing:?}, \
             rows with no registry entry {extra:?}"
        );
    }
}
