//! The service against a counting fake with injected latency. Time is
//! nuclei's paused clock, which the test network's timers and the runtime
//! clock both read, so latencies are exact and the tests take no wall time.
//! The inline tests run with no runtime at all.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use defra_core::thread_bounds::MaybeBoxFuture;
use nuclei::time::Instant;

use crate::{Answers, Config, Error, Runtime, Service, Source, Stats};

const LATENCY: Duration = Duration::from_millis(100);
const ENTRY: usize = 8;

/// A remote side that counts its round trips and the most it saw at once.
#[derive(Default)]
struct Fake {
    round_trips: AtomicUsize,
    keys: AtomicUsize,
    in_flight: AtomicUsize,
    most_in_flight: AtomicUsize,
    fail_next: AtomicUsize,
    fail_hard: AtomicBool,
    latency: Duration,
}

impl Fake {
    fn with_latency(latency: Duration) -> Self {
        Self {
            latency,
            ..Self::default()
        }
    }

    fn round_trips(&self) -> usize {
        self.round_trips.load(Ordering::SeqCst)
    }
}

impl Source for Fake {
    type Key = u64;
    /// Keys of one thousand share a group.
    type Group = u64;
    type Value = u64;
    type Error = String;

    fn name(&self) -> &'static str {
        "fake"
    }

    fn group(&self, key: &u64) -> u64 {
        key / 1000
    }

    fn fetch<'a>(
        &'a self,
        _group: &'a u64,
        keys: &'a [u64],
    ) -> MaybeBoxFuture<'a, Answers<u64, String>> {
        Box::pin(async move {
            self.round_trips.fetch_add(1, Ordering::SeqCst);
            self.keys.fetch_add(keys.len(), Ordering::SeqCst);
            let now = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_in_flight.fetch_max(now, Ordering::SeqCst);
            if !self.latency.is_zero() {
                nuclei::time::sleep(self.latency).await;
            }
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            if keys.contains(&666) {
                panic!("the fake panics on 666");
            }
            if self.fail_hard.load(Ordering::SeqCst) {
                return Err("refused".to_string());
            }
            if self
                .fail_next
                .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
            {
                return Err("transient".to_string());
            }
            Ok(keys.iter().map(|key| Ok(key * 2)).collect())
        })
    }

    fn retryable(&self, error: &String) -> bool {
        error == "transient"
    }

    fn cache_bytes(&self, _key: &u64, _value: &u64) -> Option<usize> {
        Some(ENTRY)
    }
}

/// Round trips as tasks of the test's own nuclei runtime, whose time the
/// test pauses: the network a node's edge is, without its threads.
struct TestNetwork;

impl crate::Network for TestNetwork {
    fn spawn(&self, work: MaybeBoxFuture<'static, ()>) {
        nuclei::spawn(work).detach();
    }

    fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()> {
        Box::pin(nuclei::time::sleep(duration))
    }
}

fn service(fake: Fake, config: Config) -> Service<Fake> {
    let runtime = Runtime::new(
        std::sync::Arc::new(TestNetwork),
        std::sync::Arc::new(crate::RuntimeClock::new()),
    );
    Service::new(fake, config, runtime)
}

fn cached(config: Config) -> Config {
    Config {
        cache_bytes: 1 << 20,
        ..config
    }
}

#[nuclei::test(start_paused = true)]
async fn concurrent_identical_requests_make_one_round_trip() {
    let service = service(Fake::with_latency(LATENCY), Config::default());
    let calls: Vec<_> = (0..32)
        .map(|_| {
            let service = service.clone();
            nuclei::spawn(async move { service.get(7).await })
        })
        .collect();
    for call in calls {
        assert_eq!(call.join().await.unwrap(), Ok(14));
    }
    assert_eq!(service.source().round_trips(), 1);
    assert_eq!(Stats::get(&service.stats().joined), 31);
}

#[nuclei::test(start_paused = true)]
async fn requests_that_arrive_in_flight_ride_the_next_round_trip() {
    let service = service(Fake::with_latency(LATENCY), Config::default());
    let first = {
        let service = service.clone();
        nuclei::spawn(async move { service.get(1).await })
    };
    nuclei::time::sleep(LATENCY / 10).await;
    assert_eq!(service.source().round_trips(), 1, "the first is in flight");
    let later: Vec<_> = (2..50)
        .map(|key| {
            let service = service.clone();
            nuclei::spawn(async move { service.get(key).await })
        })
        .collect();
    assert_eq!(first.join().await.unwrap(), Ok(2));
    for (key, call) in (2..50).zip(later) {
        assert_eq!(call.join().await.unwrap(), Ok(key * 2));
    }
    assert_eq!(service.source().round_trips(), 2);
    assert_eq!(service.stats().batch_sizes.max(), 48);
}

