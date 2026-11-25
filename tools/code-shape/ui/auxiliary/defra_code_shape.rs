#[derive(Debug)]
pub enum Error {
    Missing,
}

pub type Result<Value> = std::result::Result<Value, Error>;
pub type Other<Value> = std::result::Result<Value, Error>;
