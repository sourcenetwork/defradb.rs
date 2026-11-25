#[macro_export]
macro_rules! external_declaration {
    () => {
        struct External<Value: std::fmt::Debug>(Value);
    };
}