#[nuclei::test(start_paused = true)]
async fn time_scales_with_waves_not_items() {
    let service = service(Fake::with_latency(LATENCY), Config::default());
    let started = Instant::now();
    let answers = service.get_many((0..200).collect()).await;
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(service.source().round_trips(), 1);
    assert_eq!(
        started.elapsed(),
        LATENCY,
        "one wave of 200 keys takes one round trip"
    );

    let capped = self::service(
        Fake::with_latency(LATENCY),
        Config {
            max_batch: 64,
            ..Config::default()
        },
    );
    let started = Instant::now();
    let answers = capped.get_many((0..200).collect()).await;
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(
        capped.source().round_trips(),
        4,
        "200 keys in batches of 64"
    );
    assert_eq!(started.elapsed(), LATENCY * 4);
}

#[nuclei::test(start_paused = true)]
async fn a_source_without_batches_runs_its_keys_concurrently_under_the_cap() {
    let service = service(
        Fake::with_latency(LATENCY),
        Config {
            max_batch: 1,
            flights_per_group: 8,
            max_in_flight: 8,
            ..Config::default()
        },
    );
    let started = Instant::now();
    let answers = service.get_many((0..32).collect()).await;
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(service.source().round_trips(), 32);
    assert_eq!(service.source().most_in_flight.load(Ordering::SeqCst), 8);
    assert_eq!(started.elapsed(), LATENCY * 4, "32 keys, 8 at a time");
}

#[nuclei::test(start_paused = true)]
async fn round_trips_in_flight_stay_under_the_cap() {
    let service = service(
        Fake::with_latency(LATENCY),
        Config {
            max_in_flight: 2,
            ..Config::default()
        },
    );
    // Ten groups, so ten lanes would each start a round trip at once.
    let answers = service
        .get_many((0..10).map(|group| group * 1000).collect())
        .await;
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(service.source().round_trips(), 10);
    assert_eq!(service.source().most_in_flight.load(Ordering::SeqCst), 2);
}

#[nuclei::test(start_paused = true)]
async fn cached_answers_make_no_round_trip_until_they_expire() {
    let service = service(
        Fake::with_latency(LATENCY),
        cached(Config {
            ttl: Some(Duration::from_secs(60)),
            ..Config::default()
        }),
    );
    assert_eq!(service.get(3).await, Ok(6));
    assert_eq!(service.get(3).await, Ok(6));
    assert_eq!(service.source().round_trips(), 1);
    assert_eq!(Stats::get(&service.stats().cache_hits), 1);

    nuclei::time::sleep(Duration::from_secs(61)).await;
    assert_eq!(service.cached(&3), None, "expired");
    assert_eq!(service.get(3).await, Ok(6));
    assert_eq!(service.source().round_trips(), 2);
}

#[nuclei::test(start_paused = true)]
async fn an_invalidation_during_a_flight_keeps_its_answer_out_of_the_cache() {
    let service = service(Fake::with_latency(LATENCY), cached(Config::default()));
    let call = {
        let service = service.clone();
        nuclei::spawn(async move { service.get(5).await })
    };
    nuclei::time::sleep(LATENCY / 10).await;
    service.invalidate(|_| true);
    assert_eq!(
        call.join().await.unwrap(),
        Ok(10),
        "the caller still gets its answer"
    );
    assert_eq!(service.cached(&5), None, "but the answer is not cached");

    assert_eq!(service.get(5).await, Ok(10));
    assert_eq!(
        service.cached(&5),
        Some(10),
        "a flight begun after it is cached"
    );
}

#[nuclei::test(start_paused = true)]
async fn the_cache_stays_within_its_byte_budget() {
    let entry = ENTRY + 96;
    let service = service(
        Fake::with_latency(LATENCY),
        Config {
            cache_bytes: 10 * entry,
            ..Config::default()
        },
    );
    service.get_many((0..100).collect()).await;
    assert!(service.cache_bytes() <= 10 * entry);
    assert_eq!(service.cache_bytes(), 10 * entry, "the newest ten stay");
    assert_eq!(service.cached(&99), Some(198));
    assert_eq!(service.cached(&0), None);
}

