//! A service: one input kind's remote side behind single-flight, batching, a
//! byte-bounded cache, deadlines, retries, an in-flight cap and metrics.
//!
//! A caller's keys go through the cache, then the flight map: a key already
//! in flight is joined, a new one gets a flight and waits in its group's
//! lane. Round trips run as tasks on the service's network, each taking the
//! oldest waiting keys of a lane, so keys that arrive while one is in flight
//! ride the next. Callers suspend on their flights' events until the answers
//! come or their deadline passes; giving up cancels nothing others share.

use std::hash::BuildHasher;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures_lite::FutureExt;
use rapidhash::fast::{RandomState, RapidHashSet};
use regolith::sync::{HopscotchMap, Semaphore};

use crate::cache::Cache;
use crate::config::Config;
use crate::error::Error;
use crate::flight::{Answer, Flight};
use crate::lane::Lane;
use crate::network::Runtime;
use crate::retry::Jitter;
use crate::source::Source;
use crate::stats::Stats;

type ResultOf<S> = Result<<S as Source>::Value, Error<<S as Source>::Error>>;
type FlightOf<S> = Arc<Flight<<S as Source>::Value, <S as Source>::Error>>;
type LaneOf<S> = Arc<Lane<<S as Source>::Key>>;
/// What one round trip came back with: an answer per key, or why none came.
type OutcomeOf<S> =
    Result<Vec<Result<<S as Source>::Value, <S as Source>::Error>>, Error<<S as Source>::Error>>;
/// One inline call's keys by group, each with its place in the call.
type GroupsOf<S> = Vec<(<S as Source>::Group, Vec<(usize, <S as Source>::Key)>)>;

/// One input kind's service. Cloning shares it.
pub struct Service<S>
where
    S: Source,
{
    inner: Arc<Inner<S>>,
}

