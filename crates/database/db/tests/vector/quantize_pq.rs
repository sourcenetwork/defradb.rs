//! Product quantization: codes, reconstruction, and distances from codes.

use db::index::vector::quantize::KMeans;
use db::index::vector::quantize::ProductQuantizer;
use db::index::vector::quantize::Quantizer;
use defra_core::vector::squared_euclidean;

const DIMENSIONS: usize = 16;
const M: usize = 4;

fn sample(count: usize, dimensions: usize, seed: u64) -> Vec<f32> {
    crate::support::Corpus::new(seed)
        .vectors(count, dimensions)
        .into_iter()
        .flatten()
        .collect()
}

fn trained(seed: u64) -> (ProductQuantizer, Vec<f32>) {
    let flat = sample(600, DIMENSIONS, seed);
    let pq = ProductQuantizer::train(&KMeans::new(seed), &flat, DIMENSIONS, M)
        .expect("a divisible width trains");
    (pq, flat)
}

#[test]
fn a_code_is_one_byte_per_subquantizer() {
    let (pq, _) = trained(1);
    assert_eq!(pq.code_len(), M);
    assert_eq!(pq.dimensions(), DIMENSIONS);
    assert_eq!(pq.books().len(), M);
    for book in pq.books() {
        assert_eq!(book.dimensions, DIMENSIONS / M);
    }
}

/// `m` must divide the width, or one subspace would be a different shape and
/// its codebook incomparable with the others.
#[test]
fn an_indivisible_width_is_refused() {
    let flat = sample(100, 10, 2);
    let err = ProductQuantizer::train(&KMeans::new(2), &flat, 10, 4).unwrap_err();
    assert!(
        err.to_string().contains("divide"),
        "the error must say why, got: {err}"
    );
    assert!(ProductQuantizer::train(&KMeans::new(2), &flat, 0, 4).is_err());
    assert!(ProductQuantizer::train(&KMeans::new(2), &flat, 10, 0).is_err());
}

/// Reconstruction is lossy, but it must be much closer to the original than a
/// competing vector is. Otherwise the codes carry no usable signal.
#[test]
fn reconstruction_is_closer_than_a_stranger() {
    let (pq, flat) = trained(3);
    let mut code = vec![0u8; pq.code_len()];
    let mut decoded = vec![0.0f32; DIMENSIONS];

    let mut better = 0usize;
    let total = 200;
    for i in 0..total {
        let vector = &flat[i * DIMENSIONS..(i + 1) * DIMENSIONS];
        let stranger = &flat[(i + 300) * DIMENSIONS..(i + 301) * DIMENSIONS];
        pq.encode(vector, &mut code);
        pq.decode(&code, &mut decoded);

        if squared_euclidean(vector, &decoded) < squared_euclidean(vector, stranger) {
            better += 1;
        }
    }
    assert_eq!(
        better,
        total,
        "reconstruction was worse than an unrelated vector for {} of {total}",
        total - better
    );
}

/// The whole point of the lookup table: the distance it yields must equal the
/// distance to the reconstruction, or the scan is ranking on something else.
#[test]
fn the_lookup_table_agrees_with_the_reconstruction() {
    let (pq, flat) = trained(4);
    let mut code = vec![0u8; pq.code_len()];
    let mut decoded = vec![0.0f32; DIMENSIONS];

    let query = &flat[7 * DIMENSIONS..8 * DIMENSIONS];
    let table = pq.distance_table(query);
    assert_eq!(table.len(), M * 256);

    for i in 0..100 {
        let vector = &flat[i * DIMENSIONS..(i + 1) * DIMENSIONS];
        pq.encode(vector, &mut code);
        pq.decode(&code, &mut decoded);

        let from_table = pq.distance(&table, &code);
        let exact = squared_euclidean(query, &decoded);
        assert!(
            (from_table - exact).abs() <= 1e-4 * exact.max(1.0),
            "table said {from_table}, reconstruction says {exact}"
        );
    }
}

