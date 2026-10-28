//! Coverage matrix over the registry.
//!
//! The "diff" the harness defends: every family the README claims is *Modeled*
//! must appear in `registry::PROPERTIES` with at least one binding tier. A model
//! that lands without a conformance hook fails `every_modeled_family_is_bound`.

use crate::registry::{Property, Tier, PROPERTIES};

/// The Modeled families from `proofs/README.md`. Kept here so the assertion is
/// self-contained; if the README grows a family, this list and `PROPERTIES`
/// must both grow or the tests fail, and the README must list exactly these.
pub const MODELED_FAMILIES: &[&str] = &[
    "HTTP router composition",
    "Storage failure classification",
    "B3 filtered replication",
    "DAG convergence (partition / eviction / restart)",
    "CRDT merge laws",
    "Replicator lifecycle (no-loss / resume)",
    "Sync ownership transfer (head hint / receiver pull)",
    "Multi-instance claim",
    "Block integrity / signatures",
    "KMS key distribution",
    "Encrypted LWW restart/replay",
    "Management-channel auth (NAC gate)",
    "ACP soundness + revocation + dual-path commits",
    "Storage transaction validation",
    "P2P explicit-replay capability gate",
    "NAC lifecycle privilege-escalation",
    "Transaction & merge-queue concurrency",
    "Document materialization status convergence",
    "JWT issuer / algorithm binding",
    "CID content-addressing determinism + Block canonicalization",
    "Deferred-ACP overlay consistency",
    "Index-maintenance consistency",
    "Order-preserving key encoding",
    "Concurrent collection-head transitions",
    "Vector index link maintenance",
    "DiskANN resident code table",
    "DiskANN walk cutoff",
    "PQ distance four sums",
    "DiskANN pending cache",
    "DiskANN shared quantizer",
    "RobustPrune blocked covered checks",
    "DiskANN back-edges in place",
    "Commit-ordered document arrivals",
    "Retry input and marker fencing",
    "Collection schema-version OCC",
    "Schema publication completion",
    "Peer connection observation",
    "Explicit transaction statement atomicity",
    "Engine ticker export",
    "Worker pool CPU ledger",
    "Worker default pool",
    "Node shutdown order",
    "Inbound stream admission",
    "Peer fetch stats",
    "Gossip connection ownership",
    "Iroh service records",
    "Iroh solicited replies",
    "Reply-driven DAG fetch",
    "Embedded node startup",
    "Text CRDT convergence",
    "JSON CRDT per-path merge",
    "Custom CRDT types",
    "Document create pipeline",
];

pub fn properties_for(family: &'static str) -> impl Iterator<Item = &'static Property> {
    PROPERTIES.iter().filter(move |p| p.family == family)
}

pub fn count(tier: Tier) -> usize {
    PROPERTIES
        .iter()
        .filter(|p| p.tiers.contains(&tier))
        .count()
}

/// A `family -> bound?` summary line for each modeled family.
pub fn summary() -> Vec<(&'static str, bool)> {
    MODELED_FAMILIES
        .iter()
        .map(|&fam| (fam, properties_for(fam).next().is_some()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_modeled_family_is_bound() {
        let unbound: Vec<_> = MODELED_FAMILIES
            .iter()
            .filter(|&&fam| properties_for(fam).next().is_none())
            .collect();
        assert!(
            unbound.is_empty(),
            "modeled families with no conformance binding in registry::PROPERTIES: {unbound:?}"
        );
    }

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
    fn readme_lists_exactly_the_modeled_families() {
        let section = include_str!("../README.md")
            .split("### Modeled: ")
            .nth(1)
            .and_then(|rest| rest.split("\n###").next())
            .expect("README has a Modeled section");
        let claimed: usize = section
            .split(' ')
            .next()
            .and_then(|n| n.parse().ok())
            .expect("the Modeled heading starts with a family count");
        let rows: Vec<&str> = section
            .lines()
            .filter_map(|line| line.strip_prefix("| "))
            .filter_map(|line| line.split(" |").next())
            .filter(|&family| family != "Family")
            .collect();
        let missing: Vec<_> = MODELED_FAMILIES
            .iter()
            .filter(|&&family| !rows.contains(&family))
            .collect();
        let extra: Vec<_> = rows
            .iter()
            .filter(|&&family| !MODELED_FAMILIES.contains(&family))
            .collect();
        assert!(
            missing.is_empty() && extra.is_empty(),
            "README Modeled table: missing {missing:?}, not in MODELED_FAMILIES {extra:?}"
        );
        assert_eq!(
            claimed,
            MODELED_FAMILIES.len(),
            "README Modeled heading count"
        );
    }

    #[test]
    fn registry_covers_every_modeled_family() {
        // No stray registry family that isn't a declared modeled family.
        let stray: Vec<_> = PROPERTIES
            .iter()
            .filter(|p| !MODELED_FAMILIES.contains(&p.family))
            .map(|p| p.family)
            .collect();
        assert!(
            stray.is_empty(),
            "registry families not in MODELED_FAMILIES: {stray:?}"
        );
    }
}
