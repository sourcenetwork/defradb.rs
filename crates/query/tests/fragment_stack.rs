//! A long chain of fragments in a query is refused with an error, never by
//! a stack overflow, on a thread with a stack as small as macOS gives test
//! threads (#2465). The bound is the node's `max_fragment_nesting`.

use query::query_parse::MAX_FRAGMENT_NESTING;
use query::QueryLimits;

const SMALL_STACK: usize = 512 * 1024;

/// A raised limit costs stack: a 64-deep chain overflows 512 KiB in a debug
/// build, so the raised-limit test runs on the 8 MiB a main thread gets.
const LARGE_STACK: usize = 8 * 1024 * 1024;

/// A query whose fragments spread one another in a chain `count` long.
fn chain(count: usize) -> String {
    let mut document = String::from("query { User { ...F0 } }");
    for i in 0..count {
        if i + 1 == count {
            document.push_str(&format!(" fragment F{i} on User {{ name }}"));
        } else {
            document.push_str(&format!(" fragment F{i} on User {{ ...F{} }}", i + 1));
        }
    }
    document
}

fn nesting(max_fragment_nesting: usize) -> QueryLimits {
    QueryLimits {
        max_fragment_nesting,
        ..QueryLimits::default()
    }
}

/// What parsing `document` under `limits` says, run on a thread with a
/// 512 KiB stack.
fn parse_on_small_stack(document: String, limits: QueryLimits) -> Result<(), String> {
    parse_on_stack(SMALL_STACK, document, limits)
}

/// What parsing `document` under `limits` says, on a `stack`-byte thread.
fn parse_on_stack(stack: usize, document: String, limits: QueryLimits) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(stack)
        .spawn(move || {
            query::parse_query_with_limits(&document, None, limits)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .expect("parsing must not overflow the thread's stack")
}

#[test]
fn a_long_fragment_chain_is_refused_on_a_small_stack() {
    let error = parse_on_small_stack(chain(2_000), QueryLimits::default()).unwrap_err();
    assert!(
        error.contains("nests fragments more than 32 deep"),
        "{error}"
    );
    assert!(error.contains("--query-max-fragment-nesting"), "{error}");
}

#[test]
fn a_chain_at_the_default_limit_still_parses() {
    assert_eq!(MAX_FRAGMENT_NESTING, 32);
    parse_on_small_stack(chain(MAX_FRAGMENT_NESTING), QueryLimits::default()).unwrap();
    let error =
        parse_on_small_stack(chain(MAX_FRAGMENT_NESTING + 1), QueryLimits::default()).unwrap_err();
    assert!(
        error.contains("nests fragments more than 32 deep"),
        "{error}"
    );
}

#[test]
fn a_raised_limit_accepts_a_deeper_chain() {
    parse_on_stack(LARGE_STACK, chain(64), nesting(64)).unwrap();
    let error = parse_on_stack(LARGE_STACK, chain(65), nesting(64)).unwrap_err();
    assert!(
        error.contains("nests fragments more than 64 deep"),
        "{error}"
    );
}

#[test]
fn a_lowered_limit_refuses_a_shorter_chain() {
    parse_on_small_stack(chain(4), nesting(4)).unwrap();
    let error = parse_on_small_stack(chain(5), nesting(4)).unwrap_err();
    assert!(
        error.contains("nests fragments more than 4 deep"),
        "{error}"
    );
}
