//! A long chain of fragments in a query is refused with an error, never by
//! a stack overflow, on a thread with a stack as small as macOS gives test
//! threads (#2465).

use query::query_parse::MAX_FRAGMENT_NESTING;

const SMALL_STACK: usize = 512 * 1024;

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

/// What parsing `document` says, run on a thread with a 512 KiB stack.
fn parse_on_small_stack(document: String) -> Result<(), String> {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(move || {
            query::parse_query(&document)
                .map(|_| ())
                .map_err(|e| e.to_string())
        })
        .unwrap()
        .join()
        .expect("parsing must not overflow a 512 KiB stack")
}

#[test]
fn a_long_fragment_chain_is_refused_on_a_small_stack() {
    let error = parse_on_small_stack(chain(2_000)).unwrap_err();
    assert!(error.contains("nests fragments more than"), "{error}");
}

#[test]
fn a_chain_at_the_limit_still_parses() {
    parse_on_small_stack(chain(MAX_FRAGMENT_NESTING)).unwrap();
    let error = parse_on_small_stack(chain(MAX_FRAGMENT_NESTING + 1)).unwrap_err();
    assert!(error.contains("nests fragments more than"), "{error}");
}
