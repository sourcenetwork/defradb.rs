//! An embedder's own stream protocols over iroh (`defra.stream_protocols`).
//! They are a Rust embedder API with no HTTP, CLI or Go surface, so these run
//! under the in-process client; every other client stops at `NewIrohNode`
//! with `unsupported op`.

use integration_test::actions::stream::{Handler, IrohSpec, ProtocolSpec, ReadMode, Size};
use integration_test::actions::{
    Actions, AddCollection, AddDoc, AllowStreamPeer, Close, DenyStreamPeer, NewIrohNode,
    OpenStream, ReadKeptStream,
};
use integration_test::scenario::{Common, Eventually};
use integration_test::test_case::TestCase;
use serde_json::{json, Value};

fn on(node: &str) -> Common {
    Common {
        node: Some(node.into()),
        ..Default::default()
    }
}

fn protocol(tag: &str, handler: Handler) -> ProtocolSpec {
    ProtocolSpec {
        tag: tag.into(),
        handler,
        ..Default::default()
    }
}

fn echo() -> ProtocolSpec {
    protocol("/test/echo/1", Handler::Echo)
}

fn serving(node: &str, protocols: Vec<ProtocolSpec>) -> Actions {
    NewIrohNode {
        iroh: IrohSpec {
            protocols,
            ..Default::default()
        },
        common: on(node),
        ..Default::default()
    }
    .into()
}

fn refused(iroh: IrohSpec, error: &str) -> Actions {
    NewIrohNode {
        iroh,
        expected_error: Some(error.into()),
        common: on("db0"),
        ..Default::default()
    }
    .into()
}

fn open(from: &str, to: &str, protocol: ProtocolSpec) -> OpenStream {
    OpenStream {
        target: to.into(),
        protocol,
        request: "ping".into(),
        common: on(from),
        ..Default::default()
    }
}

fn pinged() -> Value {
    json!({ "reply": { "peer": "opener", "request": "ping" } })
}

fn ping(from: &str, to: &str) -> Actions {
    OpenStream {
        expected_data: pinged(),
        ..open(from, to, echo())
    }
    .into()
}

fn fails(open: OpenStream, error: &str) -> Actions {
    OpenStream {
        expected_error: Some(error.into()),
        ..open
    }
    .into()
}

/// Opens a stream to a `Stuck` or `Hold` handler and keeps it as `name`.
fn kept(from: &str, to: &str, protocol: ProtocolSpec, name: &str) -> Actions {
    OpenStream {
        read: ReadMode::Started,
        keep: Some(name.into()),
        expected_data: json!({ "reply": "started" }),
        ..open(from, to, protocol)
    }
    .into()
}

