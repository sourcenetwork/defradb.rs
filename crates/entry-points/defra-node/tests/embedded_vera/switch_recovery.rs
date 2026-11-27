//! A switch that does not complete: a stop that begins during it, and a
//! take-over whose start fails once the backup stopped for it, which the
//! host completes in place.
//!
//! Both start a laptop, active, and a phone, its backup; the laptop then
//! stops without a handover and the phone takes the deployment over from its
//! own newest block.

use std::time::Duration;

use defra_node::vera_embedded::{
    HostConfig, HostError, HostedNode, SoloDeployment, TermCertificate,
};
use defra_node::{EmbeddedVeraDevice, Node};
use serde_json::Value;
use vera::vera_rs::RpcTransport as _;

use super::switching::{
    certify, device, free_address, identity, pair, until_settled, vera, Paired, PATIENCE,
};

/// The phone, a backup of the laptop, once the laptop stopped, with the
/// certificate of the phone's take-over and the deployment's key material.
struct Stranded {
    directory: tempfile::TempDir,
    phone: Node,
    paired: Paired,
    deployment: SoloDeployment,
    certificate: TermCertificate,
}

async fn stranded() -> Stranded {
    let directories = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
    let [laptop_device, phone_device] = pair();
    let deployment = identity().deal(laptop_device.listen).expect("deployment");
    let laptop = device(
        directories[0].path(),
        42,
        laptop_device.listen,
        EmbeddedVeraDevice::Active {
            deployment: Box::new(deployment.clone()),
            switching: laptop_device.switching(),
        },
    )
    .await;
    let phone = device(
        directories[1].path(),
        43,
        phone_device.listen,
        EmbeddedVeraDevice::Backup {
            record: deployment.record().to_vec(),
            active: laptop_device.listen,
            switching: phone_device.switching(),
        },
    )
    .await;
    laptop.shutdown().await.expect("laptop shutdown");
    drop(laptop);
    let change = until_settled(|| vera(&phone).takeover_change()).await;
    let [_, directory] = directories;
    Stranded {
        directory,
        phone,
        paired: phone_device,
        deployment,
        certificate: certify(change),
    }
}

fn active(stranded: &Stranded) -> EmbeddedVeraDevice {
    EmbeddedVeraDevice::Active {
        deployment: Box::new(stranded.deployment.clone()),
        switching: stranded.paired.switching(),
    }
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn a_stop_during_a_switch_starts_no_node() {
    workers::block_on(async {
        let stranded = stranded().await;
        let edge = edge::testing::edge();
        let switching = vera(&stranded.phone).take_over(
            stranded.deployment.keys().clone(),
            stranded.certificate.clone(),
        );
        let stopping = async {
            edge.sleep(Duration::from_millis(20)).await;
            stranded.phone.shutdown().await
        };
        let (switched, stopped) = futures::join!(switching, stopping);
        stopped.expect("phone shutdown");
        match switched {
            Ok(()) | Err(HostError::NotRunning) => {}
            Err(error) => panic!("the switch ended otherwise: {error}"),
        }
        let hosted = vera(&stranded.phone);
        assert!(matches!(hosted.term().await, Err(HostError::NotRunning)));
        assert!(!hosted.is_active().await);
        assert!(matches!(
            hosted.complete_switch().await,
            Err(HostError::NotRunning)
        ));
        assert!(matches!(
            hosted.restart(active(&stranded)).await,
            Err(HostError::NotRunning)
        ));

        // No node the switch started outlived the stop: its directory is
        // free for the next node, in whichever role the interrupted switch
        // left it.
        let start = |device| HostConfig {
            data_dir: stranded.directory.path().join("vera"),
            deployment: identity(),
            device,
            listen: stranded.paired.listen,
            start_deadline: Duration::from_secs(30),
            stop_deadline: Duration::from_secs(30),
        };
        let next = match HostedNode::start(start(active(&stranded)), &edge).await {
            Ok(node) => node,
            Err(error) if error.term_refusal().is_some() => HostedNode::start(
                start(EmbeddedVeraDevice::Backup {
                    record: stranded.deployment.record().to_vec(),
                    active: free_address(),
                    switching: stranded.paired.switching(),
                }),
                &edge,
            )
            .await
            .expect("the backup starts again"),
            Err(error) => panic!("the directory is still held: {error}"),
        };
        next.stop().await.expect("stop");
    })
    .unwrap();
}

/// A stop that begins while the switch waits for the backup's thread, or
/// for the thread of the node it launched, says it stopped only once no
/// Vera node thread holds the directory: the directory is free the moment
/// it returns.
#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn a_stop_during_a_switch_waits_for_the_node() {
    workers::block_on(async {
        let stranded = stranded().await;
        let edge = edge::testing::edge();
        let hosted = vera(&stranded.phone);
        let switching = hosted.take_over(
            stranded.deployment.keys().clone(),
            stranded.certificate.clone(),
        );
        let stopping = async {
            edge.sleep(Duration::from_millis(20)).await;
            let stopped = hosted.stop().await;
            let probe = HostedNode::resume_stopped(&HostConfig {
                data_dir: stranded.directory.path().join("vera"),
                deployment: identity(),
                device: active(&stranded),
                listen: stranded.paired.listen,
                start_deadline: Duration::from_secs(30),
                stop_deadline: Duration::from_secs(30),
            });
            (stopped, probe)
        };
        let (_switched, (stopped, probe)) = futures::join!(switching, stopping);
        stopped.expect("stop");
        if let Err(error) = &probe {
            assert!(
                !error.to_string().contains("is in use"),
                "the stop returned while a Vera node still holds the directory: {error}"
            );
        }
        stranded.phone.shutdown().await.expect("phone shutdown");
    })
    .unwrap();
}

