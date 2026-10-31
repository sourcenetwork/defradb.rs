#[derive(Debug, thiserror::Error)]
pub enum ScenarioError {
    #[error("invalid expectation matcher: {0}")]
    Matcher(String),
    #[error("{0}")]
    Path(String),
    #[error("{0}")]
    Resolve(String),
    #[error("relational expectation exceeded 1000000 matching visits; add discriminating literal fields or explicit ordering")]
    Exhausted,
}
