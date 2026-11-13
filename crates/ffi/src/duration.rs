//! Go duration strings (`time.ParseDuration`), as Go's C API takes them.

/// The nanoseconds a Go duration string such as `"1h30m"` or `"2.5s"` names.
pub(crate) fn parse_go_duration(input: &str) -> Result<i64, String> {
    let invalid = || format!("time: invalid duration \"{}\"", input);
    let (negative, mut remaining) = match input.as_bytes().first() {
        Some(b'-') => (true, &input[1..]),
        Some(b'+') => (false, &input[1..]),
        _ => (false, input),
    };
    if remaining == "0" {
        return Ok(0);
    }
    if remaining.is_empty() {
        return Err(invalid());
    }

    let mut total: f64 = 0.0;
    while !remaining.is_empty() {
        let number_end = remaining
            .find(|c: char| !c.is_ascii_digit() && c != '.')
            .unwrap_or(remaining.len());
        if number_end == 0 {
            return Err(invalid());
        }
        let number: f64 = remaining[..number_end].parse().map_err(|_| invalid())?;
        remaining = &remaining[number_end..];

        let unit_end = remaining
            .find(|c: char| c.is_ascii_digit() || c == '.')
            .unwrap_or(remaining.len());
        if unit_end == 0 {
            return Err(format!("time: missing unit in duration \"{}\"", input));
        }
        let nanos_per_unit = match &remaining[..unit_end] {
            "ns" => 1.0,
            "us" | "µs" | "μs" => 1e3,
            "ms" => 1e6,
            "s" => 1e9,
            "m" => 60e9,
            "h" => 3600e9,
            unit => {
                return Err(format!(
                    "time: unknown unit \"{}\" in duration \"{}\"",
                    unit, input
                ))
            }
        };
        remaining = &remaining[unit_end..];
        total += number * nanos_per_unit;
    }

    if total > i64::MAX as f64 {
        return Err(invalid());
    }
    let nanos = total as i64;
    Ok(if negative { -nanos } else { nanos })
}

#[cfg(test)]
mod tests {
    use super::parse_go_duration;

    #[test]
    fn parses_go_durations() {
        assert_eq!(parse_go_duration("0"), Ok(0));
        assert_eq!(parse_go_duration("30s"), Ok(30_000_000_000));
        assert_eq!(parse_go_duration("1h30m"), Ok(5_400_000_000_000));
        assert_eq!(parse_go_duration("2.5ms"), Ok(2_500_000));
        assert_eq!(parse_go_duration("-1s"), Ok(-1_000_000_000));
    }

    #[test]
    fn rejects_what_go_rejects() {
        for input in ["", "30", "s", "1x", "1.2.3s"] {
            assert!(
                parse_go_duration(input).is_err(),
                "{input:?} must not parse"
            );
        }
    }
}
