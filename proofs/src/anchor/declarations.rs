//! What Rust source declares, read from its tokens: every name a `fn`,
//! `struct`, `enum`, `union`, `trait`, `type`, `const`, `static`, `mod` or
//! `macro_rules!` introduces, every struct field and enum variant, every trait
//! an `impl` implements, and what each owner holds: a struct or union its
//! fields, an enum its variants, a trait or an inline module the items it
//! declares directly, and an `impl` its items, for its type and its trait.
//! Comments, strings and character literals are skipped.

use std::collections::{HashMap, HashSet};

const DECLARING: &[&str] = &[
    "fn", "struct", "enum", "union", "trait", "type", "const", "static", "mod",
];

#[derive(Default)]
pub struct Declarations {
    names: HashSet<String>,
    owned: HashMap<String, HashSet<String>>,
}

#[derive(Clone, Copy)]
enum Holds {
    Fields,
    Variants,
    Items,
}

#[derive(Clone, Copy)]
struct Owner<'a> {
    holds: Holds,
    name: &'a str,
    implements: Option<&'a str>,
}

impl Declarations {
    pub fn read(source: &str) -> Self {
        let tokens = tokens(source);
        let mut found = Self::default();
        let mut open: Vec<Option<Owner>> = Vec::new();
        let mut header: Option<(Owner, usize)> = None;
        for (at, &token) in tokens.iter().enumerate() {
            let prev = at.checked_sub(1).map(|i| tokens[i]);
            let next = tokens.get(at + 1).copied();
            match token {
                "{" => {
                    let owner = header.take().filter(|&(_, depth)| depth == open.len());
                    open.push(owner.map(|(owner, _)| owner));
                    continue;
                }
                "(" | "[" => {
                    open.push(None);
                    continue;
                }
                "}" | ")" | "]" => {
                    open.pop();
                    continue;
                }
                ";" => {
                    header = header.filter(|&(_, depth)| depth != open.len());
                    continue;
                }
                _ if !is_ident(&token) => continue,
                _ => {}
            }
            let declared = prev.is_some_and(|p| DECLARING.contains(&p))
                || (prev == Some("!") && at >= 2 && tokens[at - 2] == "macro_rules");
            let direct = open.last().copied().flatten();
            let held = direct.is_some_and(|owner| match owner.holds {
                Holds::Fields => {
                    next == Some(":") && tokens.get(at + 2) != Some(&":") && prev != Some(":")
                }
                Holds::Variants => matches!(prev, Some("{" | "," | "]")),
                Holds::Items => declared,
            });
            if declared || held {
                found.names.insert(token.to_string());
            }
            if let (true, Some(owner)) = (held, direct) {
                for name in std::iter::once(owner.name).chain(owner.implements) {
                    found.hold(name, token);
                }
            }
            let holds = match token {
                "struct" | "union" => Some(Holds::Fields),
                "enum" => Some(Holds::Variants),
                "trait" | "mod" => Some(Holds::Items),
                _ => None,
            };
            if let (Some(holds), Some(name)) = (holds, next.filter(is_ident)) {
                let owner = Owner {
                    holds,
                    name,
                    implements: None,
                };
                header = Some((owner, open.len()));
            }
            if token == "impl" && matches!(prev, None | Some("}" | ";" | "{" | "]" | "unsafe")) {
                if let Some((name, implements)) = impl_owner(&tokens[at + 1..]) {
                    found.names.extend(implements.map(str::to_string));
                    let owner = Owner {
                        holds: Holds::Items,
                        name,
                        implements,
                    };
                    header = Some((owner, open.len()));
                }
            }
        }
        found
    }

    fn hold(&mut self, owner: &str, item: &str) {
        self.owned
            .entry(owner.to_string())
            .or_default()
            .insert(item.to_string());
    }

    pub fn declares(&self, name: &str) -> bool {
        self.names.contains(name)
    }

    pub fn holds(&self, owner: &str, item: &str) -> bool {
        self.owned
            .get(owner)
            .is_some_and(|items| items.contains(item))
    }
}

/// The type an `impl` header is for and the trait it implements: the last
/// path segment outside generics, after `for` and before it.
fn impl_owner<'a>(header: &[&'a str]) -> Option<(&'a str, Option<&'a str>)> {
    let mut angle = 0usize;
    let mut owner = None;
    let mut implements = None;
    let mut prev = "";
    for &token in header {
        match token {
            "{" | ";" => break,
            "where" if angle == 0 => break,
            "<" => angle += 1,
            ">" if prev != "-" => angle = angle.saturating_sub(1),
            "for" if angle == 0 => implements = owner.take(),
            "dyn" | "mut" | "const" | "unsafe" => {}
            _ if angle == 0 && is_ident(&token) => owner = Some(token),
            _ => {}
        }
        prev = token;
    }
    owner.map(|owner| (owner, implements))
}

fn is_ident(token: &&str) -> bool {
    token
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic() || c == '_')
        && token.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// Identifiers and single ASCII punctuation, in order.