#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn a_failed_take_over_is_completed_in_place() {
    workers::block_on(async {
        let stranded = stranded().await;
        let hosted = vera(&stranded.phone);

        // Keys of another deployment pass the certificate's check, and the
        // backup stops for the switch; the node they would start is refused.
        let other = identity().deal(free_address()).expect("other deployment");
        let failed = hosted
            .take_over(other.keys().clone(), stranded.certificate.clone())
            .await
            .expect_err("a start with another deployment's keys fails");
        assert!(!failed.node_runs(), "{failed}");
        assert!(matches!(hosted.term().await, Err(HostError::NotRunning)));
        assert!(!hosted.is_active().await);

        // Completing the switch asks again with the same keys, and fails the
        // same way; the slot stays empty.
        let again = hosted
            .complete_switch()
            .await
            .expect_err("the same keys fail again");
        assert!(!again.node_runs(), "{again}");

        // Started in place with the deployment's own keys, the phone is the
        // active device of the term the certificate started, behind the same
        // transports.
        hosted
            .restart(active(&stranded))
            .await
            .expect("the phone starts as the active device");
        assert!(hosted.is_active().await);
        assert_eq!(
            hosted.term().await.unwrap(),
            stranded.certificate.change.term
        );
        assert!(matches!(
            hosted.restart(active(&stranded)).await,
            Err(HostError::Running)
        ));
        let owner = stranded.phone.vera_acp().unwrap().self_did().unwrap();
        stranded
            .phone
            .vera_acp()
            .unwrap()
            .add_policy(&owner, super::POLICY, None)
            .await
            .expect("the provider reaches the restarted node");

        // A refused switch leaves the node running: the phone cannot follow a
        // term it holds itself.
        let refused = hosted
            .follow(stranded.certificate.clone())
            .await
            .expect_err("a device does not follow itself");
        assert!(refused.term_refusal().is_some(), "{refused}");
        assert!(refused.node_runs(), "{refused}");
        assert!(hosted.is_active().await);

        stranded.phone.shutdown().await.expect("phone shutdown");
    })
    .unwrap();
}

/// A follow whose node starts as the certificate's backup, then fails to read
/// its discarded report, leaves that backup in the slot and behind the
/// transports. The error reports that the node still runs.
#[test]
#[mantra_macros::req_test("defra.acp.vera_embedded_switching")]
fn a_failed_follow_report_leaves_the_started_backup_running() {
    workers::block_on(async {
        let directories = [tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap()];
        let [laptop_device, phone_device] = pair();
        let deployment = identity().deal(laptop_device.listen).expect("deployment");
        let laptop = device(
            directories[0].path(),
            42,
            laptop_device.listen,
            EmbeddedVeraDevice::Active {
                deployment: Box::new(deployment.clone()),
                switching: laptop_device.switching(),
            },
        )
        .await;
        let phone = device(
            directories[1].path(),
            43,
            phone_device.listen,
            EmbeddedVeraDevice::Backup {
                record: deployment.record().to_vec(),
                active: laptop_device.listen,
                switching: phone_device.switching(),
            },
        )
        .await;

        let successor = phone_device.key.public_key();
        let change = until_settled(|| vera(&laptop).hand_over(&successor)).await;
        let certificate = certify(change);
        std::fs::write(
            directories[0]
                .path()
                .join("vera/discarded-submissions.json"),
            b"{",
        )
        .expect("malformed discarded report");

        let failed = vera(&laptop)
            .follow(certificate.clone())
            .await
            .expect_err("the discarded report is malformed");
        assert!(failed.node_runs(), "{failed}");
        assert!(!vera(&laptop).is_active().await);
        assert_eq!(
            vera(&laptop).term().await.expect("the backup runs"),
            certificate.change.term
        );

        let status: Value = vera(&laptop)
            .transport(PATIENCE)
            .call("vera_nodeStatus", Value::Null, 4096)
            .await
            .expect("the transports reach the started backup");
        assert!(status.is_object(), "{status}");

        laptop.shutdown().await.expect("laptop shutdown");
        phone.shutdown().await.expect("phone shutdown");
    })
    .unwrap();
}
