//! Warm-up queries disjoint from the test set: midpoints of random corpus pairs.
//!
//! `mkwarm <base.f32> <dims> <count> <out.f32>`

use std::io::Write;
use std::path::Path;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let [_, base, dims, count, out] = args.as_slice() else {
        panic!("usage: mkwarm <base.f32> <dims> <count> <out.f32>");
    };
    let dims: usize = dims.parse().unwrap();
    let count: usize = count.parse().unwrap();
    let base = vector_bench::read_f32(Path::new(base), dims);
    let mut x: u64 = 0x9e3779b97f4a7c15;
    let mut next = || {
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        (x as usize) % base.len()
    };
    let mut out = std::io::BufWriter::new(std::fs::File::create(out).unwrap());
    for _ in 0..count {
        let (a, c) = (&base[next()], &base[next()]);
        for (p, q) in a.iter().zip(c) {
            out.write_all(&((p + q) / 2.0).to_le_bytes()).unwrap();
        }
    }
    out.flush().unwrap();
}
