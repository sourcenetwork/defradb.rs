//! Several submissions handed to the pool at once: each is admitted as
//! [`Pool::submit`] admits it, and the jobs bound for one worker reach its
//! inbox together with one wake, so a batch crosses to each worker once.

use std::future::Future;
use std::sync::Arc;

use super::{Pool, Submit, Target};
use crate::completion::Completion;
use crate::worker::Job;
use crate::Error;

impl Pool {
    /// Runs the future each `make` of `jobs` builds on the worker its target
    /// names, as [`submit`](Self::submit) would one at a time, and returns
    /// each one's completion in order, or why it was not admitted. The jobs
    /// for one worker reach its inbox together, in order, with one wake.
    /// While a worker is full it waits, having handed over what it admitted
    /// so far, so a batch never holds the room it waits for.
    pub async fn submit_batch<F, Fut>(
        &self,
        jobs: impl IntoIterator<Item = (Target, F)>,
    ) -> Vec<Result<Completion<Fut::Output>, Error>>
    where
        F: FnOnce() -> Fut + Send + 'static,
        Fut: Future + 'static,
        Fut::Output: Send + 'static,
    {
        let shared = &self.handle.shared;
        let bytes = std::mem::size_of::<F>();
        let mut admitted = Vec::new();
        let mut completions = Vec::new();
        for (target, make) in jobs {
            let room = match shared.admit_now(target, bytes) {
                Ok(Some(room)) => Ok(room),
                Ok(None) => {
                    hand_over(&mut admitted);
                    match shared.route(target) {
                        Ok(worker) => Ok(worker.admit(bytes).await),
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            };
            completions.push(room.map(|room| {
                let (job, completion) = Self::job(target, Submit::default(), room, make);
                admitted.push(job);
                completion
            }));
        }
        hand_over(&mut admitted);
        completions
    }
}

/// Hands `jobs` to their workers: each worker's in one push, in their order,
/// with one wake.
fn hand_over(jobs: &mut Vec<Job>) {
    // Stable: each worker's jobs keep their order.
    jobs.sort_by_key(|job| job.admitted.worker().id.index);
    let mut jobs = jobs.drain(..).peekable();
    while let Some(first) = jobs.next() {
        let worker = Arc::clone(first.admitted.worker());
        let rest =
            std::iter::from_fn(|| jobs.next_if(|job| Arc::ptr_eq(job.admitted.worker(), &worker)));
        worker.push_all(std::iter::once(first).chain(rest));
    }
}

#[cfg(all(test, not(target_family = "wasm")))]
mod tests {
    use std::time::Duration;

    use crate::{block_on_timeout, current_worker, Config, Pool, Target};

    const LIMIT: Duration = Duration::from_secs(10);

    #[test]
    fn each_job_runs_where_its_target_names_and_answers_in_order() {
        let pool = Pool::start(Config::unplaced(2)).unwrap();
        let jobs = [1, 0, 1, 0].into_iter().enumerate().map(|(order, worker)| {
            let job = move || async move { (order, current_worker().map(|id| id.index)) };
            (Target::Worker(worker), job)
        });
        let ran = block_on_timeout(
            async {
                let mut ran = Vec::new();
                for completion in pool.submit_batch(jobs).await {
                    ran.push(completion.unwrap().await.unwrap());
                }
                ran
            },
            LIMIT,
        )
        .unwrap();
        assert_eq!(
            ran,
            vec![(0, Some(1)), (1, Some(0)), (2, Some(1)), (3, Some(0))]
        );
        pool.shutdown_blocking().unwrap();
    }

    #[test]
    fn a_batch_larger_than_a_workers_room_hands_over_before_it_waits() {
        let pool = Pool::start(Config {
            inbox_items: 2,
            ..Config::unplaced(1)
        })
        .unwrap();
        let jobs = (0..16u64).map(|n| (Target::Worker(0), move || async move { n }));
        let total = block_on_timeout(
            async {
                let mut total = 0;
                for completion in pool.submit_batch(jobs).await {
                    total += completion.unwrap().await.unwrap();
                }
                total
            },
            LIMIT,
        )
        .expect("the batch finished");
        assert_eq!(total, (0..16).sum::<u64>());
        pool.shutdown_blocking().unwrap();
    }
}
