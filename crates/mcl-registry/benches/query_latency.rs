//! Query latency benchmark for mcl-registry.
//!
//! Seeds a `Registry` with 100k signals, then measures query latency
//! for each `RegistryQuery` variant. Reports p99 latency per query type.
//!
//! Target: p99 < 5ms at 100k signals.
//!
//! Run with:
//!   MCL_MOCK_EMBEDDING=1 cargo bench -p mcl-registry --bench query_latency
//!   MCL_MOCK_EMBEDDING=1 cargo bench -p mcl-registry --bench query_latency -- --save-baseline stage2

use std::cell::RefCell;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use mcl_graph::id::NodeId;
use mcl_registry::{
    CausalityGraph, FsyncPolicy, Registry, RegistryIndex, RegistryQuery, SignalId, Verdict,
};
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::span::Span;
use mcl_spec::value::MclValue;

const SIGNAL_COUNT: u64 = 100_000;

/// Build a diverse test signal. Cycles through SignalKind variants to exercise
/// all serialization paths and produce varied content hashes.
fn make_signal(index: u64) -> MclSignal {
    let variant = index % 5;
    match variant {
        0 => MclSignal {
            kind: SignalKind::Action {
                name: format!("attack_{index}"),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        },
        1 => MclSignal {
            kind: SignalKind::Mutate {
                path: format!("entity_{index}.hp"),
                value: MclValue::Int((index % 100) as i64),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        },
        2 => MclSignal {
            kind: SignalKind::Dialogue {
                entity: format!("npc_{index}"),
                text: format!("Hello turn {index}"),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        },
        3 => MclSignal {
            kind: SignalKind::Transition {
                context: format!("room_{index}"),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        },
        _ => MclSignal {
            kind: SignalKind::Narrate {
                tag: format!("event_{index}"),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        },
    }
}

struct BenchFixture {
    registry: RefCell<Registry>,
    #[allow(dead_code)]
    dir: tempfile::TempDir,
    sample_signal_id: SignalId,
    first_signal_id: SignalId,
    mid_turn: u64,
}

impl BenchFixture {
    fn new(signal_count: u64) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("bench_query.jsonl");

        let mut registry = Registry::new(&path, FsyncPolicy::OnShutdown).expect("registry");

        let channels = ["actions", "inventory", "dialogue", "internal", "movement"];
        let origins = ["player", "guard", "npc_1", "game_engine", "scheduler"];

        let mut index = RegistryIndex::new().expect("index");
        let mut causality = CausalityGraph::new();

        let mut prev_id: Option<SignalId> = None;
        let mut first_signal_id: Option<SignalId> = None;

        for i in 0..signal_count {
            let i_usize = i as usize;
            let signal = make_signal(i);
            let origin = NodeId::new(origins[i_usize % origins.len()]);
            let channel = channels[i_usize % channels.len()].to_string();
            let turn = i / 5;

            let entry = registry
                .log_mut()
                .append(signal, origin.clone(), turn, channel, Verdict::Accept)
                .expect("append");

            index.ingest(&entry);

            if first_signal_id.is_none() {
                first_signal_id = Some(entry.id.clone());
            }

            if let Some(pid) = prev_id.take() {
                causality.add_edge(&entry.id, &[pid]);
            }
            prev_id = Some(entry.id.clone());
        }

        *registry.index_mut() = index;
        *registry.causality_mut() = causality;

        let entries = registry.log().read().expect("read");
        let mid_idx = entries.len() / 2;
        let sample_signal_id = entries[mid_idx].id.clone();
        let mid_turn = entries[mid_idx].turn;

        Self {
            registry: RefCell::new(registry),
            dir,
            sample_signal_id,
            first_signal_id: first_signal_id.expect("at least one signal"),
            mid_turn,
        }
    }
}

fn query_by_path(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(50);

    group.bench_function("by_path", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::ByPath {
                    path: "actions".to_string(),
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_by_turn_range(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let mid = fixture.mid_turn;
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(50);

    group.bench_function("by_turn_range", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::ByTurnRange {
                    start: mid.saturating_sub(10),
                    end: mid + 10,
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_by_origin(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(50);

    group.bench_function("by_origin", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::ByOrigin {
                    node_id: NodeId::new("player"),
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_by_semantic(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(20);

    group.bench_function("by_semantic_k10", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::BySemantic {
                    text: "attack goblin".to_string(),
                    k: 10,
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_by_ancestors(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let signal_id = fixture.sample_signal_id.clone();
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(50);

    group.bench_function("by_ancestors", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::ByAncestors {
                    signal_id: signal_id.clone(),
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_by_descendants(c: &mut Criterion) {
    let fixture = BenchFixture::new(SIGNAL_COUNT);
    let first_id = fixture.first_signal_id.clone();
    let mut group = c.benchmark_group("query_latency");
    group.sample_size(50);

    // For descendants we need an early signal (first entry) to have
    // the longest possible descendant chain.
    group.bench_function("by_descendants", |b| {
        b.iter(|| {
            let mut reg = fixture.registry.borrow_mut();
            black_box(
                reg.query(RegistryQuery::ByDescendants {
                    signal_id: first_id.clone(),
                })
                .expect("query"),
            )
        })
    });

    group.finish();
}

fn query_latency_by_dataset_size(c: &mut Criterion) {
    let mut group = c.benchmark_group("query_latency_scale");
    group.sample_size(20);

    for &size in &[1_000u64, 10_000, 50_000, 100_000] {
        let fixture = BenchFixture::new(size);
        group.bench_with_input(BenchmarkId::new("by_path", size), &size, |b, &_size| {
            b.iter(|| {
                let mut reg = fixture.registry.borrow_mut();
                black_box(
                    reg.query(RegistryQuery::ByPath {
                        path: "actions".to_string(),
                    })
                    .expect("query"),
                )
            })
        });
    }

    group.finish();
}

criterion_group!(
    benches,
    query_by_path,
    query_by_turn_range,
    query_by_origin,
    query_by_semantic,
    query_by_ancestors,
    query_by_descendants,
    query_latency_by_dataset_size,
);
criterion_main!(benches);