#[nuclei::test(start_paused = true)]
async fn retryable_failures_are_retried_within_the_attempts() {
    let fake = Fake::with_latency(LATENCY);
    fake.fail_next.store(2, Ordering::SeqCst);
    let service = service(fake, Config::default());
    assert_eq!(service.get(4).await, Ok(8));
    assert_eq!(service.source().round_trips(), 3);
    assert_eq!(Stats::get(&service.stats().retries), 2);

    let fake = Fake::with_latency(LATENCY);
    fake.fail_next.store(5, Ordering::SeqCst);
    let service = self::service(fake, Config::default());
    assert_eq!(
        service.get(4).await,
        Err(Error::Source("transient".to_string()))
    );
    assert_eq!(
        service.source().round_trips(),
        3,
        "attempts bound the retries"
    );
}

#[nuclei::test(start_paused = true)]
async fn a_failure_that_is_not_retryable_is_not_retried() {
    let fake = Fake::with_latency(LATENCY);
    fake.fail_hard.store(true, Ordering::SeqCst);
    let service = service(fake, Config::default());
    assert_eq!(
        service.get(4).await,
        Err(Error::Source("refused".to_string()))
    );
    assert_eq!(service.source().round_trips(), 1);
}

#[nuclei::test(start_paused = true)]
async fn a_caller_gives_up_at_its_deadline() {
    let deadline = Duration::from_secs(1);
    let service = service(
        Fake::with_latency(Duration::from_secs(10)),
        Config {
            deadline,
            ..Config::default()
        },
    );
    let started = Instant::now();
    assert_eq!(service.get(9).await, Err(Error::Deadline(deadline)));
    assert_eq!(started.elapsed(), deadline);
    assert_eq!(Stats::get(&service.stats().deadlines), 1);
}

#[nuclei::test(start_paused = true)]
async fn a_panicking_round_trip_fails_only_its_own_keys() {
    let service = service(Fake::with_latency(LATENCY), Config::default());
    assert_eq!(service.get(666).await, Err(Error::Panicked));
    assert_eq!(service.get(1).await, Ok(2), "the lane goes on");
}

#[nuclei::test(start_paused = true)]
async fn a_remembered_answer_is_served_without_a_round_trip() {
    let service = service(Fake::with_latency(LATENCY), cached(Config::default()));
    assert!(service.remember(12, 24));
    assert_eq!(service.get(12).await, Ok(24));
    assert_eq!(service.source().round_trips(), 0);
}

#[test]
fn backoff_doubles_up_to_its_cap_and_stays_within_its_jitter() {
    let config = Config {
        backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(80),
        ..Config::default()
    };
    let jitter = crate::retry::Jitter::new(1);
    for (retry, full) in [(1, 10), (2, 20), (3, 40), (4, 80), (9, 80)] {
        for _ in 0..64 {
            let wait = jitter.backoff(&config, retry);
            assert!(wait >= Duration::from_millis(full) / 2, "{retry}: {wait:?}");
            assert!(wait <= Duration::from_millis(full), "{retry}: {wait:?}");
        }
    }
}

#[test]
fn a_histogram_bounds_its_quantiles() {
    let histogram = crate::Histogram::default();
    assert_eq!(histogram.quantile(0.5), None);
    for value in 1..=100 {
        histogram.record(value);
    }
    assert_eq!(histogram.count(), 100);
    assert_eq!(histogram.max(), 100);
    let p50 = histogram.quantile(0.5).unwrap();
    assert!((50..=63).contains(&p50), "{p50}");
    assert_eq!(histogram.quantile(1.0), Some(100));
}

/// A network that refuses every task, as a stopping runtime does.
struct Refusing;

impl crate::Network for Refusing {
    fn spawn(&self, work: MaybeBoxFuture<'static, ()>) {
        drop(work);
    }

    fn sleep(&self, duration: Duration) -> MaybeBoxFuture<'static, ()> {
        Box::pin(nuclei::time::sleep(duration))
    }
}

