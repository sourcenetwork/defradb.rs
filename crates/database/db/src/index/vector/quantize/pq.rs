//! Product quantization: a vector as `m` bytes.

use super::kmeans::{self, Centroids, Clusterer};
use crate::index::error::{Error, Result};
use defra_core::vector::squared_euclidean;
use regolith::sync::OnceCell;

/// Codebook entries per subquantizer. `nbits = 8`, so a code component is a
/// `u8` and a list scan is a byte lookup; that is the whole reason the scan is
/// cheap, and why other widths are out of scope.
pub const CODEBOOK_SIZE: usize = 256;

/// Splits a vector into `m` subvectors and quantizes each against its own
/// codebook, so `m` bytes stand in for `dimensions` floats.
///
/// At 768 dimensions and `m = 96` that is 96 bytes against 3,072: **32x**.
#[derive(Debug)]
pub struct ProductQuantizer {
    dimensions: usize,
    m: usize,
    dsub: usize,
    /// `m` codebooks, each `CODEBOOK_SIZE * dsub` values.
    books: Vec<Centroids>,
    /// Each codebook again, dimension-major: `dsub` rows of `k` values. Built
    /// only by [`prepare_encode`](Self::prepare_encode): the copy costs about
    /// as much as one encode, so only a quantizer that encodes many vectors
    /// earns it back.
    columns: OnceCell<Vec<Vec<f32>>>,
}

impl Clone for ProductQuantizer {
    fn clone(&self) -> Self {
        Self {
            dimensions: self.dimensions,
            m: self.m,
            dsub: self.dsub,
            books: self.books.clone(),
            columns: OnceCell::new(),
        }
    }
}

impl PartialEq for ProductQuantizer {
    fn eq(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions && self.books == other.books
    }
}

/// Centroids an encode compares at once, one per lane.
const ENCODE_LANES: usize = 16;

/// Transposes `book` so that one dimension of every centroid is contiguous.
fn columns(book: &Centroids) -> Vec<f32> {
    let mut out = vec![0.0f32; book.k * book.dimensions];
    for c in 0..book.k {
        for (d, value) in book.get(c).iter().enumerate() {
            out[d * book.k + c] = *value;
        }
    }
    out
}

/// [`Centroids::nearest`] for a subspace narrower than `kmeans::LANES`,
/// sixteen centroids at a time. There `nearest` sums each centroid's terms one
/// after another; so does this, in the same order, and ties go to the lower
/// index, so the code is bit-identical: a stored code must equal a re-encode
/// of its vector.
fn nearest_by_columns(columns: &[f32], k: usize, part: &[f32]) -> usize {
    let mut best = (0usize, f32::INFINITY);
    let mut c0 = 0;
    while c0 < k {
        let width = ENCODE_LANES.min(k - c0);
        let mut acc = [0.0f32; ENCODE_LANES];
        for (d, x) in part.iter().enumerate() {
            let row = &columns[d * k + c0..d * k + c0 + width];
            for (sum, q) in acc.iter_mut().zip(row) {
                let diff = x - q;
                *sum += diff * diff;
            }
        }
        for (lane, distance) in acc[..width].iter().enumerate() {
            if *distance < best.1 {
                best = (c0 + lane, *distance);
            }
        }
        c0 += width;
    }
    best.0
}

impl ProductQuantizer {
    /// Trains one codebook per subspace over `sample`, a flat
    /// `n * dimensions` buffer.
    ///
    /// `m` must divide `dimensions`: a ragged split would leave one subspace a
    /// different width and its codebook incomparable with the others.
    pub fn train<C: Clusterer>(
        clusterer: &C,
        sample: &[f32],
        dimensions: usize,
        m: usize,
    ) -> Result<Self> {
        if dimensions == 0 || m == 0 {
            return Err(Error::Other(
                "product quantizer needs a non-zero width and subquantizer count".into(),
            ));
        }
        if !dimensions.is_multiple_of(m) {
            return Err(Error::Other(format!(
                "product quantizer needs m to divide the width: {m} does not divide {dimensions}"
            )));
        }

        let dsub = dimensions / m;
        let n = sample.len() / dimensions;
        let mut books = Vec::with_capacity(m);

        let mut subspace = vec![0.0f32; n * dsub];
        for j in 0..m {
            for i in 0..n {
                let from = i * dimensions + j * dsub;
                subspace[i * dsub..(i + 1) * dsub].copy_from_slice(&sample[from..from + dsub]);
            }
            let (centroids, _) = clusterer.fit(&subspace, dsub, CODEBOOK_SIZE);
            books.push(centroids);
        }

        Ok(Self {
            dimensions,
            m,
            dsub,
            columns: OnceCell::new(),
            books,
        })
    }