impl<S> Clone for Service<S>
where
    S: Source,
{
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct Inner<S>
where
    S: Source,
{
    source: S,
    config: Config,
    runtime: Runtime,
    flights: HopscotchMap<S::Key, FlightOf<S>, RandomState>,
    lanes: HopscotchMap<S::Group, LaneOf<S>, RandomState>,
    cache: Cache<S::Key, S::Value>,
    in_flight: Semaphore,
    trips: AtomicU64,
    jitter: Jitter,
    stats: Stats,
}

enum Slot<S>
where
    S: Source,
{
    Ready(ResultOf<S>),
    Waiting(FlightOf<S>),
}

impl<S> Service<S>
where
    S: Source,
{
    /// A service over `source`, running its round trips on `runtime`.
    pub fn new(source: S, config: Config, runtime: Runtime) -> Self {
        let config = config.normalized();
        let seed = RandomState::default().hash_one(runtime.now());
        Self {
            inner: Arc::new(Inner {
                source,
                cache: Cache::new(config.cache_bytes, config.ttl),
                in_flight: Semaphore::new(config.max_in_flight),
                config,
                runtime,
                flights: HopscotchMap::with_hasher(RandomState::default()),
                lanes: HopscotchMap::with_hasher(RandomState::default()),
                trips: AtomicU64::new(0),
                jitter: Jitter::new(seed),
                stats: Stats::default(),
            }),
        }
    }

    /// `key`'s answer.
    pub async fn get(&self, key: S::Key) -> ResultOf<S> {
        self.get_many(vec![key])
            .await
            .into_iter()
            .next()
            .unwrap_or(Err(Error::Answers {
                expected: 1,
                got: 0,
            }))
    }

    /// The answers for `keys`, in their order. Keys not cached or in flight
    /// go out together, in as few round trips as the batch cap allows.
    pub async fn get_many(&self, keys: Vec<S::Key>) -> Vec<ResultOf<S>> {
        let inner = &self.inner;
        if inner.runtime.network.is_none() {
            return inner.inline(keys).await;
        }
        let start = inner.runtime.now();
        Stats::add(&inner.stats.requests, keys.len() as u64);
        let mut touched: Vec<(S::Group, LaneOf<S>)> = Vec::new();
        let slots: Vec<Slot<S>> = keys
            .into_iter()
            .map(|key| inner.slot(key, start, &mut touched))
            .collect();
        for (group, lane) in touched {
            inner.pump(group, lane);
        }
        inner.wait(slots, start).await
    }

    /// Starts the round trips `keys` need, without waiting for them: their
    /// answers land in the cache (where the source allows), and a later
    /// [`get_many`](Self::get_many) of a key still in flight joins it.
    /// Inline, with nothing to run them in the background, it does nothing:
    /// the later call fetches them.
    pub fn prefetch(&self, keys: Vec<S::Key>) {
        let inner = &self.inner;
        if inner.runtime.network.is_none() {
            return;
        }
        let now = inner.runtime.now();
        Stats::add(&inner.stats.requests, keys.len() as u64);
        let mut touched: Vec<(S::Group, LaneOf<S>)> = Vec::new();
        for key in keys {
            inner.slot(key, now, &mut touched);
        }
        for (group, lane) in touched {
            inner.pump(group, lane);
        }
    }

    /// `key`'s cached answer, without asking the remote side.
    pub fn cached(&self, key: &S::Key) -> Option<S::Value> {
        self.inner.cache.get(key, self.inner.runtime.now())
    }

    /// Caches an answer learned another way (a key this node just made),
    /// when the source allows caching it.
    pub fn remember(&self, key: S::Key, value: S::Value) -> bool {
        let inner = &self.inner;
        let Some(bytes) = inner.source.cache_bytes(&key, &value) else {
            return false;
        };
        let generation = inner.cache.generation();
        inner
            .cache
            .insert(key, value, bytes, inner.runtime.now(), generation)
    }

    /// Drops every cached answer whose key matches, and keeps answers
    /// fetched in the meantime from being cached. Returns how many.
    pub fn invalidate<F>(&self, matches: F) -> usize
    where
        F: Fn(&S::Key) -> bool,
    {
        self.inner.cache.invalidate(matches)
    }

    /// The service's metrics.
    pub fn stats(&self) -> &Stats {
        &self.inner.stats
    }

    /// The runtime its round trips run on.
    pub fn runtime(&self) -> &Runtime {
        &self.inner.runtime
    }

    /// The bytes the cache holds now.
    pub fn cache_bytes(&self) -> usize {
        self.inner.cache.bytes()
    }

    /// The remote side.
    pub fn source(&self) -> &S {
        &self.inner.source
    }

    /// The bounds the service runs with.
    pub fn config(&self) -> &Config {
        &self.inner.config
    }
}

impl<S> Inner<S>
where
    S: Source,
{
    /// Where `key`'s answer comes from: the cache, a flight in progress, or
    /// a new flight queued in its group's lane.
    fn slot(&self, key: S::Key, now: u64, touched: &mut Vec<(S::Group, LaneOf<S>)>) -> Slot<S> {
        if let Some(value) = self.cache.get(&key, now) {
            Stats::add(&self.stats.cache_hits, 1);
            return Slot::Ready(Ok(value));
        }
        let fresh: FlightOf<S> = Arc::new(Flight::new(self.cache.generation()));
        if let Some(existing) = self
            .flights
            .insert_if_absent(key.clone(), Arc::clone(&fresh))
        {
            Stats::add(&self.stats.joined, 1);
            return Slot::Waiting(existing);
        }
        // Cached between the probe and the claim: settle the new flight
        // with it, for anyone who joined it meanwhile.
        if let Some(value) = self.cache.get(&key, now) {
            fresh.settle(Answer {
                result: Ok(value.clone()),
                trip: 0,
            });
            self.flights
                .remove_if(&key, |flight| Arc::ptr_eq(flight, &fresh));
            Stats::add(&self.stats.cache_hits, 1);
            return Slot::Ready(Ok(value));
        }
        let group = self.source.group(&key);
        let lane = self
            .lanes
            .get_or_insert(group.clone(), Arc::new(Lane::new()));
        lane.push(key);
        if !touched.iter().any(|(_, seen)| Arc::ptr_eq(seen, &lane)) {
            touched.push((group, lane));
        }
        Slot::Waiting(fresh)
    }

    /// Starts round trips for `lane`'s waiting keys, as many as its slots
    /// and its batches allow. The keys a caller pushed are always driven:
    /// either here, or by a round trip in flight that takes them next.
    fn pump(self: &Arc<Self>, group: S::Group, lane: LaneOf<S>) {
        let batches = lane.waiting().div_ceil(self.config.max_batch).max(1);
        for _ in 0..batches {
            if !lane.claim(self.config.flights_per_group) {
                return;
            }
            let driving = Driving {
                inner: Arc::clone(self),
                group: group.clone(),
                lane: Arc::clone(&lane),
                holding: true,
                started: false,
                batch: Vec::new(),
            };
            match &self.runtime.network {
                Some(network) => network.spawn(Box::pin(driving.run())),
                // Never reached: an inline service runs no lanes. Dropped, the
                // claim settles its lane's keys as closed.
                None => drop(driving),
            }
        }
    }

    /// One batch's round trips: the first, then retries while the source
    /// allows them and the deadline leaves time.
    async fn round_trip(
        &self,
        group: &S::Group,
        keys: &[S::Key],
    ) -> Result<Vec<Result<S::Value, S::Error>>, Error<S::Error>> {
        let started = self.runtime.now();
        let budget = micros(self.config.deadline);
        Stats::add(&self.stats.fetched, keys.len() as u64);
        self.stats.batch_sizes.record(keys.len() as u64);
        let mut attempt = 0;
        loop {
            attempt += 1;
            let remaining = budget.saturating_sub(self.runtime.now().saturating_sub(started));
            if remaining == 0 {
                return Err(Error::Deadline(self.config.deadline));
            }
            Stats::add(&self.stats.round_trips, 1);
            if attempt > 1 {
                Stats::add(&self.stats.retries, 1);
            }
            let sent = self.runtime.now();
            let fetch = AssertUnwindSafe(self.source.fetch(group, keys)).catch_unwind();
            let outcome = match &self.runtime.network {
                Some(network) => {
                    let timer = network.sleep(Duration::from_micros(remaining));
                    async { Some(fetch.await) }
                        .or(async {
                            timer.await;
                            None
                        })
                        .await
                }
                None => Some(fetch.await),
            };
            self.stats
                .round_trip_micros
                .record(self.runtime.now().saturating_sub(sent));
            let error = match outcome {
                None => return Err(Error::Deadline(self.config.deadline)),
                Some(Err(_panic)) => {
                    tracing::warn!(
                        input = self.source.name(),
                        "a remote input request panicked"
                    );
                    return Err(Error::Panicked);
                }
                Some(Ok(Ok(answers))) if answers.len() == keys.len() => return Ok(answers),
                Some(Ok(Ok(answers))) => {
                    return Err(Error::Answers {
                        expected: keys.len(),
                        got: answers.len(),
                    })
                }
                Some(Ok(Err(error))) => error,
            };
            let left = budget.saturating_sub(self.runtime.now().saturating_sub(started));
            if attempt >= self.config.attempts || !self.source.retryable(&error) || left == 0 {
                return Err(Error::Source(error));
            }
            let wait = self
                .jitter
                .backoff(&self.config, attempt)
                .min(Duration::from_micros(left));
            tracing::debug!(
                input = self.source.name(),
                attempt,
                wait_micros = wait.as_micros() as u64,
                %error,
                "remote input request failed; retrying"
            );
            if let Some(network) = &self.runtime.network {
                network.sleep(wait).await;
            }
        }
    }

    /// Settles a round trip's keys in two passes: every answer is recorded
    /// (and cached) first, then every flight is released, so a waiter woken
    /// by any release of the batch finds the whole batch answered.
    fn deliver(&self, keys: Vec<S::Key>, outcome: OutcomeOf<S>, trip: u64) {
        let answers = self.answers(keys.len(), outcome);
        self.settle_all(keys, answers, trip);
    }

    /// One answer per key of a round trip that ended with `outcome`.
    fn answers(&self, count: usize, outcome: OutcomeOf<S>) -> Vec<ResultOf<S>> {
        match outcome {
            Ok(answers) => answers
                .into_iter()
                .map(|answer| answer.map_err(Error::Source))
                .collect(),
            Err(error) => {
                Stats::add(&self.stats.failures, 1);
                tracing::debug!(
                    input = self.source.name(),
                    keys = count,
                    %error,
                    "remote input request ended without answers"
                );
                (0..count).map(|_| Err(error.clone())).collect()
            }
        }
    }

    /// Caches `key`'s answer where the source allows, unless an invalidation
    /// began since `generation`.
    fn cache_answer(&self, key: &S::Key, result: &ResultOf<S>, now: u64, generation: u64) {
        let Ok(value) = result else {
            return;
        };
        if let Some(bytes) = self.source.cache_bytes(key, value) {
            if self
                .cache
                .insert(key.clone(), value.clone(), bytes, now, generation)
            {
                Stats::add(&self.stats.cached, 1);
            }
        }
    }

    /// An inline call: the keys not cached go out from this task, batched by
    /// group, with nothing spawned and no timer read. A key another inline
    /// call is fetching is waited for, not asked again; if that call is
    /// dropped before its answer comes, the key is fetched here.
    async fn inline(&self, keys: Vec<S::Key>) -> Vec<ResultOf<S>> {
        let start = self.runtime.now();
        Stats::add(&self.stats.requests, keys.len() as u64);
        let mut results: Vec<Option<ResultOf<S>>> = (0..keys.len()).map(|_| None).collect();
        let mut pending: Vec<(usize, S::Key)> = keys.into_iter().enumerate().collect();
        let mut trips = 0u64;
        while !pending.is_empty() {
            let generation = self.cache.generation();
            let mut owned = Owned {
                inner: self,
                flights: Vec::new(),
            };
            let mut joined = Vec::new();
            let mut groups: GroupsOf<S> = Vec::new();
            for (index, key) in pending.drain(..) {
                if let Some(value) = self.cache.get(&key, start) {
                    Stats::add(&self.stats.cache_hits, 1);
                    results[index] = Some(Ok(value));
                    continue;
                }
                let fresh: FlightOf<S> = Arc::new(Flight::new(generation));
                if let Some(existing) = self
                    .flights
                    .insert_if_absent(key.clone(), Arc::clone(&fresh))
                {
                    Stats::add(&self.stats.joined, 1);
                    joined.push((index, key, existing));
                    continue;
                }
                owned.flights.push((key.clone(), fresh));
                let group = self.source.group(&key);
                match groups.iter_mut().find(|(seen, _)| *seen == group) {
                    Some((_, members)) => members.push((index, key)),
                    None => groups.push((group, vec![(index, key)])),
                }
            }
            for (group, members) in &groups {
                for batch in members.chunks(self.config.max_batch) {
                    let keys: Vec<S::Key> = batch.iter().map(|(_, key)| key.clone()).collect();
                    let trip = self.trips.fetch_add(1, Ordering::Relaxed) + 1;
                    trips += 1;
                    let outcome = {
                        let _permit = self.in_flight.acquire(1).await;
                        self.round_trip(group, &keys).await
                    };
                    let answers = self.answers(keys.len(), outcome);
                    for ((index, _), answer) in batch.iter().zip(&answers) {
                        results[*index] = Some(answer.clone());
                    }
                    self.settle_all(keys, answers, trip);
                }
            }
            drop(owned);
            for (index, key, flight) in joined {
                flight.settled().await;
                match flight.answer().map(|answer| &answer.result) {
                    // Its fetcher was dropped before the answer came.
                    None | Some(Err(Error::Closed)) => pending.push((index, key)),
                    Some(result) => results[index] = Some(result.clone()),
                }
            }
        }
        self.stats
            .wait_micros
            .record(self.runtime.now().saturating_sub(start));
        self.stats.trips_per_call.record(trips);
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or(Err(Error::Answers {
                    expected: 1,
                    got: 0,
                }))
            })
            .collect()
    }

    fn settle_all(&self, keys: Vec<S::Key>, answers: Vec<ResultOf<S>>, trip: u64) {
        let now = self.runtime.now();
        let mut settled = Vec::with_capacity(keys.len());
        for (key, result) in keys.into_iter().zip(answers) {
            let Some(flight) = self.flights.get(&key) else {
                continue;
            };
            self.cache_answer(&key, &result, now, flight.generation);
            flight.record(Answer { result, trip });
            settled.push((key, flight));
        }
        // Out of the map before anyone wakes: a caller that has its answer
        // and asks again starts a fresh flight, and never joins this settled
        // one for an answer that may not be kept (a decision with no state).
        for (key, flight) in settled {
            self.flights
                .remove_if(&key, |current| Arc::ptr_eq(current, &flight));
            flight.release();
        }
    }

    /// Waits for every slot's answer, together bounded by the deadline.
    ///
    /// A round trip settles its keys in the order the caller queued them, so
    /// the caller suspends on its last unsettled flight first: it is woken
    /// once per round trip, when the whole batch is settled, never once per
    /// key. A worker waiting here holds no thread and runs its other tasks.
    async fn wait(&self, slots: Vec<Slot<S>>, start: u64) -> Vec<ResultOf<S>> {
        let mut timer = None;
        let mut expired = false;
        for slot in slots.iter().rev() {
            let Slot::Waiting(flight) = slot else {
                continue;
            };
            if expired || flight.answer().is_some() {
                continue;
            }
            let Some(network) = &self.runtime.network else {
                flight.settled().await;
                continue;
            };
            let left = micros(self.config.deadline)
                .saturating_sub(self.runtime.now().saturating_sub(start));
            let timer = timer.get_or_insert_with(|| network.sleep(Duration::from_micros(left)));
            let settled = async {
                flight.settled().await;
                true
            }
            .or(async {
                timer.await;
                false
            })
            .await;
            expired = !settled;
        }
        let mut trips = RapidHashSet::default();
        let results = slots
            .into_iter()
            .map(|slot| match slot {
                Slot::Ready(result) => result,
                Slot::Waiting(flight) => match flight.answer() {
                    Some(answer) => {
                        if answer.trip != 0 {
                            trips.insert(answer.trip);
                        }
                        answer.result.clone()
                    }
                    None => {
                        Stats::add(&self.stats.deadlines, 1);
                        Err(Error::Deadline(self.config.deadline))
                    }
                },
            })
            .collect();
        self.stats
            .wait_micros
            .record(self.runtime.now().saturating_sub(start));
        self.stats.trips_per_call.record(trips.len() as u64);
        results
    }
}

