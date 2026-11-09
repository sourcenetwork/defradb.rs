//! P2P command unit tests

use super::replicator::P2pReplicatorCommand;

#[test]
fn test_p2p_replicator_forget_parses() {
    use clap::Parser;

    #[derive(Parser)]
    struct Wrap {
        #[command(subcommand)]
        command: P2pReplicatorCommand,
    }

    let wrap = Wrap::parse_from(["replicator", "forget", "peer-123"]);
    let P2pReplicatorCommand::Forget(args) = wrap.command else {
        panic!("expected forget subcommand");
    };
    assert_eq!(args.peer_id, "peer-123");
}

#[test]
fn test_p2p_manage_collection_add_parses() {
    use clap::Parser;

    use super::manage::{P2pManageCollectionCommand, P2pManageCommand};
    use super::P2pCommand;

    #[derive(Parser)]
    struct Wrap {
        #[command(subcommand)]
        cmd: P2pCommand,
    }

    let wrap = Wrap::parse_from([
        "p2p",
        "manage",
        "collection",
        "add",
        "--target",
        "/ip4/127.0.0.1/tcp/9000/p2p/abc",
        "--identity",
        "deadbeef",
        "Users",
        "Posts",
    ]);

    let P2pCommand::Manage(manage) = wrap.cmd else {
        panic!("expected manage subcommand");
    };
    let P2pManageCommand::Collection(collection) = manage.command else {
        panic!("expected collection subcommand");
    };
    let P2pManageCollectionCommand::Add(add) = collection.command else {
        panic!("expected add subcommand");
    };
    assert_eq!(add.target.target, "/ip4/127.0.0.1/tcp/9000/p2p/abc");
    assert_eq!(add.target.identity.as_deref(), Some("deadbeef"));
    assert_eq!(add.collection_ids, vec!["Users", "Posts"]);
}

#[test]
fn test_p2p_manage_replicator_add_parses() {
    use clap::Parser;

    use super::manage::{P2pManageCommand, P2pManageReplicatorCommand};
    use super::P2pCommand;

    #[derive(Parser)]
    struct Wrap {
        #[command(subcommand)]
        cmd: P2pCommand,
    }

    let wrap = Wrap::parse_from([
        "p2p",
        "manage",
        "replicator",
        "add",
        "--target",
        "/ip4/127.0.0.1/tcp/9000/p2p/abc",
        "--identity",
        "deadbeef",
        "--address",
        "/ip4/127.0.0.1/tcp/9001/p2p/def",
        "Users",
    ]);

    let P2pCommand::Manage(manage) = wrap.cmd else {
        panic!("expected manage subcommand");
    };
    let P2pManageCommand::Replicator(replicator) = manage.command else {
        panic!("expected replicator subcommand");
    };
    let P2pManageReplicatorCommand::Add(add) = replicator.command else {
        panic!("expected add subcommand");
    };
    assert_eq!(add.address, "/ip4/127.0.0.1/tcp/9001/p2p/def");
    assert_eq!(add.collection_ids, vec!["Users"]);
}

#[test]
fn test_p2p_disconnect_parses_every_address() {
    use clap::Parser;

    use super::P2pCommand;

    #[derive(Parser)]
    struct Wrap {
        #[command(subcommand)]
        cmd: P2pCommand,
    }

    let wrap = Wrap::parse_from([
        "p2p",
        "disconnect",
        "/ip4/1.2.3.4/tcp/1",
        "/ip4/1.2.3.4/tcp/2",
    ]);
    let P2pCommand::Disconnect(args) = wrap.cmd else {
        panic!("expected disconnect subcommand");
    };
    assert_eq!(args.addresses, ["/ip4/1.2.3.4/tcp/1", "/ip4/1.2.3.4/tcp/2"]);
}