    /// Rebuilds from stored codebooks.
    pub fn from_books(dimensions: usize, books: Vec<Centroids>) -> Result<Self> {
        let m = books.len();
        if m == 0 || !dimensions.is_multiple_of(m) {
            return Err(Error::Other(
                "product quantizer codebooks do not divide the width".into(),
            ));
        }
        let dsub = dimensions / m;
        if books.iter().any(|book| book.dimensions != dsub) {
            return Err(Error::Other(
                "a product quantizer codebook has the wrong subspace width".into(),
            ));
        }
        Ok(Self {
            dimensions,
            m,
            dsub,
            columns: OnceCell::new(),
            books,
        })
    }

    pub fn books(&self) -> &[Centroids] {
        &self.books
    }

    pub fn m(&self) -> usize {
        self.m
    }

    /// Readies narrow subspaces to encode sixteen centroids at a time. Called
    /// by whoever is about to encode many vectors with this quantizer, or
    /// keeps it across writes; a quantizer built for one write skips it, since
    /// the copy would cost as much as the encode it speeds up.
    pub fn prepare_encode(&self) {
        if self.dsub < kmeans::LANES {
            self.columns
                .get_or_init_racy(|| self.books.iter().map(columns).collect());
        }
    }

    fn subspace<'v>(&self, vector: &'v [f32], j: usize) -> &'v [f32] {
        &vector[j * self.dsub..(j + 1) * self.dsub]
    }
}

impl Quantizer for ProductQuantizer {
    fn code_len(&self) -> usize {
        self.m
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn encode(&self, vector: &[f32], code: &mut [u8]) {
        let transposed = self.columns.get();
        for j in 0..self.m {
            let book = &self.books[j];
            if book.k == 0 || vector.len() < (j + 1) * self.dsub || code.len() <= j {
                continue;
            }
            let part = self.subspace(vector, j);
            let nearest = match transposed {
                Some(transposed) => nearest_by_columns(&transposed[j], book.k, part),
                None => book.nearest(part).0,
            };
            code[j] = nearest as u8;
        }
    }

    fn decode(&self, code: &[u8], out: &mut [f32]) {
        for j in 0..self.m {
            let book = &self.books[j];
            if book.k == 0 || code.len() <= j || out.len() < (j + 1) * self.dsub {
                continue;
            }
            let index = (code[j] as usize).min(book.k - 1);
            out[j * self.dsub..(j + 1) * self.dsub].copy_from_slice(book.get(index));
        }
    }

    /// `m * CODEBOOK_SIZE` partial squared distances, computed once per query.
    fn distance_table(&self, query: &[f32]) -> Vec<f32> {
        let mut table = vec![0.0f32; self.m * CODEBOOK_SIZE];
        for j in 0..self.m {
            let book = &self.books[j];
            if query.len() < (j + 1) * self.dsub {
                continue;
            }
            let part = self.subspace(query, j);
            for c in 0..CODEBOOK_SIZE {
                table[j * CODEBOOK_SIZE + c] = if c < book.k {
                    squared_euclidean(part, book.get(c)) as f32
                } else {
                    f32::INFINITY
                };
            }
        }
        table
    }

    fn distance(&self, table: &[f32], code: &[u8]) -> f64 {
        // Four independent sums let the lookups overlap; one running total
        // serialises every add behind the previous one. Each sum is named by a
        // constant index so it stays in a register: indexed by `j % 4`, the
        // array lives on the stack and every add waits on a store.
        let lookup = |j: usize, part: u8| {
            table
                .get(j * CODEBOOK_SIZE + part as usize)
                .map_or(0.0, |v| *v as f64)
        };
        let code = &code[..code.len().min(self.m)];
        let (quads, rest) = code.as_chunks::<4>();
        let mut acc = [0.0f64; 4];
        for (q, quad) in quads.iter().enumerate() {
            let j = q * 4;
            acc[0] += lookup(j, quad[0]);
            acc[1] += lookup(j + 1, quad[1]);
            acc[2] += lookup(j + 2, quad[2]);
            acc[3] += lookup(j + 3, quad[3]);
        }
        let j = quads.len() * 4;
        for (k, part) in rest.iter().enumerate() {
            acc[k] += lookup(j + k, *part);
        }
        acc.iter().sum()
    }
}

/// `distance_table` is computed once per query and `distance` is then a few
/// lookups per candidate, which is what makes a list scan cheap.
pub trait Quantizer {
    fn code_len(&self) -> usize;

    fn dimensions(&self) -> usize;

    fn encode(&self, vector: &[f32], code: &mut [u8]);

    /// Lossy by construction.
    fn decode(&self, code: &[u8], out: &mut [f32]);

    fn distance_table(&self, query: &[f32]) -> Vec<f32>;

    fn distance(&self, table: &[f32], code: &[u8]) -> f64;
}