fn micros(duration: Duration) -> u64 {
    u64::try_from(duration.as_micros()).unwrap_or(u64::MAX)
}

/// One round-trip slot of a lane, claimed before its task is spawned.
///
/// Dropped mid-round-trip (its runtime stopping), it settles the batch it
/// carried as [`Error::Closed`] and frees its slot. Dropped before it ever
/// ran (a runtime that refused the task), it also settles the keys waiting in
/// its lane, which no round trip would take. No waiter or lane is left
/// behind.
struct Driving<S>
where
    S: Source,
{
    inner: Arc<Inner<S>>,
    group: S::Group,
    lane: LaneOf<S>,
    holding: bool,
    started: bool,
    batch: Vec<S::Key>,
}

impl<S> Driving<S>
where
    S: Source,
{
    /// Takes the lane's oldest waiting keys, makes their round trip, settles
    /// their flights, and goes on while keys wait.
    async fn run(mut self) {
        self.started = true;
        let inner = Arc::clone(&self.inner);
        loop {
            self.batch = self.lane.take(inner.config.max_batch);
            if self.batch.is_empty() {
                self.holding = false;
                self.lane.release();
                // A key pushed after the take saw the slot taken and left
                // its round trip to this one.
                if self.lane.has_waiting() && self.lane.claim(inner.config.flights_per_group) {
                    self.holding = true;
                    continue;
                }
                if self.lane.is_idle() {
                    let lane = &self.lane;
                    inner.lanes.remove_if(&self.group, |current| {
                        Arc::ptr_eq(current, lane) && lane.is_idle()
                    });
                }
                return;
            }
            let trip = inner.trips.fetch_add(1, Ordering::Relaxed) + 1;
            let outcome = {
                let _permit = inner.in_flight.acquire(1).await;
                inner.round_trip(&self.group, &self.batch).await
            };
            let batch = std::mem::take(&mut self.batch);
            inner.deliver(batch, outcome, trip);
        }
    }
}

impl<S> Drop for Driving<S>
where
    S: Source,
{
    fn drop(&mut self) {
        let mut closed = std::mem::take(&mut self.batch);
        if self.holding {
            self.lane.release();
        }
        if !self.started {
            closed.extend(self.lane.take(usize::MAX));
        }
        let answers = closed.iter().map(|_| Err(Error::Closed)).collect();
        self.inner.settle_all(closed, answers, 0);
    }
}

/// The flights an inline call registered and has not settled yet: dropped
/// before their answers came (the call cancelled), it settles them as closed,
/// so whoever joined them fetches the keys instead of waiting for ever.
struct Owned<'a, S>
where
    S: Source,
{
    inner: &'a Inner<S>,
    flights: Vec<(S::Key, FlightOf<S>)>,
}

impl<S> Drop for Owned<'_, S>
where
    S: Source,
{
    fn drop(&mut self) {
        for (key, flight) in self.flights.drain(..) {
            if flight.answer().is_some() {
                continue;
            }
            flight.settle(Answer {
                result: Err(Error::Closed),
                trip: 0,
            });
            self.inner
                .flights
                .remove_if(&key, |current| Arc::ptr_eq(current, &flight));
        }
    }
}
