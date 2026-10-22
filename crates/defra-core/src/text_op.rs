//! Edits to a Text field, as a Text block carries them.
//!
//! A block's ops apply in order. The first counts its position in the block's
//! parent version (the text its parent heads produce); each later op counts in
//! the text the ops before it left. Positions and lengths count Unicode scalar
//! values. A position past the end means the end, and a range past the end
//! stops there, so `Delete { pos: 0, len: u64::MAX }` clears the text whatever
//! its length.

use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// One edit to a Text field.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TextOp {
    /// Insert `text` before the character at `pos`.
    #[serde(rename = "ins")]
    Insert { pos: u64, text: String },
    /// Delete `len` characters starting at `pos`.
    #[serde(rename = "del")]
    Delete { pos: u64, len: u64 },
}

impl TextOp {
    /// The ops that turn any text into `text`: clear it, then insert `text`.
    pub fn replace_all(text: &str) -> Vec<TextOp> {
        let mut ops = vec![TextOp::Delete {
            pos: 0,
            len: u64::MAX,
        }];
        if !text.is_empty() {
            ops.push(TextOp::Insert {
                pos: 0,
                text: text.to_owned(),
            });
        }
        ops
    }

    /// The fewest ops, by common prefix and suffix, that turn `old` into `new`:
    /// one delete of the middle that changed, one insert of what replaces it.
    pub fn diff(old: &str, new: &str) -> Vec<TextOp> {
        let prefix = old
            .chars()
            .zip(new.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let old_rest: Vec<char> = old.chars().skip(prefix).collect();
        let new_rest: Vec<char> = new.chars().skip(prefix).collect();
        let suffix = old_rest
            .iter()
            .rev()
            .zip(new_rest.iter().rev())
            .take_while(|(a, b)| a == b)
            .count();
        let removed = old_rest.len() - suffix;
        let inserted: String = new_rest[..new_rest.len() - suffix].iter().collect();
        let mut ops = Vec::with_capacity(2);
        if removed > 0 {
            ops.push(TextOp::Delete {
                pos: prefix as u64,
                len: removed as u64,
            });
        }
        if !inserted.is_empty() {
            ops.push(TextOp::Insert {
                pos: prefix as u64,
                text: inserted,
            });
        }
        ops
    }

    /// `text` after `ops`, applied in order.
    pub fn apply_all(text: &str, ops: &[TextOp]) -> String {
        let mut chars: Vec<char> = text.chars().collect();
        for op in ops {
            match op {
                TextOp::Insert { pos, text } => {
                    let at = clamp(*pos, chars.len());
                    chars.splice(at..at, text.chars());
                }
                TextOp::Delete { pos, len } => {
                    let start = clamp(*pos, chars.len());
                    let end = clamp(pos.saturating_add(*len), chars.len());
                    chars.drain(start..end);
                }
            }
        }
        chars.into_iter().collect()
    }

    /// The CBOR a Text block's `data` holds for `ops`.
    pub fn encode_all(ops: &[TextOp]) -> Result<Vec<u8>> {
        let mut bytes = Vec::new();
        ciborium::into_writer(ops, &mut bytes)
            .map_err(|error| Error::Serialization(format!("text ops: {error}")))?;
        Ok(bytes)
    }

    /// The ops a Text block's `data` holds.
    pub fn decode_all(bytes: &[u8]) -> Result<Vec<TextOp>> {
        ciborium::from_reader(bytes)
            .map_err(|error| Error::Serialization(format!("text ops: {error}")))
    }
}

/// `pos` as an index into a text of `len` characters, the end when past it.
fn clamp(pos: u64, len: usize) -> usize {
    usize::try_from(pos).map_or(len, |pos| pos.min(len))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_diff_applied_to_the_old_text_gives_the_new_one() {
        let cases = [
            ("", ""),
            ("", "abc"),
            ("abc", ""),
            ("The fox", "The quick fox"),
            ("The quick fox", "The fox"),
            ("abcdef", "abXYef"),
            ("aaaa", "aa"),
            ("héllo wörld", "hello world"),
            ("same", "same"),
        ];
        for (old, new) in cases {
            let ops = TextOp::diff(old, new);
            assert!(ops.len() <= 2, "{old:?} -> {new:?}: {ops:?}");
            assert_eq!(TextOp::apply_all(old, &ops), new, "{old:?} -> {new:?}");
        }
        assert!(TextOp::diff("same", "same").is_empty());
    }

    #[test]
    fn positions_past_the_end_stop_at_the_end() {
        let ops = [
            TextOp::Insert {
                pos: 99,
                text: "!".into(),
            },
            TextOp::Delete { pos: 1, len: 99 },
        ];
        assert_eq!(TextOp::apply_all("abc", &ops[..1]), "abc!");
        assert_eq!(TextOp::apply_all("abc", &ops[1..]), "a");
        assert_eq!(TextOp::apply_all("abc", &TextOp::replace_all("xy")), "xy");
    }

    #[test]
    fn positions_count_characters_not_bytes() {
        let ops = [TextOp::Insert {
            pos: 1,
            text: "ö".into(),
        }];
        assert_eq!(TextOp::apply_all("é!", &ops), "éö!");
    }

    #[test]
    fn ops_round_trip_through_their_encoding() {
        let ops = vec![
            TextOp::Insert {
                pos: 3,
                text: "héllo".into(),
            },
            TextOp::Delete { pos: 1, len: 2 },
        ];
        let bytes = TextOp::encode_all(&ops).unwrap();
        assert_eq!(TextOp::decode_all(&bytes).unwrap(), ops);
    }

    #[test]
    fn replace_all_clears_then_inserts() {
        assert_eq!(
            TextOp::replace_all("ab"),
            vec![
                TextOp::Delete {
                    pos: 0,
                    len: u64::MAX
                },
                TextOp::Insert {
                    pos: 0,
                    text: "ab".into()
                },
            ]
        );
        assert_eq!(TextOp::replace_all("").len(), 1);
    }

    #[test]
    fn garbage_is_an_error() {
        assert!(TextOp::decode_all(&[0xff, 0x00]).is_err());
    }
}