/// Ranking is what the index uses. Against its own codes, the quantizer must
/// put a vector nearest itself.
#[test]
fn a_vector_ranks_nearest_its_own_code() {
    let (pq, flat) = trained(5);
    let mut codes = Vec::new();
    let mut code = vec![0u8; pq.code_len()];
    for i in 0..200 {
        pq.encode(&flat[i * DIMENSIONS..(i + 1) * DIMENSIONS], &mut code);
        codes.push(code.clone());
    }

    let mut correct = 0usize;
    for i in 0..200 {
        let query = &flat[i * DIMENSIONS..(i + 1) * DIMENSIONS];
        let table = pq.distance_table(query);
        let nearest = codes
            .iter()
            .enumerate()
            .min_by(|a, b| {
                pq.distance(&table, a.1)
                    .total_cmp(&pq.distance(&table, b.1))
            })
            .unwrap()
            .0;
        if nearest == i {
            correct += 1;
        }
    }
    // Two vectors can share a code, so this is not required to be perfect; it
    // must be overwhelming or the codebooks are not separating anything.
    assert!(
        correct >= 190,
        "only {correct}/200 vectors ranked nearest their own code"
    );
}

/// More subquantizers means finer codes, so error must fall as `m` rises.
#[test]
fn error_falls_as_subquantizers_rise() {
    let flat = sample(600, DIMENSIONS, 6);
    let mut previous = f64::INFINITY;

    for m in [1usize, 2, 4, 8, 16] {
        let pq = ProductQuantizer::train(&KMeans::new(6), &flat, DIMENSIONS, m).unwrap();
        let mut code = vec![0u8; pq.code_len()];
        let mut decoded = vec![0.0f32; DIMENSIONS];

        let error: f64 = (0..200)
            .map(|i| {
                let vector = &flat[i * DIMENSIONS..(i + 1) * DIMENSIONS];
                pq.encode(vector, &mut code);
                pq.decode(&code, &mut decoded);
                squared_euclidean(vector, &decoded)
            })
            .sum::<f64>()
            / 200.0;

        assert!(
            error <= previous + 1e-6,
            "error rose at m={m}: {error} after {previous}"
        );
        previous = error;
    }
}

/// A quantizer rebuilt from stored codebooks must encode identically, or a
/// reopened index would disagree with the one that wrote it.
#[test]
fn a_quantizer_rebuilt_from_its_codebooks_is_identical() {
    let (pq, flat) = trained(8);
    let rebuilt = ProductQuantizer::from_books(DIMENSIONS, pq.books().to_vec()).unwrap();
    assert_eq!(pq, rebuilt);

    let mut a = vec![0u8; pq.code_len()];
    let mut b = vec![0u8; rebuilt.code_len()];
    for i in 0..100 {
        let vector = &flat[i * DIMENSIONS..(i + 1) * DIMENSIONS];
        pq.encode(vector, &mut a);
        rebuilt.encode(vector, &mut b);
        assert_eq!(a, b);
    }
}

#[test]
fn mismatched_codebooks_are_refused() {
    let (pq, _) = trained(9);
    assert!(ProductQuantizer::from_books(DIMENSIONS, Vec::new()).is_err());
    assert!(ProductQuantizer::from_books(DIMENSIONS + 1, pq.books().to_vec()).is_err());
    let mut short = pq.books().to_vec();
    short[0].values.pop();
    let err = ProductQuantizer::from_books(DIMENSIONS, short).unwrap_err();
    assert!(
        err.to_string().contains("wrong shape"),
        "a codebook with too few values must be refused, got: {err}"
    );
}

/// Compression is the reason this kind exists, so it is asserted rather than
/// assumed.
#[test]
fn a_code_is_far_smaller_than_the_vector() {
    let flat = sample(400, 768, 10);
    let pq = ProductQuantizer::train(&KMeans::new(10), &flat, 768, 96).unwrap();

    let vector_bytes = 768 * size_of::<f32>();
    let code_bytes = pq.code_len();
    assert_eq!(code_bytes, 96);
    assert_eq!(vector_bytes / code_bytes, 32);
}

