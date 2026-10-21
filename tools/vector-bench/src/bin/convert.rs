//! VectorDBBench parquet to the raw little-endian files `vector_disk_cost` reads.
//!
//! `convert <dir>` reads `train`, `test` and `neighbors.parquet` and writes
//! `base.f32`, `queries.f32` and `gt10.u64` (ids shifted by one, as the
//! benchmark numbers its nodes).

use std::fs::File;
use std::io::{BufWriter, Write};

use arrow::array::{Array, AsArray, LargeListArray};
use arrow::datatypes::{DataType, Float32Type, Float64Type, Int64Type};
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;

fn lists<F>(path: &str, column: &str, mut each: F)
where
    F: FnMut(i64, &dyn Array),
{
    let reader = ParquetRecordBatchReaderBuilder::try_new(File::open(path).unwrap())
        .unwrap()
        .with_batch_size(8192)
        .build()
        .unwrap();
    for batch in reader {
        let batch = batch.unwrap();
        let ids = batch
            .column_by_name("id")
            .unwrap()
            .as_primitive::<Int64Type>()
            .clone();
        let list: &LargeListArray = batch.column_by_name(column).unwrap().as_list::<i64>();
        for row in 0..batch.num_rows() {
            each(ids.value(row), list.value(row).as_ref());
        }
    }
}

fn floats(path: &str, out: &str) {
    let mut w = BufWriter::new(File::create(out).unwrap());
    let (mut n, mut dims, mut expect) = (0usize, 0usize, 0i64);
    lists(path, "emb", |id, v| {
        assert_eq!(id, expect, "{path}: ids not dense and ordered");
        expect += 1;
        let vals: Vec<f32> = match v.data_type() {
            DataType::Float32 => v.as_primitive::<Float32Type>().values().to_vec(),
            _ => v
                .as_primitive::<Float64Type>()
                .values()
                .iter()
                .map(|x| *x as f32)
                .collect(),
        };
        dims = vals.len();
        for f in vals {
            w.write_all(&f.to_le_bytes()).unwrap();
        }
        n += 1;
    });
    w.flush().unwrap();
    eprintln!("{out}: {n} x {dims}");
}

fn main() {
    let dir = std::env::args().nth(1).expect("usage: convert <dir>");
    floats(&format!("{dir}/train.parquet"), &format!("{dir}/base.f32"));
    floats(
        &format!("{dir}/test.parquet"),
        &format!("{dir}/queries.f32"),
    );
    let mut w = BufWriter::new(File::create(format!("{dir}/gt10.u64")).unwrap());
    let mut q = 0;
    lists(
        &format!("{dir}/neighbors.parquet"),
        "neighbors_id",
        |_, v| {
            for id in v.as_primitive::<Int64Type>().values().iter().take(10) {
                w.write_all(&((*id as u64) + 1).to_le_bytes()).unwrap();
            }
            q += 1;
        },
    );
    w.flush().unwrap();
    eprintln!("gt10.u64: {q} queries");
}
