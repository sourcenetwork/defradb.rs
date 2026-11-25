#![cfg_attr(code_shape_ui, deny(untyped_error, test_result))]

// Keep the parse error typed instead of returning an untyped error container.
fn parse_count(input: &str) -> Result<u32, std::num::ParseIntError> {
    input.parse()
}

// Keep a foreign error out of the test signature and assert on the outcome.
#[test]
fn parses_count() {
    assert_eq!(parse_count("42"), Ok(42));
}

fn main() {
    assert_eq!(parse_count("42"), Ok(42));
}