fn cut(from: &str, name: &str) -> Actions {
    ReadKeptStream {
        stream: name.into(),
        expected_error: Some("stream ended".into()),
        common: on(from),
        ..Default::default()
    }
    .into()
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.registration")]
fn a_node_refuses_a_protocol_it_cannot_serve() {
    let with = |protocols: Vec<ProtocolSpec>| IrohSpec {
        protocols,
        ..Default::default()
    };
    TestCase::from_actions(vec![
        refused(
            with(vec![protocol("/defra-iroh/echo/1", Handler::Echo)]),
            "reserved",
        ),
        refused(
            with(vec![protocol("", Handler::Echo)]),
            "a tag is 1 to 64 bytes",
        ),
        refused(
            with(vec![protocol(&"a".repeat(65), Handler::Echo)]),
            "a tag is 1 to 64 bytes",
        ),
        refused(with(vec![echo(), echo()]), "registered twice"),
        refused(
            with(vec![ProtocolSpec {
                max_message: Size::AboveShare,
                ..echo()
            }]),
            "one peer may hold at most",
        ),
        refused(
            with(vec![ProtocolSpec {
                deadline_millis: Some(0),
                ..echo()
            }]),
            "a deadline is more than zero",
        ),
        refused(
            with(vec![ProtocolSpec {
                deadline_millis: Some(24 * 60 * 60 * 1000 + 1),
                ..echo()
            }]),
            "a deadline is more than zero",
        ),
        refused(
            IrohSpec {
                without_network: true,
                ..with(vec![echo()])
            },
            "stream protocols require the iroh transport",
        ),
        serving(
            "db0",
            vec![ProtocolSpec {
                max_message: Size::Share,
                deadline_millis: Some(24 * 60 * 60 * 1000),
                ..protocol(&"a".repeat(64), Handler::Echo)
            }],
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.registration")]
fn opening_a_protocol_the_opener_cannot_send_fails_before_the_network() {
    TestCase::from_actions(vec![
        serving("db0", vec![echo()]),
        serving("db1", Vec::new()),
        fails(
            open(
                "db1",
                "db0",
                protocol("/defra-iroh/identity/0.1", Handler::Echo),
            ),
            "reserved",
        ),
        fails(
            OpenStream {
                request_size: Some(Size::AboveShare),
                ..open("db1", "db0", echo())
            },
            "larger than stream protocol",
        ),
        fails(
            open("db", "db0", echo()),
            "P2P is not enabled for this node",
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.round_trip")]
fn a_request_and_its_reply_round_trip_without_connecting_first() {
    TestCase::from_actions(vec![
        serving("db0", vec![echo()]),
        serving("db1", Vec::new()),
        ping("db1", "db0"),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.round_trip")]
fn a_handler_answers_from_its_nodes_database() {
    let query = protocol("/test/query/1", Handler::Query);
    TestCase::from_actions(vec![
        serving("db0", vec![query.clone()]),
        serving("db1", Vec::new()),
        AddCollection {
            sdl: "type Book { title: String }".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        AddDoc {
            collection: "Book".into(),
            doc_map: Some(json!({ "title": "Served over a stream" })),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        OpenStream {
            request: "{ Book { title } }".into(),
            expected_data: json!({ "reply": { "Book": [{ "title": "Served over a stream" }] } }),
            ..open("db1", "db0", query)
        }
        .into(),
    ])
    .execute();
}

/// db1 opens to db0 without connecting first; db0 then opens back to db1 by
/// its peer ID alone, which only the connection db1 dialled can carry.
#[test]
#[mantra_macros::req_test("defra.stream_protocols.two_way")]
fn the_connection_a_stream_dials_serves_both_nodes() {
    TestCase::from_actions(vec![
        serving("db0", vec![echo()]),
        serving("db1", vec![echo()]),
        ping("db1", "db0"),
        OpenStream {
            by_id: true,
            expected_data: pinged(),
            ..open("db0", "db1", echo())
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.admission")]
fn a_tag_nobody_registered_is_stopped_and_the_connection_keeps_serving() {
    TestCase::from_actions(vec![
        serving("db0", vec![echo()]),
        serving("db1", Vec::new()),
        fails(
            OpenStream {
                read: ReadMode::UntilStopped,
                ..open(
                    "db1",
                    "db0",
                    protocol("/test/unregistered/1", Handler::Echo),
                )
            },
            "stream reset by the peer with code 2",
        ),
        ping("db1", "db0"),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.admission")]
fn a_peer_the_allowlist_refuses_is_served_once_allowed() {
    TestCase::from_actions(vec![
        NewIrohNode {
            iroh: IrohSpec {
                protocols: vec![echo()],
                explicit_allowlist: true,
                ..Default::default()
            },
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        serving("db1", Vec::new()),
        fails(open("db1", "db0", echo()), "stream failed"),
        AllowStreamPeer {
            target: "db1".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        OpenStream {
            expected_data: pinged(),
            common: Common {
                eventually: Some(Eventually::default()),
                ..on("db1")
            },
            ..open("db1", "db0", echo())
        }
        .into(),
    ])
    .execute();
}

fn large(handler: Handler) -> ProtocolSpec {
    ProtocolSpec {
        priority: Some("control".into()),
        max_message: Size::Share,
        deadline_millis: Some(60_000),
        ..protocol("/test/large/1", handler)
    }
}

/// Two requests, each the peer's whole share, reach handlers that drop their
/// request and hang: the room went back with each request.
#[test]
#[mantra_macros::req_test("defra.stream_protocols.budget")]
fn a_hung_handler_that_dropped_its_request_holds_no_room() {
    let stuck = large(Handler::Stuck);
    let share = |name: &str| -> Actions {
        OpenStream {
            request_size: Some(Size::Share),
            read: ReadMode::Started,
            keep: Some(name.into()),
            expected_data: json!({ "reply": "started" }),
            ..open("db1", "db0", stuck.clone())
        }
        .into()
    };
    TestCase::from_actions(vec![
        serving("db0", vec![stuck.clone()]),
        serving("db1", Vec::new()),
        share("first"),
        share("second"),
    ])
    .execute();
}

/// A handler that keeps its share-sized request holds that room, so a second
/// one is reset busy (code 4).
#[test]
#[mantra_macros::req_test("defra.stream_protocols.budget")]
fn a_handler_that_keeps_its_request_holds_its_room() {
    let hold = large(Handler::Hold);
    TestCase::from_actions(vec![
        serving("db0", vec![hold.clone()]),
        serving("db1", Vec::new()),
        OpenStream {
            request_size: Some(Size::Share),
            read: ReadMode::Started,
            keep: Some("first".into()),
            expected_data: json!({ "reply": "started" }),
            ..open("db1", "db0", hold.clone())
        }
        .into(),
        fails(
            OpenStream {
                request_size: Some(Size::Share),
                read: ReadMode::UntilStopped,
                ..open("db1", "db0", hold)
            },
            "stream reset by the peer with code 4",
        ),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.deadline")]
fn a_handler_past_its_deadline_is_cut_with_code_7() {
    let short = ProtocolSpec {
        deadline_millis: Some(300),
        ..protocol("/test/short/1", Handler::Stuck)
    };
    TestCase::from_actions(vec![
        serving("db0", vec![short.clone()]),
        serving("db1", Vec::new()),
        kept("db1", "db0", short, "s"),
        ReadKeptStream {
            stream: "s".into(),
            expected_error: Some("stream reset by the peer with code 7".into()),
            common: on("db1"),
            ..Default::default()
        }
        .into(),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.revocation")]
fn denying_a_peer_cuts_its_open_streams() {
    let stuck = protocol("/test/stuck/1", Handler::Stuck);
    TestCase::from_actions(vec![
        serving("db0", vec![stuck.clone()]),
        serving("db1", Vec::new()),
        kept("db1", "db0", stuck, "s"),
        DenyStreamPeer {
            target: "db1".into(),
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        cut("db1", "s"),
    ])
    .execute();
}

#[test]
#[mantra_macros::req_test("defra.stream_protocols.shutdown")]
fn closing_the_serving_node_cuts_its_open_streams() {
    let stuck = protocol("/test/stuck/1", Handler::Stuck);
    TestCase::from_actions(vec![
        serving("db0", vec![stuck.clone()]),
        serving("db1", Vec::new()),
        kept("db1", "db0", stuck, "s"),
        Close {
            common: on("db0"),
            ..Default::default()
        }
        .into(),
        cut("db1", "s"),
    ])
    .execute();
}
