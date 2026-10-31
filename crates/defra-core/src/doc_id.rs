use std::fmt;

use uuid::Uuid;

/// DocID version constant (matches Go's DocIDV0).
pub const DOC_ID_V0: u16 = 0x01;

/// Parsed DocID components shared across crates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ParsedDocId {
    version: u16,
    uuid: Uuid,
}

impl ParsedDocId {
    pub fn new(version: u16, uuid: Uuid) -> Result<Self, DocIdFormatError> {
        if version != DOC_ID_V0 {
            return Err(DocIdFormatError::InvalidVersion(version));
        }

        Ok(Self { version, uuid })
    }

    /// Parse a DocID from its string representation.
    ///
    /// Format: `{base32(version)}-{uuid}`
    pub fn from_string(s: &str) -> Result<Self, DocIdFormatError> {
        let parts: Vec<&str> = s.splitn(2, '-').collect();
        if parts.len() != 2 {
            return Err(DocIdFormatError::Malformed);
        }

        let version_str = parts[0];
        let uuid_str = parts[1];

        let (_, version_bytes) = multibase::decode(version_str)?;
        if version_bytes.is_empty() {
            return Err(DocIdFormatError::Malformed);
        }

        let version = read_uvarint(&version_bytes).ok_or(DocIdFormatError::Malformed)?;
        if version != DOC_ID_V0 as u64 {
            return Err(DocIdFormatError::InvalidVersion(version as u16));
        }

        let uuid = parse_uuid(uuid_str)?;

        Ok(Self {
            version: version as u16,
            uuid,
        })
    }

    /// Parse a DocID from its binary form.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, DocIdFormatError> {
        if bytes.len() < 17 {
            return Err(DocIdFormatError::Malformed);
        }

        let version = read_uvarint(bytes).ok_or(DocIdFormatError::Malformed)?;
        if version != DOC_ID_V0 as u64 {
            return Err(DocIdFormatError::InvalidVersion(version as u16));
        }

        let uuid_start = varint_size(version);
        if bytes.len() < uuid_start + 16 {
            return Err(DocIdFormatError::Malformed);
        }

        let uuid_bytes: [u8; 16] = bytes[uuid_start..uuid_start + 16]
            .try_into()
            .map_err(|_| DocIdFormatError::Malformed)?;
        let uuid = Uuid::from_bytes(uuid_bytes);

        Ok(Self {
            version: version as u16,
            uuid,
        })
    }

    pub fn version(&self) -> u16 {
        self.version
    }

    pub fn uuid(&self) -> Uuid {
        self.uuid
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::with_capacity(18);
        write_uvarint(&mut buf, self.version as u64);
        buf.extend_from_slice(self.uuid.as_bytes());
        buf
    }
}

impl fmt::Display for ParsedDocId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let size = varint_size(self.version as u64);
        let mut version_buf = vec![0_u8; size];
        write_uvarint_to_slice(&mut version_buf, self.version as u64);
        let version_str = multibase::encode(multibase::Base::Base32Lower, &version_buf);

        write!(f, "{}-{}", version_str, self.uuid)
    }
}

pub fn validate_doc_id_str(s: &str) -> Result<(), DocIdFormatError> {
    ParsedDocId::from_string(s).map(|_| ())
}

#[derive(Debug, thiserror::Error)]
pub enum DocIdFormatError {
    #[error("malformed document ID")]
    Malformed,

    #[error("invalid document ID version: {0}")]
    InvalidVersion(u16),

