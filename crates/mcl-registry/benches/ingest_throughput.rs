//! Ingest throughput benchmark for mcl-registry.
//!
//! Measures how many signals can be appended per second via `RegistryHandle::append`.
//! Uses `FsyncPolicy::OnShutdown` to isolate append logic from fsync overhead.
//!
//! Target: >= 10,000 signals/s
//!
//! Run with:
//!   cargo bench -p mcl-registry --bench ingest_throughput
//!   cargo bench -p mcl-registry --bench ingest_throughput -- --save-baseline stage2

use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, Criterion, Throughput};

use mcl_graph::id::NodeId;
use mcl_registry::{FsyncPolicy, RegistryHandle, Verdict};
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::span::Span;

/// Build a test signal with a unique action name to ensure distinct content hashes.
fn make_signal(index: u64) -> MclSignal {
    MclSignal {
        kind: SignalKind::Action {
            name: format!("bench_action_{index}"),
            args: Default::default(),
        },
        span: Span::new(0, 1, 1, 1),
        labels: LabelSet::default(),
    }
}

/// Benchmark: ingest 10k signals via RegistryHandle, measure throughput.
fn ingest_10k_signals(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest_throughput");
    group.sample_size(10);
    group.throughput(Throughput::Elements(10_000));

    group.bench_function("append_10k_on_shutdown", |b| {
        b.iter_batched(
            || {
                // Setup: create a temp dir and fresh handle for each sample
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("bench.jsonl");
                let handle = RegistryHandle::new(&path, FsyncPolicy::OnShutdown).expect("handle");
                (dir, handle)
            },
            |(_dir, handle)| {
                let origin = NodeId::new("bench_node");
                for i in 0..10_000u64 {
                    handle
                        .append(
                            make_signal(black_box(i)),
                            origin.clone(),
                            i,
                            "actions".to_string(),
                            Verdict::Accept,
                        )
                        .expect("append");
                }
                handle.len()
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

/// Benchmark: ingest 10k signals with `FsyncPolicy::EveryStep` (worst-case fsync).
/// This is expected to be significantly slower — included to show fsync impact.
fn ingest_10k_signals_fsync_every(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest_throughput");
    group.sample_size(10);
    group.throughput(Throughput::Elements(10_000));

    group.bench_function("append_10k_fsync_every", |b| {
        b.iter_batched(
            || {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("bench.jsonl");
                let handle = RegistryHandle::new(&path, FsyncPolicy::EveryStep).expect("handle");
                (dir, handle)
            },
            |(_dir, handle)| {
                let origin = NodeId::new("bench_node");
                for i in 0..10_000u64 {
                    handle
                        .append(
                            make_signal(black_box(i)),
                            origin.clone(),
                            i,
                            "actions".to_string(),
                            Verdict::Accept,
                        )
                        .expect("append");
                }
                handle.len()
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

/// Benchmark: ingest 10k signals with `FsyncPolicy::EveryNSteps(100)` (balanced).
fn ingest_10k_signals_fsync_every_100(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingest_throughput");
    group.sample_size(10);
    group.throughput(Throughput::Elements(10_000));

    group.bench_function("append_10k_fsync_every_100", |b| {
        b.iter_batched(
            || {
                let dir = tempfile::tempdir().expect("tempdir");
                let path = dir.path().join("bench.jsonl");
                let handle =
                    RegistryHandle::new(&path, FsyncPolicy::EveryNSteps(100)).expect("handle");
                (dir, handle)
            },
            |(_dir, handle)| {
                let origin = NodeId::new("bench_node");
                for i in 0..10_000u64 {
                    handle
                        .append(
                            make_signal(black_box(i)),
                            origin.clone(),
                            i,
                            "actions".to_string(),
                            Verdict::Accept,
                        )
                        .expect("append");
                }
                handle.len()
            },
            BatchSize::SmallInput,
        )
    });

    group.finish();
}

criterion_group!(
    benches,
    ingest_10k_signals,
    ingest_10k_signals_fsync_every,
    ingest_10k_signals_fsync_every_100,
);
criterion_main!(benches);
