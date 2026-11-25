#![crate_name = "anyhow"]

pub const VALUE: u8 = 1;

#[macro_export]
macro_rules! unit {
    () => { () };
}
