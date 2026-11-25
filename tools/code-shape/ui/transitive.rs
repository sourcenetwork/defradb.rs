// aux-build: untyped.rs
// aux-build: forwarder.rs

extern crate forwarder;

fn main() {
    let _ = forwarder::VALUE;
}