fn tokens(source: &str) -> Vec<&str> {
    let bytes = source.as_bytes();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let rest = &bytes[at..];
        let c = rest[0];
        let skip = if rest.starts_with(b"//") {
            rest.iter().position(|&b| b == b'\n').unwrap_or(rest.len())
        } else if rest.starts_with(b"/*") {
            block_comment(rest)
        } else if c == b'"' {
            quoted(rest)
        } else if c == b'\'' {
            character(source, at)
        } else if c.is_ascii_alphanumeric() || c == b'_' {
            let len = rest
                .iter()
                .position(|&b| !(b.is_ascii_alphanumeric() || b == b'_'))
                .unwrap_or(rest.len());
            match string_prefix(&source[at..at + len], &rest[len..]) {
                Some(string) => len + string,
                None => {
                    tokens.push(&source[at..at + len]);
                    len
                }
            }
        } else {
            if c.is_ascii_punctuation() {
                tokens.push(&source[at..at + 1]);
            }
            1
        };
        at += skip.max(1);
    }
    tokens
}

fn block_comment(rest: &[u8]) -> usize {
    let mut depth = 0usize;
    let mut at = 0;
    while at < rest.len() {
        if rest[at..].starts_with(b"/*") {
            depth += 1;
            at += 2;
        } else if rest[at..].starts_with(b"*/") {
            depth -= 1;
            at += 2;
            if depth == 0 {
                return at;
            }
        } else {
            at += 1;
        }
    }
    rest.len()
}

/// The length of a string opening at `rest[0]`, escapes included.
fn quoted(rest: &[u8]) -> usize {
    let mut at = 1;
    while at < rest.len() {
        match rest[at] {
            b'\\' => at += 2,
            b'"' => return at + 1,
            _ => at += 1,
        }
    }
    rest.len()
}

/// After an identifier `word`, the length of the string it prefixes
/// (`b"…"`, `r#"…"#`, `br"…"`, `c"…"`), if it prefixes one.
fn string_prefix(word: &str, after: &[u8]) -> Option<usize> {
    let raw = matches!(word, "r" | "br" | "cr");
    if !(raw || matches!(word, "b" | "c")) {
        return None;
    }
    let hashes = if raw {
        after.iter().take_while(|&&b| b == b'#').count()
    } else {
        0
    };
    if after.get(hashes) != Some(&b'"') {
        return None;
    }
    let body = &after[hashes..];
    Some(
        hashes
            + if raw {
                raw_quoted(body, hashes)
            } else {
                quoted(body)
            },
    )
}

fn raw_quoted(rest: &[u8], hashes: usize) -> usize {
    let mut close = vec![b'"'];
    close.resize(1 + hashes, b'#');
    rest[1..]
        .windows(close.len())
        .position(|window| window == close.as_slice())
        .map_or(rest.len(), |at| at + 1 + close.len())
}

/// A character literal's length at `source[at]`, or 1 for a lifetime's tick.
fn character(source: &str, at: usize) -> usize {
    let bytes = source.as_bytes();
    if bytes.get(at + 1) == Some(&b'\\') {
        return bytes
            .get(at + 3..)
            .and_then(|rest| rest.iter().position(|&b| b == b'\''))
            .map_or(1, |end| end + 4);
    }
    let Some(c) = source[at + 1..].chars().next() else {
        return 1;
    };
    let end = at + 1 + c.len_utf8();
    if bytes.get(end) == Some(&b'\'') {
        end + 1 - at
    } else {
        1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owners_hold_only_what_they_declare() {
        let found = Declarations::read(
            r##"
            /// fn in_a_comment
            pub struct Queue { owners: Map<Cid, Event>, }
            pub struct Guard { cid: Cid }
            pub enum Outcome { #[default] Merged, Failed(Vec<u8>), Deferred { after: u64 } }
            impl<'a, F: Fn() -> u8> std::fmt::Debug for Queue where F: Copy {
                fn fmt(&self) -> impl Iterator<Item = u8> { let s = "fn in_a_string {"; let c = '{'; }
            }
            impl Queue { pub(crate) const CAP: usize = 4; async fn try_acquire(&self) { guard.cid; } }
            mod tests { #[test] fn named_test() { let r = r#"fn raw"#; } }
            macro_rules! made { () => {} }
            "##,
        );
        for name in [
            "Queue", "owners", "cid", "Merged", "Failed", "Deferred", "fmt", "Debug",
        ] {
            assert!(found.declares(name), "{name}");
        }
        for name in ["try_acquire", "CAP", "named_test", "made"] {
            assert!(found.declares(name), "{name}");
        }
        for (owner, item) in [
            ("Queue", "owners"),
            ("Queue", "fmt"),
            ("Debug", "fmt"),
            ("Queue", "try_acquire"),
            ("Queue", "CAP"),
            ("Guard", "cid"),
            ("Outcome", "Merged"),
            ("Outcome", "Deferred"),
            ("tests", "named_test"),
        ] {
            assert!(found.holds(owner, item), "{owner}::{item}");
        }
        for name in [
            "in_a_comment",
            "in_a_string",
            "raw",
            "guard",
            "after",
            "Event",
        ] {
            assert!(!found.declares(name), "{name}");
        }
        for (owner, item) in [("Queue", "cid"), ("Outcome", "after"), ("Iterator", "fmt")] {
            assert!(!found.holds(owner, item), "{owner}::{item}");
        }
    }
}
