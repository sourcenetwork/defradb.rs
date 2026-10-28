use workers::Error;

const SHUTDOWN: &str = "the node is shutting down; try again";
const INTERNAL: &str = "the request failed on an internal error; see the node's log";

#[test]
fn shut_down_is_told_as_a_retry() {
    assert_eq!(Error::ShutDown.report("x"), SHUTDOWN);
}

#[test]
fn a_panic_is_not_told_as_a_shutdown() {
    let told = Error::Panicked {
        task: "t",
        message: "m".into(),
    }
    .report("x");
    assert_eq!(told, INTERNAL);
    assert!(!told.contains("shutting down"));
}

#[test]
fn any_other_failure_is_an_internal_error() {
    let error = Error::NoSuchWorker {
        index: 9,
        workers: 1,
    };
    assert_eq!(error.report("x"), INTERNAL);
}
