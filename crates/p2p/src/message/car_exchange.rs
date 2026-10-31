//! Go Shinzo's `car` channel: `CARRequest` and `CARReply`.
//!
//! Go `shinzo-develop` opens the channel with `protocol.NewCommChannel(host,
//! "car", ...)`, so it shares `/defradb/car_req/0.0.1` and
//! `/defradb/car_resp/0.0.1` with Rust's raw-CID CAR fetch. Go writes a
//! signed CBOR map; Rust's own request is a bare CID, so the first byte tells
//! them apart (see [`is_cbor_map`]).
//!
//! Both messages flatten Go's `message.MetaData` into the map, field by
//! field, so the bytes match fxamacker/cbor's definite-length map and the
//! signature covers the same bytes on both sides. A nil `[]byte` encodes as
//! CBOR null and an empty one as an empty byte string; [`ByteList`] keeps the
//! difference so a decoded message re-encodes to the bytes it was signed over.

use serde::{Deserialize, Serialize};
use serde_bytes::ByteBuf;

use super::cbor::{nullable_bytes, optional_bytes};
use super::traits::Message;
use crate::protocol::MESSAGE_VERSION;

/// A Go `[][]byte`: `None` is a nil slice (CBOR null), and each entry is
/// `None` for a nil element (CBOR null) or its bytes.
pub type ByteList = Option<Vec<Option<ByteBuf>>>;

/// Whether `data` starts a CBOR map, as Go's signed messages do. A raw CID
/// starts with its version (0x01) or a sha2-256 multihash (0x12), never with
/// a map's major type.
pub fn is_cbor_map(data: &[u8]) -> bool {
    data.first().is_some_and(|byte| byte >> 5 == 5)
}

/// Asks a peer for the CARs rooted at the given heads.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CarRequest {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "MessageID")]
    pub message_id: String,
    #[serde(rename = "SenderID")]
    pub sender_id: String,
    #[serde(rename = "Pubkey", with = "nullable_bytes")]
    pub pubkey: Vec<u8>,
    #[serde(
        rename = "Signature",
        skip_serializing_if = "Option::is_none",
        default,
        with = "optional_bytes"
    )]
    pub signature: Option<Vec<u8>>,
    #[serde(rename = "ErrMessage", skip_serializing_if = "Option::is_none")]
    pub err_message: Option<String>,
    /// The heads, as CID bytes, in the order asked.
    #[serde(rename = "CIDs")]
    pub cids: ByteList,
}

impl CarRequest {
    /// A request for `heads`, unsigned and without an id.
    pub fn new(heads: &[cid::Cid]) -> Self {
        Self {
            version: MESSAGE_VERSION.to_string(),
            cids: Some(
                heads
                    .iter()
                    .map(|head| Some(ByteBuf::from(head.to_bytes())))
                    .collect(),
            ),
            ..Self::default()
        }
    }

    /// The asked heads' bytes, a nil entry read as empty.
    pub fn cid_bytes(&self) -> impl Iterator<Item = &[u8]> {
        entries(&self.cids)
    }
}

/// Answers a [`CarRequest`]: `cars` lines up with a prefix of the asked
/// heads. An empty entry is a head the peer would not or could not serve;
/// heads past the end were not tried and may be asked for again.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct CarReply {
    #[serde(rename = "Version")]
    pub version: String,
    #[serde(rename = "MessageID")]
    pub message_id: String,
    #[serde(rename = "SenderID")]
    pub sender_id: String,
    #[serde(rename = "Pubkey", with = "nullable_bytes")]
    pub pubkey: Vec<u8>,
    #[serde(
        rename = "Signature",
        skip_serializing_if = "Option::is_none",
        default,
        with = "optional_bytes"
    )]
    pub signature: Option<Vec<u8>>,
    #[serde(rename = "ErrMessage", skip_serializing_if = "Option::is_none")]
    pub err_message: Option<String>,
    #[serde(rename = "CARs")]
    pub cars: ByteList,
}

impl CarReply {
    /// A reply to the request `message_id` carrying `cars`; an empty CAR is
    /// written as Go's nil entry, as `serveCARs` leaves an unserved head.
    pub fn new(message_id: &str, cars: Vec<Vec<u8>>) -> Self {
        Self {
            version: MESSAGE_VERSION.to_string(),
            message_id: message_id.to_string(),
            cars: Some(
                cars.into_iter()
                    .map(|car| (!car.is_empty()).then(|| ByteBuf::from(car)))
                    .collect(),
            ),
            ..Self::default()
        }
    }

    /// A failed reply, as Go's `onRequest` sends it: nil CARs and the error.
    pub fn error(message_id: &str, error: &str) -> Self {
        Self {
            version: MESSAGE_VERSION.to_string(),
            message_id: message_id.to_string(),
            err_message: Some(error.to_string()),
            ..Self::default()
        }
    }

    /// The CARs, a nil entry read as empty.
    pub fn car_bytes(&self) -> impl Iterator<Item = &[u8]> {
        entries(&self.cars)
    }

    /// How many entries the reply carries.
    pub fn len(&self) -> usize {
        self.cars.as_ref().map_or(0, Vec::len)
    }

    /// Whether the reply carries no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

fn entries(list: &ByteList) -> impl Iterator<Item = &[u8]> {
    list.iter()
        .flatten()
        .map(|entry| entry.as_ref().map_or(&[][..], |bytes| bytes.as_slice()))
}

macro_rules! impl_message {
    ($type:ty) => {
        impl Message for $type {
            fn version(&self) -> &str {
                &self.version
            }

            fn set_version(&mut self, version: String) {
                self.version = version;
            }

            fn message_id(&self) -> &str {
                &self.message_id
            }

            fn set_message_id(&mut self, id: String) {
                self.message_id = id;
            }

            fn sender_id(&self) -> &str {
                &self.sender_id
            }

            fn set_sender_id(&mut self, id: String) {
                self.sender_id = id;
            }

            fn pubkey(&self) -> &[u8] {
                &self.pubkey
            }

            fn set_pubkey(&mut self, pubkey: Vec<u8>) {
                self.pubkey = pubkey;
            }

            fn signature(&self) -> Option<&[u8]> {
                self.signature.as_deref()
            }

            fn set_signature(&mut self, signature: Option<Vec<u8>>) {
                self.signature = signature;
            }

            fn err_message(&self) -> Option<&str> {
                self.err_message.as_deref()
            }
        }
    };
}

impl_message!(CarRequest);
impl_message!(CarReply);
