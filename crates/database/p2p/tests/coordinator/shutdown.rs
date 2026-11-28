use super::{SyncShutdownHandle, BACKGROUND_TASK_SHUTDOWN_TIMEOUT};
use crate::network::{test_network, Network};
use crate::tracked_task::TrackedTask;
use futures::FutureExt;
use nuclei::sync::oneshot;
use std::sync::Arc;

#[test]
fn concurrent_shutdown_callers_wait_for_task_completion() {
    edge::testing::paused(|edge| async move {
        let shutdown = SyncShutdownHandle::new(1, Network::on(edge));
        let (release, wait) = oneshot::channel();
        let resource = Arc::new(());
        let retained = Arc::downgrade(&resource);
        shutdown.spawn_task(async move {
            let _resource = resource;
            let _ = wait.await;
        });

        let first = nuclei::spawn({
            let shutdown = shutdown.clone();
            async move { shutdown.shutdown().await }
        });
        shutdown.cancelled().await;

        let second = shutdown.shutdown();
        let mut second = std::pin::pin!(second);
        nuclei::select! {
            biased;
            _ = &mut second => panic!("shutdown returned while a registered task was still running"),
            _ = nuclei::task::yield_now() => {}
        }

        release.send(()).unwrap();
        second.await;
        first.join().await.unwrap();
        assert!(retained.upgrade().is_none());
        shutdown.shutdown().await;
    })
}

#[nuclei::test(threads = 0)]
async fn abandoned_drain_releases_shutdown_waiters() {
    let shutdown = SyncShutdownHandle::new(1, test_network());
    assert!(shutdown.begin_shutdown());
    // Model a drain future dropped before its first poll.
    drop(shutdown.inner.shutdown_complete_tx.pop());
    nuclei::time::timeout(std::time::Duration::from_secs(1), shutdown.shutdown())
        .await
        .expect("shutdown waited forever after losing its drain");
}

#[nuclei::test(threads = 2)]
async fn shutdown_bounds_joining_a_non_cooperative_task() {
    let shutdown = SyncShutdownHandle::new(1, test_network());
    let (release, blocked) = std::sync::mpsc::channel();
    let (started, ready) = oneshot::channel();
    shutdown.spawn_task(async move {
        started.send(()).unwrap();
        let _ = blocked.recv();
    });
    ready.await.unwrap();
    let result =
        nuclei::time::timeout(std::time::Duration::from_secs(7), shutdown.shutdown()).await;
    let _ = release.send(());
    result.expect("shutdown exceeded its cancellation budget");
}

#[test]
fn shutdown_joins_aborted_tasks_before_returning() {
    edge::testing::paused(|edge| async move {
        let shutdown = SyncShutdownHandle::new(1, Network::on(edge));
        let resource = Arc::new(());
        let retained = Arc::downgrade(&resource);
        let (started, running) = oneshot::channel();

        shutdown.spawn_task(async {});
        shutdown.spawn_task(async move {
            let _resource = resource;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        running.await.unwrap();

        shutdown.shutdown().await;

        assert!(
            retained.upgrade().is_none(),
            "aborted task still owns its resource"
        );
    })
}

#[test]
fn cancelling_the_first_shutdown_caller_does_not_detach_tasks() {
    edge::testing::paused(|edge| async move {
        let shutdown = SyncShutdownHandle::new(1, Network::on(edge));
        let resource = Arc::new(());
        let retained = Arc::downgrade(&resource);
        let (started, running) = oneshot::channel();
        shutdown.spawn_task(async move {
            let _resource = resource;
            started.send(()).unwrap();
            std::future::pending::<()>().await;
        });
        running.await.unwrap();

        let first = nuclei::spawn({
            let shutdown = shutdown.clone();
            async move { shutdown.shutdown().await }
        });
        shutdown.cancelled().await;
        first.abort();
        assert!(first.join().await.unwrap_err().is_cancelled());

        shutdown.shutdown().await;

        assert!(
            retained.upgrade().is_none(),
            "cancelling shutdown detached the task"
        );
    })
}

#[nuclei::test(threads = 0)]
async fn shutdown_rejects_new_tasks_without_spawning_them() {
    let shutdown = SyncShutdownHandle::new(1, test_network());
    shutdown.shutdown().await;
    let resource = Arc::new(());
    let retained = Arc::downgrade(&resource);

    assert!(!shutdown.spawn_task(async move {
        let _resource = resource;
        panic!("task started after shutdown");
    }));

    assert!(
        retained.upgrade().is_none(),
        "rejected task was detached instead of dropped"
    );
    assert_eq!(shutdown.retained_task_count(), 0);
}

#[test]
fn drain_waits_for_an_admitted_registration_to_publish() {
    edge::testing::paused(|edge| async move {
        let shutdown = SyncShutdownHandle::new(1, Network::on(edge));
        let access = shutdown.begin_task_registry_access().unwrap();
        let resource = Arc::new(());
        let retained = Arc::downgrade(&resource);
        let task = TrackedTask::spawn(shutdown.network().edge(), async move {
            let _resource = resource;
            std::future::pending::<()>().await;
        });

        assert!(shutdown.begin_shutdown());
        let mut drain =
            std::pin::pin!(shutdown.drain_background_tasks(BACKGROUND_TASK_SHUTDOWN_TIMEOUT));
        assert!((&mut drain).now_or_never().is_none());
        assert!(shutdown.begin_task_registry_access().is_none());

        access.register_task(task);
        drop(access);
        drain.await;
        assert!(retained.upgrade().is_none());
        assert!(shutdown.inner.background_tasks.is_empty());
    })
}

#[nuclei::test(threads = 2)]
async fn task_registration_racing_shutdown_does_not_leave_resources_alive() {
    for _ in 0..32 {
        let shutdown = SyncShutdownHandle::new(1, test_network());
        let resource = Arc::new(());
        let retained = Arc::downgrade(&resource);
        let barrier = Arc::new(regolith::sync::Barrier::new(2));
        let registration = nuclei::spawn({
            let shutdown = shutdown.clone();
            let barrier = barrier.clone();
            async move {
                barrier.wait().await;
                let task_shutdown = shutdown.clone();
                shutdown.spawn_task(async move {
                    let _resource = resource;
                    task_shutdown.cancelled().await;
                });
            }
        });

        barrier.wait().await;
        shutdown.shutdown().await;
        registration.join().await.unwrap();

        assert!(
            retained.upgrade().is_none(),
            "registration raced past the drain"
        );
        assert!(shutdown.inner.background_tasks.is_empty());
    }
}

#[test]
fn shutdown_on_a_closed_edge_runs_the_drain_and_releases_tracked_tasks() {
    let edge = edge::testing::edge();
    let scope = edge.scope();
    let shutdown = SyncShutdownHandle::new(1, Network::on(scope.edge().clone()));
    let resource = Arc::new(());
    let (release, parked) = oneshot::channel::<()>();
    let held = Arc::clone(&resource);
    assert!(shutdown.spawn_task(async move {
        let _held = held;
        let _ = parked.await;
    }));
    scope.close();

    edge.block_on(async {
        shutdown.shutdown().await;
        assert!(
            shutdown.inner.shutdown_complete.clone().await.is_ok(),
            "the drain ran to its end"
        );
    })
    .unwrap();

    assert_eq!(
        Arc::strong_count(&resource),
        1,
        "the tracked task's future was dropped"
    );
    drop(release);
}
