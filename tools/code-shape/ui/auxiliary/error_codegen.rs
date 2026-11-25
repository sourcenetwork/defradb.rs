#[macro_export]
macro_rules! manual_error {
    ($name:ident) => {
        #[derive(Debug)]
        struct $name;

        impl std::fmt::Display for $name {
            fn fmt(&self, out: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                out.write_str("generated")
            }
        }

        impl std::error::Error for $name {}
    };
}