    #[error("multibase decode error: {0}")]
    Multibase(#[from] multibase::Error),

    /// The UUID part did not parse; gofrs/uuid's text, as Go reports it.
    #[error("{0}")]
    Uuid(String),
}

fn read_uvarint(buf: &[u8]) -> Option<u64> {
    let mut x: u64 = 0;
    let mut s: u32 = 0;
    for (i, &b) in buf.iter().enumerate() {
        if i == 10 {
            return None;
        }
        if b < 0x80 {
            if i == 9 && b > 1 {
                return None;
            }
            return Some(x | (b as u64) << s);
        }
        x |= ((b & 0x7f) as u64) << s;
        s += 7;
    }
    None
}

fn write_uvarint(buf: &mut Vec<u8>, mut x: u64) {
    while x >= 0x80 {
        buf.push((x as u8) | 0x80);
        x >>= 7;
    }
    buf.push(x as u8);
}

fn write_uvarint_to_slice(buf: &mut [u8], mut x: u64) -> usize {
    debug_assert!(
        buf.len() >= varint_size(x),
        "buffer too small for varint: need {} bytes, got {}",
        varint_size(x),
        buf.len()
    );

    let mut i = 0;
    while x >= 0x80 && i < buf.len() {
        buf[i] = (x as u8) | 0x80;
        x >>= 7;
        i += 1;
    }
    if i < buf.len() {
        buf[i] = x as u8;
        i += 1;
    }
    i
}

fn varint_size(x: u64) -> usize {
    if x == 0 {
        return 1;
    }

    let mut size = 0;
    let mut v = x;
    while v > 0 {
        size += 1;
        v >>= 7;
    }
    size
}

/// Parse a DocID's UUID part as gofrs/uuid v5 does (the parser Go's
/// `NewDocIDFromString` calls), with its forms and its messages: 32 hex
/// digits, the canonical 36, either one in braces (34, 38) or after
/// `urn:uuid:` (41, 45). Modeled in proofs/lean/DocIdText.lean.
fn parse_uuid(text: &str) -> Result<Uuid, DocIdFormatError> {
    let bytes = text.as_bytes();
    let refused = |message: String| DocIdFormatError::Uuid(message);
    let format = |shown: &[u8]| {
        refused(format!(
            "uuid: incorrect UUID format in string {:?}",
            String::from_utf8_lossy(shown)
        ))
    };
    let inner = match bytes.len() {
        32 | 36 => bytes,
        34 | 38 => match (bytes.first(), bytes.last()) {
            (Some(b'{'), Some(b'}')) => &bytes[1..bytes.len() - 1],
            _ => return Err(format(bytes)),
        },
        41 | 45 => match bytes.strip_prefix(b"urn:uuid:") {
            Some(rest) => rest,
            None => return Err(format(&bytes[..9])),
        },
        length => {
            return Err(refused(format!(
                "uuid: incorrect UUID length {length} in string {text:?}"
            )))
        }
    };
    let hex: Vec<u8> = match inner.len() {
        36 if [8, 13, 18, 23].iter().any(|&at| inner[at] != b'-') => return Err(format(inner)),
        36 => inner
            .iter()
            .enumerate()
            .filter(|(at, _)| ![8, 13, 18, 23].contains(at))
            .map(|(_, byte)| *byte)
            .collect(),
        _ => inner.to_vec(),
    };
    let invalid = || refused("uuid: invalid UUID format".to_owned());
    let mut out = [0_u8; 16];
    for (slot, pair) in out.iter_mut().zip(hex.chunks(2)) {
        let high = (pair[0] as char).to_digit(16).ok_or_else(invalid)?;
        let low = (pair[1] as char).to_digit(16).ok_or_else(invalid)?;
        *slot = (high * 16 + low) as u8;
    }
    Ok(Uuid::from_bytes(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_bad_uuid_is_refused_in_gofrs_words() {
        let error = ParsedDocId::from_string("bae-35953ca-518d-9e6b-9ce6cd00eff5").unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"uuid: incorrect UUID length 30 in string "35953ca-518d-9e6b-9ce6cd00eff5""#
        );
        let error =
            ParsedDocId::from_string("bae-z94acbfa-dd53-40d0-97f3-29ce16c333fc").unwrap_err();
        assert_eq!(error.to_string(), "uuid: invalid UUID format");
        let error =
            ParsedDocId::from_string("bae-c94acbfa_dd53-40d0-97f3-29ce16c333fc").unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"uuid: incorrect UUID format in string "c94acbfa_dd53-40d0-97f3-29ce16c333fc""#
        );
        let error = ParsedDocId::from_string("bae-xc94acbfadd5340d097f329ce16c333fc}").unwrap_err();
        assert_eq!(
            error.to_string(),
            r#"uuid: incorrect UUID format in string "xc94acbfadd5340d097f329ce16c333fc}""#
        );
    }

    #[test]
    fn every_gofrs_form_parses_to_the_same_uuid() {
        let canonical = "c94acbfa-dd53-40d0-97f3-29ce16c333fc";
        let hash = "c94acbfadd5340d097f329ce16c333fc";
        for form in [
            canonical.to_owned(),
            hash.to_owned(),
            format!("{{{canonical}}}"),
            format!("{{{hash}}}"),
            format!("urn:uuid:{canonical}"),
            format!("urn:uuid:{hash}"),
        ] {
            assert_eq!(parse_uuid(&form).unwrap().to_string(), canonical, "{form}");
        }
    }

    #[test]
    fn test_parse_valid_doc_id() {
        let parsed = ParsedDocId::from_string("bae-c94acbfa-dd53-40d0-97f3-29ce16c333fc").unwrap();

        assert_eq!(parsed.version(), DOC_ID_V0);
        assert_eq!(
            parsed.to_string(),
            "bae-c94acbfa-dd53-40d0-97f3-29ce16c333fc"
        );
    }

    #[test]
    fn test_parse_invalid_doc_id() {
        assert!(ParsedDocId::from_string("bae-not-a-uuid").is_err());
        assert!(ParsedDocId::from_string("nodash").is_err());
    }

    #[test]
    fn test_wrong_length_uuid_names_the_length_like_go() {
        let err = ParsedDocId::from_string("bae-35953ca-518d-9e6b-9ce6cd00eff5").unwrap_err();
        assert_eq!(
            err.to_string(),
            "uuid: incorrect UUID length 30 in string \"35953ca-518d-9e6b-9ce6cd00eff5\""
        );
    }
}