#[nuclei::test(start_paused = true)]
async fn a_refused_round_trip_settles_its_keys_as_closed_and_frees_its_lane() {
    let runtime = Runtime::new(
        std::sync::Arc::new(Refusing),
        std::sync::Arc::new(crate::RuntimeClock::new()),
    );
    let service = Service::new(Fake::with_latency(LATENCY), Config::default(), runtime);
    let started = Instant::now();
    assert_eq!(
        service.get_many(vec![1, 2]).await,
        vec![Err(Error::Closed), Err(Error::Closed)]
    );
    assert_eq!(
        started.elapsed(),
        Duration::ZERO,
        "no caller waits for its deadline"
    );
    assert_eq!(
        service.get(3).await,
        Err(Error::Closed),
        "the lane is not left claimed"
    );
    assert_eq!(service.source().round_trips(), 0);
}

/// Counts how often the future it wraps is polled.
struct Polls<F> {
    future: std::pin::Pin<Box<F>>,
    polls: std::sync::Arc<std::sync::atomic::AtomicUsize>,
}

impl<F> std::future::Future for Polls<F>
where
    F: std::future::Future,
{
    type Output = F::Output;

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<F::Output> {
        self.polls.fetch_add(1, Ordering::SeqCst);
        self.future.as_mut().poll(cx)
    }
}

#[nuclei::test(start_paused = true)]
async fn a_caller_is_woken_once_per_round_trip_not_once_per_key() {
    let service = service(Fake::with_latency(LATENCY), Config::default());
    let polls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let call = Polls {
        future: Box::pin(service.get_many((0..100).collect())),
        polls: std::sync::Arc::clone(&polls),
    };
    let answers = call.await;
    assert!(answers.iter().all(Result::is_ok));
    assert_eq!(service.source().round_trips(), 1);
    assert_eq!(
        polls.load(Ordering::SeqCst),
        2,
        "polled once to start and once when the batch settled"
    );
}

fn inline(fake: Fake, config: Config) -> Service<Fake> {
    Service::new(fake, config, Runtime::inline())
}

/// No runtime exists here: an inline service spawns nothing and reads no
/// timer, so it runs on any executor, a worker's included.
#[test]
fn an_inline_service_runs_in_the_callers_task_with_no_runtime() {
    let service = inline(
        Fake::with_latency(Duration::ZERO),
        cached(Config::default()),
    );
    let answers = futures_lite::future::block_on(service.get_many(vec![1, 2, 3, 2, 1001]));
    assert_eq!(answers, vec![Ok(2), Ok(4), Ok(6), Ok(4), Ok(2002)]);
    assert_eq!(
        service.source().round_trips(),
        2,
        "one round trip per group, a repeated key asked once"
    );
    assert_eq!(futures_lite::future::block_on(service.get(3)), Ok(6));
    assert_eq!(service.source().round_trips(), 2, "the cache answers");
}

#[test]
fn an_inline_retry_follows_its_failure_at_once() {
    let fake = Fake::with_latency(Duration::ZERO);
    fake.fail_next.store(2, Ordering::SeqCst);
    let service = inline(fake, Config::default());
    assert_eq!(futures_lite::future::block_on(service.get(4)), Ok(8));
    assert_eq!(service.source().round_trips(), 3);
}

#[test]
fn an_inline_prefetch_leaves_the_fetch_to_the_call() {
    let service = inline(
        Fake::with_latency(Duration::ZERO),
        cached(Config::default()),
    );
    service.prefetch(vec![5]);
    assert_eq!(service.source().round_trips(), 0);
    assert_eq!(futures_lite::future::block_on(service.get(5)), Ok(10));
    assert_eq!(service.source().round_trips(), 1);
}

#[test]
fn a_round_trip_handed_off_from_a_worker_runs_on_the_network() {
    let pool = workers::Pool::start(workers::Config::unplaced(1)).unwrap();
    let (network, inline) = (Runtime::on_demand(), Runtime::inline());
    let on_worker = network.clone();
    let ran_on_worker = pool
        .submit_and_wait(workers::Target::Worker(0), move || async move {
            (
                on_worker.hand_off(|| async { workers::on_worker() }).await,
                inline.hand_off(|| async { workers::on_worker() }).await,
            )
        })
        .unwrap();
    assert_eq!(
        ran_on_worker,
        (Some(false), Some(true)),
        "the round trip left the worker, the inline one stayed"
    );
    // Off the pool it runs in place.
    let edge = edge::on_demand().unwrap();
    assert_eq!(
        edge.block_on(network.hand_off(|| async { 7 })).unwrap(),
        Some(7)
    );
    pool.shutdown_blocking().unwrap();
}
