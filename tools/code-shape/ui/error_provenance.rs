// aux-build: error_codegen.rs
// aux-build: color_eyre.rs
#![deny(manual_error_impl, unfulfilled_lint_expectations)]
#![expect(untyped_error)]

extern crate error_codegen;

extern crate color_eyre as decorated;

error_codegen::manual_error!(Generated);

macro_rules! local_error {
    () => {
        #[derive(Debug)]
        struct Local;

        impl std::fmt::Display for Local {
            fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                out.write_str("local")
            }
        }

        #[expect(manual_error_impl)]
        impl std::error::Error for Local {}
    };
}

local_error!();

fn main() {
    let _ = (Generated, Local);
}
