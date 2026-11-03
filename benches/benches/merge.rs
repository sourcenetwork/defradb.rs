//! Field CRDT merges: one block's state joining a stored state, through the
//! `FieldCrdt` every merge path dispatches to, and a document value's parts
//! merging as the storage merge operator merges them.

use crdt::field::native as field_crdt;
use crdt::stored::{StoredDocument, StoredField};
use crdt::FieldCrdt;
use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion};
use document::NormalValue;
use schema::{CType, FieldDescription, FieldKind};
use std::hint::black_box;

fn state(
    crdt: &dyn FieldCrdt,
    field: &FieldDescription,
    priority: u64,
    value: NormalValue,
) -> StoredField {
    crdt.state(field, priority, &value).unwrap()
}

fn bench_merge(
    c: &mut Criterion,
    name: &str,
    crdt: &dyn FieldCrdt,
    current: StoredField,
    incoming: StoredField,
) {
    c.benchmark_group("field_crdt")
        .bench_function(BenchmarkId::from_parameter(name), |b| {
            b.iter_batched(
                || (current.clone(), incoming.clone()),
                |(mut current, incoming)| {
                    black_box(crdt.merge(&mut current, incoming));
                    current
                },
                BatchSize::SmallInput,
            )
        });
}

fn bench_field_merges(c: &mut Criterion) {
    let lww = field_crdt(CType::LwwRegister).unwrap();
    let text = FieldDescription::new("1", "name", FieldKind::string(), CType::LwwRegister);
    let value = |s: &str| NormalValue::String(s.into());
    let alice = state(lww, &text, 1, value("alice"));
    bench_merge(
        c,
        "lww_clear_winner",
        lww,
        alice,
        state(lww, &text, 2, value("bob")),
    );
    let alice = state(lww, &text, 2, value("alice"));
    bench_merge(
        c,
        "lww_tiebreak",
        lww,
        alice,
        state(lww, &text, 2, value("bob")),
    );

    let counter = field_crdt(CType::PnCounter).unwrap();
    for (name, kind, start, by) in [
        (
            "counter_int64",
            FieldKind::int(),
            NormalValue::Int(41),
            NormalValue::Int(1),
        ),
        (
            "counter_float32",
            FieldKind::float32(),
            NormalValue::Float32(0.5),
            NormalValue::Float32(0.25),
        ),
        (
            "counter_float64",
            FieldKind::float64(),
            NormalValue::Float64(0.5),
            NormalValue::Float64(0.25),
        ),
    ] {
        let field = FieldDescription::new("1", "score", kind, CType::PnCounter);
        let current = state(counter, &field, 1, start);
        bench_merge(c, name, counter, current, state(counter, &field, 2, by));
    }
}

fn document(fields: u32, priority: u64, value: &str) -> StoredDocument {
    let lww = field_crdt(CType::LwwRegister).unwrap();
    let mut document = StoredDocument::default();
    for id in 0..fields {
        let field = FieldDescription::new(
            id.to_string(),
            format!("field{id}"),
            FieldKind::string(),
            CType::LwwRegister,
        );
        let part = state(lww, &field, priority, NormalValue::String(value.into()));
        document.fields.insert(id, part);
    }
    document
}

fn bench_document_merges(c: &mut Criterion) {
    let mut group = c.benchmark_group("field_crdt");
    for fields in [1, 4, 16] {
        let base = document(fields, 1, "initial");
        let incoming = document(fields, 2, "updated");
        group.bench_function(BenchmarkId::new("document_apply", fields), |b| {
            b.iter_batched(
                || (base.clone(), incoming.clone()),
                |(mut base, incoming)| {
                    black_box(base.apply(incoming, &crdt::Crdts::default()));
                    base
                },
                BatchSize::SmallInput,
            )
        });
    }
    group.finish();
}

criterion_group!(benches, bench_field_merges, bench_document_merges);
criterion_main!(benches);
