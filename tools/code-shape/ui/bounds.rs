// aux-build: codegen.rs
#![allow(dead_code, type_alias_bounds)]

extern crate codegen;
codegen::external_declaration!();

use std::fmt::Debug;

fn function<Value: Debug>(_: Value) {}
struct Record<Value: Debug>(Value);
enum Choice<Value: Debug> { Value(Value) }
trait Source<Value: Debug> {}
type Values<Value: Debug> = Vec<Value>;
impl<Value: Debug> Record<Value> {}

struct Compliant<Value>(Value)
where
    Value: Debug;

trait Parent: Debug {}
fn lifetimes<'left: 'right, 'right>(_: &'left (), _: &'right ()) {}
fn consts<const SIZE: usize>(_: [u8; SIZE]) {}
fn scoped<Value>(_: Value)
where
    Value: Debug,
{}

macro_rules! inline {
    ($value:ident: $bound:path) => {
        fn expanded<$value: $bound>(_: $value) {}
    };
}
inline!(Element: Debug);

macro_rules! scoped {
    ($value:ident: $bound:path) => {
        fn expanded_scoped<$value>(_: $value)
        where
            $value: $bound,
        {}
    };
}
scoped!(Element: Debug);

macro_rules! authored {
    () => {
        struct Authored<Value: Debug>(Value);
    };
}
authored!();

fn main() {}
