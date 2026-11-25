pub trait Fixed {
    fn fail() -> Box<dyn std::error::Error>;
}
