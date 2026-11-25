#![allow(dead_code)]

#[derive(Debug, thiserror::Error)]
enum Error {
    #[error("could not read the input: {0}")]
    Io(#[from] std::io::Error),
}

fn main() {}
