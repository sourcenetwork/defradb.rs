//! Prints the *Modeled families* table from the registry, so it can never
//! drift from the models.

fn main() {
    print!("{}", conformance::matrix::families_table());
}
