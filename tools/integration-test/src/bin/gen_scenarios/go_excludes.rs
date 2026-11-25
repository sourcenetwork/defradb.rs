//! Reads a Go test's `MultiplierExcludes` field: the multipliers it names and
//! the comment Go writes above it.

/// Go's multipliers that defradb.rs runs, by Go constant name and the
/// `DEFRA_MULTIPLIERS` name they share.
const MULTIPLIERS: &[(&str, &str)] = &[("SecondaryIndex", "secondary-index")];

/// A multiplier a Go test does not run under, and the comment Go gives.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GoExclude {
    pub multiplier: &'static str,
    pub reason: String,
}

/// A test's `MultiplierExcludes` as Go states it.
pub enum Excludes {
    /// The excluded multipliers defradb.rs runs; empty when there are none.
    Listed(Vec<GoExclude>),
    /// Go names its list in a way this reader does not follow; the reason.
    Unresolved(String),
}

/// The defradb.rs multipliers in a `[]string{...}` or `[]multiplier.Name{...}`
/// literal at the start of `value`, or `None` when `value` is not one.
pub fn literal(value: &str) -> Option<Vec<&'static str>> {
    let rest = value
        .strip_prefix("[]string{")
        .or_else(|| value.strip_prefix("[]multiplier.Name{"))?;
    let span = &rest[..rest.find('}')?];
    let span: String = span
        .lines()
        .map(|line| line.split("//").next().unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\n");
    let mut out = Vec::new();
    for item in span
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
    {
        let found = match (item.strip_prefix("multiplier."), item.strip_prefix('"')) {
            (Some(constant), _) => MULTIPLIERS.iter().find(|(go, _)| *go == constant),
            (_, Some(quoted)) => {
                let name = quoted.trim_end_matches('"');
                MULTIPLIERS.iter().find(|(_, rust)| *rust == name)
            }
            _ => None,
        };
        if let Some((_, rust)) = found {
            if !out.contains(rust) {
                out.push(*rust);
            }
        }
    }
    Some(out)
}

/// Whether the field at byte `at` of `body` sits on a comment line.
pub fn commented_out(body: &str, at: usize) -> bool {
    let start = body[..at].rfind('\n').map_or(0, |n| n + 1);
    body[start..at].trim_start().starts_with("//")
}

/// The `//` comment lines right above the line holding byte `at`, joined.
pub fn comment_above(body: &str, at: usize) -> String {
    let start = body[..at].rfind('\n').unwrap_or(0);
    let mut lines: Vec<&str> = Vec::new();
    for line in body[..start].lines().rev() {
        match line.trim().strip_prefix("//") {
            Some(text) => lines.push(text.trim()),
            None => break,
        }
    }
    lines.reverse();
    lines.retain(|line| !line.is_empty());
    lines.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_only_multipliers_defradb_rs_runs() {
        let value = "[]string{multiplier.SignedDocs, multiplier.SecondaryIndex},\n";
        assert_eq!(literal(value), Some(vec!["secondary-index"]));
        assert_eq!(
            literal("[]string{multiplier.EncryptedDocs}"),
            Some(Vec::new())
        );
    }

    #[test]
    fn reads_a_named_list_with_comments_and_quoted_names() {
        let value = "[]multiplier.Name{\n// multiplier.SecondaryIndex,\n\"secondary-index\",\n},";
        assert_eq!(literal(value), Some(vec!["secondary-index"]));
    }

    #[test]
    fn a_variable_is_not_a_literal() {
        assert_eq!(literal("branchableCollectionCidExcludes,"), None);
    }

    #[test]
    fn reads_the_comment_above_the_field() {
        let body = "test := TestCase{\n\t\t// Counters cannot be indexed.\n\t\t// https://example/1\n\t\tMultiplierExcludes: []string{},\n";
        let at = body.find("MultiplierExcludes").unwrap();
        assert_eq!(
            comment_above(body, at),
            "Counters cannot be indexed. https://example/1"
        );
        assert!(!commented_out(body, at));
        assert!(commented_out("// MultiplierExcludes: x", 3));
    }
}
