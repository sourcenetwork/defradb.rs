//! What each mux stream tag carries, and the limits its message is read under.

use super::protocols::{
    MAX_CAR_SIZE, MAX_MANAGE_MSG_SIZE, MAX_MESSAGE_SIZE, MAX_RECORDS_MSG_SIZE, STREAM_BRANCHABLE,
    STREAM_BRANCHABLE_RESP, STREAM_CAR, STREAM_CAR_RESP, STREAM_DOCSYNC, STREAM_DOCSYNC_RESP,
    STREAM_IDENTITY, STREAM_MANAGE_QUERY_REQ, STREAM_MANAGE_QUERY_RESP, STREAM_MANAGE_REQ,
    STREAM_MANAGE_RESP, STREAM_PUSHLOG, STREAM_SE, STREAM_SERVICE_RECORDS, STREAM_SE_QUERY_REQ,
    STREAM_SE_QUERY_RESP, STREAM_TWOSTREAM, STREAM_TWOSTREAM_RESP,
};

/// How urgent a service's traffic is, highest first. Each priority has its own
/// inbound byte budget, so a slow bulk transfer cannot starve an ack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Identity, management and acks.
    Control,
    /// Pushes of new writes.
    Replication,
    /// Document, branchable and searchable-encryption sync.
    Sync,
    /// CAR block transfers.
    Bulk,
}

impl Priority {
    pub const ALL: [Priority; 4] = [
        Priority::Control,
        Priority::Replication,
        Priority::Sync,
        Priority::Bulk,
    ];

    pub(super) const fn index(self) -> usize {
        self as usize
    }

    /// QUIC send priority: higher is sent first within one connection.
    pub const fn send_priority(self) -> i32 {
        match self {
            Priority::Control => 3,
            Priority::Replication => 2,
            Priority::Sync => 1,
            Priority::Bulk => 0,
        }
    }

    pub const fn name(self) -> &'static str {
        match self {
            Priority::Control => "control",
            Priority::Replication => "replication",
            Priority::Sync => "sync",
            Priority::Bulk => "bulk",
        }
    }
}

/// One stream tag and the rules for reading its message.
#[derive(Debug)]
pub struct Service {
    pub tag: &'static [u8],
    pub priority: Priority,
    /// The largest message body accepted on this tag.
    pub max_message: usize,
}

const fn service(tag: &'static [u8], priority: Priority, max_message: usize) -> Service {
    Service {
        tag,
        priority,
        max_message,
    }
}

/// An identity request or reply is a peer ID or a token.
const MAX_IDENTITY_MSG_SIZE: usize = 64 * 1024;

/// A reverse-stream acknowledgement.
const MAX_REPLY_MSG_SIZE: usize = 1024 * 1024;

/// Every stream tag this node accepts.
pub const SERVICES: &[Service] = &[
    service(STREAM_IDENTITY, Priority::Control, MAX_IDENTITY_MSG_SIZE),
    service(STREAM_TWOSTREAM_RESP, Priority::Control, MAX_REPLY_MSG_SIZE),
    service(STREAM_MANAGE_REQ, Priority::Control, MAX_MANAGE_MSG_SIZE),
    service(STREAM_MANAGE_RESP, Priority::Control, MAX_MANAGE_MSG_SIZE),
    service(
        STREAM_MANAGE_QUERY_REQ,
        Priority::Control,
        MAX_MANAGE_MSG_SIZE,
    ),
    service(
        STREAM_MANAGE_QUERY_RESP,
        Priority::Control,
        MAX_MANAGE_MSG_SIZE,
    ),
    service(STREAM_PUSHLOG, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_TWOSTREAM, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_SE, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_SERVICE_RECORDS, Priority::Sync, MAX_RECORDS_MSG_SIZE),
    service(STREAM_DOCSYNC, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_DOCSYNC_RESP, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_BRANCHABLE, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_BRANCHABLE_RESP, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_SE_QUERY_REQ, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_SE_QUERY_RESP, Priority::Sync, MAX_MESSAGE_SIZE),
    // A CAR request is small, but its reply on the same stream is a block transfer.
    service(STREAM_CAR, Priority::Bulk, MAX_MESSAGE_SIZE),
    service(STREAM_CAR_RESP, Priority::Bulk, MAX_CAR_SIZE),
];

/// The service a stream tag names, if this node offers it.
pub fn lookup(tag: &[u8]) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.tag == tag)
}

#[cfg(test)]
mod tests {
    use rapidhash::HashSetExt;

    use super::*;
    use crate::iroh::protocols::{ALPN_MUX, MAX_STREAM_TAG_LEN};

    #[test]
    fn every_tag_fits_a_stream_tag_frame() {
        for service in SERVICES {
            assert!(
                !service.tag.is_empty() && service.tag.len() <= MAX_STREAM_TAG_LEN,
                "tag {:?} does not fit a stream tag frame",
                String::from_utf8_lossy(service.tag)
            );
        }
    }

    #[test]
    fn tags_are_distinct() {
        let mut seen = rapidhash::RapidHashSet::new();
        for service in SERVICES {
            assert!(
                seen.insert(service.tag),
                "duplicate stream tag {:?}",
                String::from_utf8_lossy(service.tag)
            );
        }
    }

    #[test]
    fn higher_priorities_are_sent_first() {
        let order = Priority::ALL.map(Priority::send_priority);
        assert!(order.windows(2).all(|pair| pair[0] > pair[1]), "{order:?}");
    }

    #[test]
    fn no_tag_collides_with_the_mux_alpn() {
        assert!(lookup(ALPN_MUX).is_none());
    }
}
