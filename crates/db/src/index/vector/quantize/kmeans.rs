//! Lloyd's k-means with k-means++ seeding, over a resident sample.

use defra_core::vector::squared_euclidean;

/// Lloyd's algorithm with k-means++ seeding.
#[derive(Debug, Clone, Copy)]
pub struct KMeans {
    pub max_iterations: usize,
    pub seed: u64,
}

impl KMeans {
    pub fn new(seed: u64) -> Self {
        Self {
            max_iterations: 25,
            seed,
        }
    }
}

impl Clusterer for KMeans {
    fn fit(&self, sample: &[f32], dimensions: usize, k: usize) -> (Centroids, Fit) {
        train(sample, dimensions, k, self.max_iterations, self.seed)
    }
}

/// Trains `k` centroids over `sample`, a flat `n * dimensions` buffer.
///
/// `k` is clamped to the sample size: asking for more centroids than points
/// would leave empty clusters whose contents are arbitrary.
pub fn train(
    sample: &[f32],
    dimensions: usize,
    k: usize,
    max_iterations: usize,
    seed: u64,
) -> (Centroids, Fit) {
    let n = sample.len().checked_div(dimensions).unwrap_or(0);
    let k = k.min(n).max(1);

    if n == 0 {
        return (
            Centroids {
                k: 0,
                dimensions,
                values: Vec::new(),
            },
            Fit {
                k: 0,
                iterations: 0,
                inertia: 0.0,
            },
        );
    }

    let point = |i: usize| &sample[i * dimensions..(i + 1) * dimensions];
    let mut rng = Rng(seed);
    let mut centroids = seed_plus_plus(sample, dimensions, n, k, &mut rng);

    let mut assignment = vec![usize::MAX; n];
    let mut nearest = vec![0usize; n];
    let mut iterations = 0;
    for _ in 0..max_iterations {
        iterations += 1;

        assign(sample, dimensions, &centroids, &mut nearest);
        let mut moved = nearest != assignment;
        assignment.copy_from_slice(&nearest);

        let mut sums = vec![0.0f64; k * dimensions];
        let mut counts = vec![0usize; k];
        for (i, &c) in assignment.iter().enumerate() {
            counts[c] += 1;
            let at = c * dimensions;
            for (d, value) in point(i).iter().enumerate() {
                sums[at + d] += *value as f64;
            }
        }

        // Distance from each point to its own centroid, only needed to reseed
        // an empty cluster, and computed once however many are empty.
        let mut own: Option<Vec<f32>> = None;
        for (c, count) in counts.iter().enumerate() {
            if *count == 0 {
                // An empty cluster contributes nothing and would stay empty.
                // Reseed it on the point furthest from its own centroid.
                let own = own.get_or_insert_with(|| {
                    let mut own = vec![0.0f32; n];
                    for_each_parallel(&mut own, |i, slot| {
                        *slot = squared_distance(point(i), centroids.get(assignment[i]));
                    });
                    own
                });
                let mut worst = (0usize, -1.0f32);
                for (i, &d) in own.iter().enumerate() {
                    if d > worst.1 {
                        worst = (i, d);
                    }
                }
                let at = c * dimensions;
                centroids.values[at..at + dimensions].copy_from_slice(point(worst.0));
                assignment[worst.0] = c;
                own[worst.0] = 0.0;
                moved = true;
                continue;
            }
            let at = c * dimensions;
            let divisor = *count as f64;
            for d in 0..dimensions {
                centroids.values[at + d] = (sums[at + d] / divisor) as f32;
            }
        }

        if !moved {
            break;
        }
    }

    assign(sample, dimensions, &centroids, &mut nearest);
    let inertia = nearest
        .iter()
        .enumerate()
        .map(|(i, &c)| squared_euclidean(point(i), centroids.get(c)))
        .sum::<f64>()
        / n as f64;

    (
        centroids,
        Fit {
            k,
            iterations,
            inertia,
        },
    )
}

/// k-means++: each seed after the first is drawn with probability proportional
/// to its squared distance from the nearest seed already chosen. Uniform
/// seeding routinely puts two seeds in one cluster and leaves another empty.
fn seed_plus_plus(
    sample: &[f32],
    dimensions: usize,
    n: usize,
    k: usize,
    rng: &mut Rng,
) -> Centroids {
    let point = |i: usize| &sample[i * dimensions..(i + 1) * dimensions];
    let mut values = Vec::with_capacity(k * dimensions);

    let first = (rng.next_u64() % n as u64) as usize;
    values.extend_from_slice(point(first));

    let mut closest = vec![0.0f32; n];
    for_each_parallel(&mut closest, |i, slot| {
        *slot = squared_distance(point(i), point(first));
    });

    for chosen in 1..k {
        let total: f64 = closest.iter().map(|d| f64::from(*d)).sum();
        let picked = if total <= 0.0 || !total.is_finite() {
            // Every remaining point coincides with a seed; any index will do.
            (rng.next_u64() % n as u64) as usize
        } else {
            let mut target = rng.next_unit() * total;
            let mut picked = n - 1;
            for (i, weight) in closest.iter().enumerate() {
                target -= f64::from(*weight);
                if target <= 0.0 {
                    picked = i;
                    break;
                }
            }
            picked
        };

        values.extend_from_slice(point(picked));
        let at = chosen * dimensions;
        let seed = &values[at..at + dimensions];
        for_each_parallel(&mut closest, |i, nearest| {
            let d = squared_distance(point(i), seed);
            if d < *nearest {
                *nearest = d;
            }
        });
    }

    Centroids {
        k,
        dimensions,
        values,
    }
}

