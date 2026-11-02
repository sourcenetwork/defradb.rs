//! A long chain of fragments is refused by the depth limit, never by a
//! stack overflow, on a thread with a stack as small as macOS gives test
//! threads. The fragment-cycle check walks fragments without recursion.

const SMALL_STACK: usize = 512 * 1024;

/// A request whose fragments spread one another in a chain `count` long,
/// opening with `operation`.
fn chain(operation: &str, root: &str, count: usize) -> String {
    let mut document = format!("{operation} {{ ...F0 }}");
    for i in 0..count {
        if i + 1 == count {
            document.push_str(&format!(" fragment F{i} on {root} {{ User {{ name }} }}"));
        } else {
            document.push_str(&format!(" fragment F{i} on {root} {{ ...F{} }}", i + 1));
        }
    }
    document
}

/// Run `check` on a thread with a 512 KiB stack and return what it says.
fn on_small_stack<F>(check: F) -> String
where
    F: FnOnce() -> String + Send + 'static,
{
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(check)
        .unwrap()
        .join()
        .expect("the walk must not overflow a 512 KiB stack")
}

#[test]
fn a_long_subscription_fragment_chain_is_refused_on_a_small_stack() {
    let error = on_small_stack(|| {
        query::subscription::validate_subscription(
            &chain("subscription", "Subscription", 2_000),
            None,
        )
        .unwrap_err()
        .to_string()
    });
    assert!(error.contains("nests fragments more than"), "{error}");
}
