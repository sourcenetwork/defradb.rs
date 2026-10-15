use std::future::{pending, Future};
use std::sync::Arc;

use super::super::task_registry::REAP_FLOOR;
use super::{shutdown_tracked_tasks, spawn_task, SpawnedTasks};
use crate::tracked_task::TrackedAbort;

fn registry() -> SpawnedTasks {
    Arc::new(super::TaskRegistry::default())
}

fn spawn(tasks: &SpawnedTasks, future: impl Future<Output = ()> + Send + 'static) {
    let _ = spawn_task(tasks, future);
}

async fn finished(task: &TrackedAbort) {
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        while !task.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await
    .expect("task did not finish");
}

#[tokio::test]
async fn shutdown_rejects_new_work_without_polling_it() {
    let tasks = registry();
    shutdown_tracked_tasks(tasks.clone(), vec![]).await;
    let resource = Arc::new(());
    let retained = Arc::downgrade(&resource);
    spawn(&tasks, async move {
        let _resource = resource;
        panic!("work started after shutdown");
    });
    assert!(
        retained.upgrade().is_none(),
        "rejected work retained its resources"
    );
}

struct SpawnOnDrop {
    tasks: SpawnedTasks,
    resource: Arc<()>,
}

impl Drop for SpawnOnDrop {
    fn drop(&mut self) {
        let resource = self.resource.clone();
        spawn(&self.tasks, async move {
            let _resource = resource;
            pending::<()>().await;
        });
    }
}

#[tokio::test]
async fn shutdown_joins_tasks_and_rejects_work_spawned_during_cleanup() {
    let tasks = registry();
    let resource = Arc::new(());
    let retained = Arc::downgrade(&resource);
    let guard = SpawnOnDrop {
        tasks: tasks.clone(),
        resource,
    };
    let (started, ready) = tokio::sync::oneshot::channel();
    spawn(&tasks, async move {
        let _guard = guard;
        started.send(()).unwrap();
        pending::<()>().await;
    });
    ready.await.unwrap();
    shutdown_tracked_tasks(tasks, vec![]).await;
    assert!(
        retained.upgrade().is_none(),
        "late-spawned task escaped shutdown"
    );
}

#[tokio::test]
async fn spawning_reaps_completed_tasks_and_preserves_individual_cancellation() {
    let tasks = registry();
    for _ in 0..REAP_FLOOR {
        let completed = spawn_task(&tasks, async {}).unwrap();
        finished(&completed).await;
    }
    let resource = Arc::new(());
    let retained = Arc::downgrade(&resource);
    let running = spawn_task(&tasks, async move {
        let _resource = resource;
        pending::<()>().await;
    })
    .unwrap();
    assert_eq!(tasks.len(), 1);
    running.abort();
    shutdown_tracked_tasks(tasks.clone(), vec![]).await;
    assert!(retained.upgrade().is_none());
    assert!(tasks.is_closed());
    shutdown_tracked_tasks(tasks, vec![]).await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_spawn_and_shutdown_release_all_resources() {
    let tasks = registry();
    let resource = Arc::new(());
    let retained = Arc::downgrade(&resource);
    let barrier = Arc::new(tokio::sync::Barrier::new(65));
    let mut callers = tokio::task::JoinSet::new();
    for _ in 0..64 {
        let tasks = tasks.clone();
        let resource = resource.clone();
        let barrier = barrier.clone();
        callers.spawn(async move {
            barrier.wait().await;
            spawn(&tasks, async move {
                let _resource = resource;
                pending::<()>().await;
            });
        });
    }
    drop(resource);
    barrier.wait().await;
    shutdown_tracked_tasks(tasks.clone(), vec![]).await;
    while let Some(result) = callers.join_next().await {
        result.unwrap();
    }
    assert!(tasks.is_closed());
    assert!(retained.upgrade().is_none());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn shutdown_bounds_non_cooperative_tasks_and_readers() {
    for reader in [false, true] {
        let tasks = registry();
        let (release, blocked) = std::sync::mpsc::channel();
        let (started, ready) = tokio::sync::oneshot::channel();
        let work = async move {
            started.send(()).unwrap();
            // Deliberately model a synchronous section that cannot be aborted.
            let _ = blocked.recv();
        };
        let readers = if reader {
            vec![tokio::spawn(work)]
        } else {
            spawn(&tasks, work);
            vec![]
        };
        ready.await.unwrap();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(7),
            shutdown_tracked_tasks(tasks.clone(), readers),
        )
        .await;
        // Release even on failure so the test runtime can shut down.
        let _ = release.send(());
        result.expect("shutdown exceeded its task drain budget");
        assert!(tasks.is_closed());
    }
}

/// Publishing spawns a short task per message beside the endpoint's
/// long-lived ones. Reaping walks every tracked task, so it must not run on
/// every spawn: the walks stay proportional to the spawns, and the queue to
/// the live tasks.
#[tokio::test]
async fn spawning_beside_long_lived_tasks_reaps_amortized() {
    const LIVE: usize = 200;
    const SHORT: usize = 2_000;
    let tasks = registry();
    for _ in 0..LIVE {
        spawn(&tasks, pending::<()>());
    }
    for _ in 0..SHORT {
        let short = spawn_task(&tasks, async {}).unwrap();
        finished(&short).await;
        assert!(
            tasks.len() <= 2 * LIVE + REAP_FLOOR,
            "{} tracked",
            tasks.len()
        );
    }
    assert!(
        tasks.reaps() <= SHORT / LIVE + 4,
        "{} reaps for {} spawns",
        tasks.reaps(),
        LIVE + SHORT
    );
    shutdown_tracked_tasks(tasks, vec![]).await;
}