/// Points below this count are assigned on the calling thread: spawning costs
/// more than the work.
const PARALLEL_MIN: usize = 4_096;

/// Writes each point's nearest centroid into `out`.
///
/// Ranks by `|c|^2 - 2 x.c`, which orders centroids exactly as the squared
/// distance does (`|x|^2` is shared), so each comparison is one f32 dot
/// product. Training is the one place this runs `n * k` times; the shared
/// kernel widens to f64 for ranking parity, which training does not need.
fn assign(sample: &[f32], dimensions: usize, centroids: &Centroids, out: &mut [usize]) {
    let norms: Vec<f32> = (0..centroids.k)
        .map(|c| dot(centroids.get(c), centroids.get(c)))
        .collect();
    for_each_parallel(out, |i, slot| {
        let point = &sample[i * dimensions..(i + 1) * dimensions];
        let mut best = (0usize, f32::INFINITY);
        for (c, norm) in norms.iter().enumerate() {
            let score = norm - 2.0 * dot(point, centroids.get(c));
            if score < best.1 {
                best = (c, score);
            }
        }
        *slot = best.0;
    });
}

/// Runs `work(index, slot)` over every slot, split across the machine's
/// threads. Each slot depends only on its index, so the result is the same
/// however the slots are split. A target without threads runs serially.
///
/// The calling thread drains the same chunk queue as the spawned ones, so a
/// thread the OS refuses to create leaves fewer workers rather than a panic.
fn for_each_parallel<T: Send>(slots: &mut [T], work: impl Fn(usize, &mut T) + Sync) {
    let threads = std::thread::available_parallelism().map_or(1, |n| n.get());
    if threads == 1 || slots.len() < PARALLEL_MIN {
        for (i, slot) in slots.iter_mut().enumerate() {
            work(i, slot);
        }
        return;
    }
    let per = slots.len().div_ceil(threads);
    let run = |(chunk, part): (usize, &mut [T])| {
        for (i, slot) in part.iter_mut().enumerate() {
            work(chunk * per + i, slot);
        }
    };
    let chunks = regolith::sync::ArrayQueue::new(slots.len().div_ceil(per));
    for chunk in slots.chunks_mut(per).enumerate() {
        // The queue is sized for every chunk; one that did not fit runs here.
        if let Err(chunk) = chunks.push(chunk) {
            run(chunk);
        }
    }
    let drain = || {
        while let Some(chunk) = chunks.pop() {
            run(chunk);
        }
    };
    std::thread::scope(|scope| {
        for _ in 1..threads {
            if std::thread::Builder::new()
                .spawn_scoped(scope, drain)
                .is_err()
            {
                break;
            }
        }
        drain();
    });
}

/// Independent accumulators, so the compiler can keep several SIMD lanes busy
/// without reassociating one float sum.
pub(super) const LANES: usize = 16;

fn dot(a: &[f32], b: &[f32]) -> f32 {
    let ((x, x_tail), (y, y_tail)) = (a.as_chunks::<LANES>(), b.as_chunks::<LANES>());
    let mut acc = [0.0f32; LANES];
    for (p, q) in x.iter().zip(y) {
        for lane in 0..LANES {
            acc[lane] += p[lane] * q[lane];
        }
    }
    let tail: f32 = x_tail.iter().zip(y_tail).map(|(p, q)| p * q).sum();
    acc.iter().sum::<f32>() + tail
}

fn squared_distance(a: &[f32], b: &[f32]) -> f32 {
    let ((x, x_tail), (y, y_tail)) = (a.as_chunks::<LANES>(), b.as_chunks::<LANES>());
    let mut acc = [0.0f32; LANES];
    for (p, q) in x.iter().zip(y) {
        for lane in 0..LANES {
            let d = p[lane] - q[lane];
            acc[lane] += d * d;
        }
    }
    let tail: f32 = x_tail
        .iter()
        .zip(y_tail)
        .map(|(p, q)| (p - q) * (p - q))
        .sum();
    acc.iter().sum::<f32>() + tail
}

struct Rng(u64);

impl Rng {
    fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn next_unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// Trained centroids, flat: `k * dimensions`.
#[derive(Debug, Clone, PartialEq)]
pub struct Centroids {
    pub k: usize,
    pub dimensions: usize,
    pub values: Vec<f32>,
}

impl Centroids {
    pub fn get(&self, index: usize) -> &[f32] {
        let at = index * self.dimensions;
        &self.values[at..at + self.dimensions]
    }

    /// The nearest centroid to `vector`, and its squared distance.
    ///
    /// Build, insert and update all assign lists through this one function,
    /// so they agree even on a near-tie: an update finds the entry to remove
    /// by assigning the old vector again.
    pub fn nearest(&self, vector: &[f32]) -> (usize, f64) {
        let mut best = (0usize, f32::INFINITY);
        for index in 0..self.k {
            let distance = squared_distance(vector, self.get(index));
            if distance < best.1 {
                best = (index, distance);
            }
        }
        (best.0, f64::from(best.1))
    }
}

/// How the fit went. Reported rather than assumed, because a sample smaller
/// than `k` cannot produce `k` meaningful centroids and the caller has to know.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fit {
    /// Centroids actually produced, never more than the distinct sample size.
    pub k: usize,
    /// Iterations run before assignments stopped changing.
    pub iterations: usize,
    /// Mean squared distance from a sample point to its centroid.
    pub inertia: f64,
}

pub trait Clusterer {
    fn fit(&self, sample: &[f32], dimensions: usize, k: usize) -> (Centroids, Fit);
}