/// A stored code has to equal a re-encode of its vector, so an encode must pick
/// exactly the centroid `nearest` picks, near-ties included, at every subspace
/// width, prepared or not, and for codebooks smaller than 256 or not a
/// multiple of the lanes.
#[test]
fn encode_picks_the_centroid_nearest_picks() {
    for (dimensions, m, count) in [
        (16, 16, 600),
        (32, 8, 600),
        (96, 12, 600),
        (48, 3, 600),
        (64, 4, 600),
        (60, 4, 100),
        (24, 3, 37),
    ] {
        let flat = sample(count, dimensions, dimensions as u64);
        let pq = ProductQuantizer::train(&KMeans::new(9), &flat, dimensions, m)
            .expect("a divisible width trains");
        let dsub = dimensions / m;
        let mut code = vec![0u8; m];
        let mut probes: Vec<Vec<f32>> = sample(300, dimensions, 77)
            .chunks(dimensions)
            .map(<[f32]>::to_vec)
            .collect();
        let midpoints = (0..m).map(|j| {
            let book = &pq.books()[j];
            let (a, b) = (book.get(0), book.get(book.k - 1));
            a.iter()
                .zip(b)
                .map(|(x, y)| (x + y) / 2.0)
                .collect::<Vec<f32>>()
        });
        probes.push(midpoints.flatten().collect());
        probes.push(
            pq.books()
                .iter()
                .flat_map(|book| book.get(book.k / 2).to_vec())
                .collect(),
        );
        for prepared in [false, true] {
            if prepared {
                pq.prepare_encode();
            }
            for probe in &probes {
                pq.encode(probe, &mut code);
                for (j, book) in pq.books().iter().enumerate() {
                    let (want, _) = book.nearest(&probe[j * dsub..(j + 1) * dsub]);
                    assert_eq!(
                        code[j] as usize, want,
                        "dims {dimensions}, m {m}, k {}, subspace {j}, prepared {prepared}",
                        book.k
                    );
                }
            }
        }
    }
}

/// What `decode` must write: centroid `code[j]` of book `j` into subvector `j`,
/// for every `j` the code and the output both reach, and nothing else.
fn decoded_by_hand(pq: &ProductQuantizer, code: &[u8], out: &mut [f32]) {
    let dsub = pq.dimensions() / pq.code_len();
    for (j, book) in pq.books().iter().enumerate() {
        if j >= code.len() || out.len() < (j + 1) * dsub {
            continue;
        }
        let index = (code[j] as usize).min(book.k - 1);
        out[j * dsub..(j + 1) * dsub].copy_from_slice(book.get(index));
    }
}

/// Every subspace width decodes to exactly the centroids, the widths with a
/// fixed-size copy among them, and a short code or output leaves the rest of
/// the output as it was.
#[test]
fn decode_writes_the_centroids_at_every_width() {
    for (dimensions, m) in [(16, 8), (16, 4), (32, 4), (64, 4), (48, 4), (24, 8)] {
        let flat = sample(300, dimensions, dimensions as u64 + 3);
        let pq = ProductQuantizer::train(&KMeans::new(5), &flat, dimensions, m)
            .expect("a divisible width trains");
        let mut code = vec![0u8; m];
        for i in 0..50 {
            let vector = &flat[i * dimensions..(i + 1) * dimensions];
            pq.encode(vector, &mut code);
            code[i % m] = u8::MAX;
            for (code_len, out_len) in [(m, dimensions), (m - 1, dimensions), (m, dimensions - 1)] {
                let mut got = vec![-1.0f32; out_len];
                let mut want = got.clone();
                pq.decode(&code[..code_len], &mut got);
                decoded_by_hand(&pq, &code[..code_len], &mut want);
                assert_eq!(got, want, "dimensions={dimensions} m={m} vector {i}");
            }
        }
    }
}
