use super::*;

#[test]
fn signed_docs_in_handles_all_cases() {
    assert!(!signed_docs_in(None), "unset → false");
    assert!(!signed_docs_in(Some("")), "empty → false");
    assert!(signed_docs_in(Some("signed-docs")), "exact → true");
    assert!(signed_docs_in(Some(" signed-docs ")), "padded → true");
    assert!(
        signed_docs_in(Some("signed-docs,foo")),
        "first of list → true"
    );
    assert!(
        signed_docs_in(Some("foo,signed-docs")),
        "second of list → true"
    );
    assert!(
        signed_docs_in(Some("foo, signed-docs ,bar")),
        "padded middle → true"
    );
    assert!(!signed_docs_in(Some("foo")), "other only → false");
    assert!(!signed_docs_in(Some("foo,bar")), "no match → false");
    assert!(
        !signed_docs_in(Some("SIGNED-DOCS")),
        "case-sensitive → false"
    );
}

#[test]
fn node_extra_rust_args_follow_the_cluster_wide_ones_for_that_node_only() {
    let b = TestClusterBuilder::new()
        .with_extra_rust_args(["--a", "1"])
        .with_node_extra_rust_args(2, ["--b", "2"]);
    assert_eq!(b.rust_args_for(0), vec!["--a", "1"]);
    assert_eq!(b.rust_args_for(2), vec!["--a", "1", "--b", "2"]);
    assert_eq!(b.rust_args_for(3), vec!["--a", "1"]);
}

#[test]
fn no_signing_multiplier_sets_opt_out_flag() {
    let b = TestClusterBuilder::new();
    assert!(!b.signing_multiplier_opt_out, "default is opt-in");

    let b = TestClusterBuilder::new().no_signing_multiplier();
    assert!(b.signing_multiplier_opt_out, "after call, opt-out is true");
}

#[test]
fn signed_docs_multiplier_active_reads_env() {
    // Drop-on-scope-exit guard: restores DEFRA_MULTIPLIERS to its
    // prior value even if a later assertion panics. Without this,
    // a failing assertion would leak the mutated env into sibling
    // tests on the same binary.
    struct RestoreEnv(Option<String>);
    impl Drop for RestoreEnv {
        fn drop(&mut self) {
            match &self.0 {
                Some(v) => unsafe { std::env::set_var("DEFRA_MULTIPLIERS", v) },
                None => unsafe { std::env::remove_var("DEFRA_MULTIPLIERS") },
            }
        }
    }

    let _restore = RestoreEnv(std::env::var("DEFRA_MULTIPLIERS").ok());

    // Safety: this is the only test in the binary that writes the
    // environment, and no other test reads DEFRA_MULTIPLIERS.
    unsafe {
        std::env::remove_var("DEFRA_MULTIPLIERS");
    }
    assert!(!signed_docs_multiplier_active(), "unset → false");

    unsafe {
        std::env::set_var("DEFRA_MULTIPLIERS", "signed-docs");
    }
    assert!(signed_docs_multiplier_active(), "exact → true");

    unsafe {
        std::env::set_var("DEFRA_MULTIPLIERS", "foo,signed-docs,bar");
    }
    assert!(signed_docs_multiplier_active(), "in list → true");

    unsafe {
        std::env::set_var("DEFRA_MULTIPLIERS", "foo");
    }
    assert!(!signed_docs_multiplier_active(), "other only → false");
}
