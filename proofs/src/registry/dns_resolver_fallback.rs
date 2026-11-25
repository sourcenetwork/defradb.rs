use super::{Axis::*, Property, Tier::*};

/// The crates this family's models cover.
pub(super) const CRATES: &str = "p2p";

pub(super) const PROPERTIES: &[Property] = &[
    Property {
        // #2924: a host resolver configuration is read the way Go reads it
        // (rejected lines and words skipped, the rest kept); one left with no
        // nameservers, or that cannot be read, gets Go's local nameservers
        // instead of failing the node's start.
        family: "DNS resolver fallback",
        name: "DnsResolverFallback.chosen_has_nameservers / usable_read_passes_through / accepted_lines_survive / kept_lines_accepted / rejected_nameserver_dropped / failed_read_falls_back",
        axis: Lean,
        anchor: "crates/database/p2p/src/host/transport/resolver.rs read_file, parse_lenient, usable_line, accepted, resolver_config, fallback_config, FALLBACK_NAMESERVERS",
        model_ref: "DnsResolverFallback (lake build)",
        tiers: &[Contract],
        bound_by: "crates/database/p2p/src/host/transport/resolver_tests.rs a_failed_read_falls_back_to_the_localhost_nameservers, a_read_with_nameservers_passes_through_unchanged, a_systemd_resolved_stub_passes_through, an_unknown_option_keeps_the_hosts_nameserver, a_bad_nameserver_line_keeps_the_good_ones, a_bad_nameserver_line_never_uses_its_comment, an_unknown_directive_keeps_the_hosts_nameserver, a_bad_search_domain_keeps_the_others, a_file_without_nameservers_keeps_its_search_and_options, a_comment_that_is_not_utf8_keeps_the_hosts_nameserver, a_missing_file_falls_back_to_the_localhost_nameservers, the_fallback_matches_the_lean_model",
    },
];
