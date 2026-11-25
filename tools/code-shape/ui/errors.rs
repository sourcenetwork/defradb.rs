// aux-build: untyped.rs
#![allow(dead_code)]

extern crate anyhow as renamed;

use std::error::Error as RenamedError;
use std::fmt;

#[derive(Debug)]
struct Handwritten;

impl fmt::Display for Handwritten {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a typed failure")
    }
}

impl RenamedError for Handwritten {}

mod anyhow {
    pub const VALUE: u8 = 1;
}

fn main() {
    let _ = renamed::unit!();
    let _ = anyhow::VALUE;
}
