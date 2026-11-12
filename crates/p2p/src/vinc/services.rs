//! The mux ALPN, its stream tags, what each carries, and the urgency its
//! streams are sent at.
//!
//! Every reply goes back on its request's own stream, so there are no reply
//! tags. vinc reads and writes the tag frame itself (`open_bi_tagged`,
//! `accept_bi_tagged`), at most [`vinc::MAX_TAG`] bytes.

/// The one ALPN every Defra protocol is multiplexed over.
pub const ALPN_MUX: &[u8] = b"/defra-vinc/mux/0.1";

pub const STREAM_IDENTITY: &[u8] = b"/defra-vinc/identity/0.1";
pub const STREAM_MANAGE: &[u8] = b"/defra-vinc/manage/0.1";
pub const STREAM_MANAGE_QUERY: &[u8] = b"/defra-vinc/manage-query/0.1";
pub const STREAM_PUSHLOG: &[u8] = b"/defra-vinc/pushlog/0.1";
pub const STREAM_TWOSTREAM: &[u8] = b"/defra-vinc/twostream/0.1";
pub const STREAM_SE: &[u8] = b"/defra-vinc/se/0.1";
pub const STREAM_RECORD: &[u8] = b"/defra-vinc/record/0.1";
/// The records of a node's other peers, which the receiver dials.
pub const STREAM_PEERS: &[u8] = b"/defra-vinc/peers/0.1";
pub const STREAM_DOCSYNC: &[u8] = b"/defra-vinc/docsync/0.1";
pub const STREAM_BRANCHABLE: &[u8] = b"/defra-vinc/branchable/0.1";
pub const STREAM_SE_QUERY: &[u8] = b"/defra-vinc/se-query/0.1";
pub const STREAM_CAR: &[u8] = b"/defra-vinc/car/0.1";

/// The general message limit, as on the other transports.
pub const MAX_MESSAGE_SIZE: usize = 16 * 1024 * 1024;
/// The largest CAR reply a requester reads.
pub const MAX_CAR_SIZE: usize = 64 * 1024 * 1024;
/// The largest management message.
pub const MAX_MANAGE_MSG_SIZE: usize = 4 * 1024 * 1024;
/// An identity request or reply is a peer ID or a token.
const MAX_IDENTITY_MSG_SIZE: usize = 64 * 1024;
/// A signed vinc node record.
const MAX_RECORD_MSG_SIZE: usize = 4 * 1024;
/// The records one peer exchange carries, at most.
pub const MAX_SHARED_PEERS: usize = 16;
const MAX_PEERS_MSG_SIZE: usize = MAX_SHARED_PEERS * 2 * MAX_RECORD_MSG_SIZE;

/// The length of the service-record pointer a node publishes in its record's
/// hints, the same 38 bytes iroh carries in pkarr.
pub const SERVICE_POINTER_LEN: usize = 38;

/// How urgent a service's traffic is, highest first. Each priority has its
/// own inbound byte budget, so a slow bulk transfer cannot starve an ack.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Priority {
    /// Identity, management and acks.
    Control,
    /// Pushes of new writes.
    Replication,
    /// Document, branchable and searchable-encryption sync, node records.
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

    pub const fn index(self) -> usize {
        self as usize
    }

    /// vinc stream urgency: a lower number is sent first on a connection, so a
    /// control message overtakes a CAR reply already queued.
    pub const fn urgency(self) -> u8 {
        match self {
            Priority::Control => 0,
            Priority::Replication => 2,
            Priority::Sync => 4,
            Priority::Bulk => 7,
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

/// One stream tag and the rules for reading its request.
#[derive(Debug)]
pub struct Service {
    pub tag: &'static [u8],
    pub priority: Priority,
    /// The largest request body accepted on this tag, and the largest reply
    /// read back on its stream (a CAR reply excepted).
    pub max_message: usize,
}

const fn service(tag: &'static [u8], priority: Priority, max_message: usize) -> Service {
    Service {
        tag,
        priority,
        max_message,
    }
}

/// Every stream tag this node accepts.
pub const SERVICES: &[Service] = &[
    service(STREAM_IDENTITY, Priority::Control, MAX_IDENTITY_MSG_SIZE),
    service(STREAM_MANAGE, Priority::Control, MAX_MANAGE_MSG_SIZE),
    service(STREAM_MANAGE_QUERY, Priority::Control, MAX_MANAGE_MSG_SIZE),
    service(STREAM_PUSHLOG, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_TWOSTREAM, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_SE, Priority::Replication, MAX_MESSAGE_SIZE),
    service(STREAM_RECORD, Priority::Sync, MAX_RECORD_MSG_SIZE),
    service(STREAM_PEERS, Priority::Sync, MAX_PEERS_MSG_SIZE),
    service(STREAM_DOCSYNC, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_BRANCHABLE, Priority::Sync, MAX_MESSAGE_SIZE),
    service(STREAM_SE_QUERY, Priority::Sync, MAX_MESSAGE_SIZE),
    // A CAR request is small, but its reply on the same stream is a block transfer.
    service(STREAM_CAR, Priority::Bulk, MAX_MESSAGE_SIZE),
];

/// The service a stream tag names, if this node offers it.
pub fn lookup(tag: &[u8]) -> Option<&'static Service> {
    SERVICES.iter().find(|service| service.tag == tag)
}

/// The urgency streams of `tag` are sent at, both the request and its reply.
pub fn urgency_of(tag: &[u8]) -> u8 {
    lookup(tag).map_or(vinc::DEFAULT_URGENCY, |service| service.priority.urgency())
}

#[cfg(test)]
mod tests {
    use rapidhash::HashSetExt;

    use super::*;

    #[test]
    fn every_tag_fits_a_vinc_tag_frame() {
        for service in SERVICES {
            assert!(
                !service.tag.is_empty() && service.tag.len() <= vinc::MAX_TAG,
                "tag {:?} does not fit a stream tag frame",
                String::from_utf8_lossy(service.tag)
            );
        }
    }

    #[test]
    fn tags_are_distinct() {
        let mut seen = rapidhash::RapidHashSet::new();
        for service in SERVICES {
            assert!(seen.insert(service.tag));
        }
    }

    #[test]
    fn control_is_sent_before_replication_before_sync_before_bulk() {
        let order = Priority::ALL.map(Priority::urgency);
        assert!(order.windows(2).all(|pair| pair[0] < pair[1]), "{order:?}");
        assert!(urgency_of(STREAM_MANAGE) < urgency_of(STREAM_CAR));
    }

    #[test]
    fn an_unknown_tag_keeps_the_default_urgency() {
        assert_eq!(urgency_of(b"/nope"), vinc::DEFAULT_URGENCY);
    }
}
