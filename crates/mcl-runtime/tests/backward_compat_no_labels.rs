//! R1 §6 test 10: backward compatibility verification.
//!
//! For each existing fixture directory (crypt, debate, multi_persona, quiz),
//! verify that:
//! 1. All .mcl files parse without errors.
//! 2. Signals produced from parsed documents with empty LabelSet produce the
//!    same SignalId as pre-R1 behavior (labels field absent in JSON).
//! 3. JSON serialization of empty-label signals omits the labels key entirely
//!    (matching the golden baseline in signal_serde_baseline.json).

use std::path::PathBuf;

use mcl_graph::id::NodeId;
use mcl_registry::SignalId;
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::span::Span;

// ---------------------------------------------------------------------------
// Fixture discovery
// ---------------------------------------------------------------------------

/// All pre-R1 fixture directories that must remain backward-compatible.
const FIXTURE_DIRS: &[&str] = &["crypt", "debate", "multi_persona", "quiz"];

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn fixture_mcl_files(dir_name: &str) -> Vec<PathBuf> {
    let dir = examples_dir().join(dir_name);
    assert!(dir.is_dir(), "fixture dir '{dir_name}' should exist");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read dir {dir_name}: {e}"))
        .filter_map(|entry| {
            let entry = entry.expect("read dir entry");
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mcl") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

// ---------------------------------------------------------------------------
// Core backward-compat assertions
// ---------------------------------------------------------------------------

/// Every .mcl file in every pre-R1 fixture directory must parse without errors.
#[test]
fn all_fixtures_parse_without_error() {
    for dir_name in FIXTURE_DIRS {
        let files = fixture_mcl_files(dir_name);
        assert!(
            !files.is_empty(),
            "fixture dir '{dir_name}' should contain at least one .mcl file"
        );
        for path in &files {
            let src =
                std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
            let result = mcl_parse::parse_document(&src);
            assert!(
                result.is_ok(),
                "parse error in {path:?}: {:?}",
                result.err()
            );
        }
    }
}

/// Signals with an explicitly empty LabelSet must produce the same SignalId
/// as signals where the `labels` field is absent (pre-R1 JSON format).
#[test]
fn signal_id_empty_labels_matches_absent_labels() {
    let origin = NodeId::new("backward_compat_test");

    let cases = [
        SignalKind::Transition {
            context: "room_2".into(),
        },
        SignalKind::Action {
            name: "attack".into(),
            args: Default::default(),
        },
        SignalKind::Mutate {
            path: "lit".into(),
            value: mcl_spec::value::MclValue::Bool(true),
        },
        SignalKind::Dialogue {
            entity: "guard".into(),
            text: "Halt!".into(),
        },
        SignalKind::Noop,
        SignalKind::Narrate {
            tag: "intro".into(),
        },
        SignalKind::Query {
            path: "player.hp".into(),
        },
    ];

    for (i, kind) in cases.iter().enumerate() {
        let signal_with_labels = MclSignal {
            kind: kind.clone(),
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        };

        let id_with_labels = SignalId::from_signal(&origin, 1, "ch", &signal_with_labels);

        let mut json_val = serde_json::to_value(&signal_with_labels).expect("serialize signal");
        if let Some(obj) = json_val.as_object_mut() {
            obj.remove("labels");
        }

        let signal_no_labels: MclSignal =
            serde_json::from_value(json_val).expect("deserialize signal without labels field");

        let id_no_labels = SignalId::from_signal(&origin, 1, "ch", &signal_no_labels);

        assert_eq!(
            id_with_labels, id_no_labels,
            "case {i} ({kind:?}): SignalId must be identical for empty labels vs absent labels field"
        );
        assert!(
            signal_no_labels.labels.is_empty(),
            "case {i}: deserialized signal should have empty labels after default"
        );
    }
}

/// JSON serialization of a signal with empty labels must NOT include the
/// `labels` key — matching the golden baseline exactly.
#[test]
fn empty_labels_omitted_from_json() {
    let signal = MclSignal {
        kind: SignalKind::Transition {
            context: "room_2".into(),
        },
        span: Span::new(0, 1, 1, 1),
        labels: LabelSet::default(),
    };

    let json_bytes = serde_json::to_vec(&signal).unwrap();
    let json_str = String::from_utf8(json_bytes.clone()).unwrap();

    assert!(
        !json_str.contains("labels"),
        "empty LabelSet must be skipped in serialization, but got: {json_str}"
    );

    let baseline_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../mcl-spec/tests/signal_serde_baseline.json");
    let golden_bytes =
        std::fs::read(&baseline_path).unwrap_or_else(|e| panic!("read golden baseline: {e}"));

    assert_eq!(
        json_bytes, golden_bytes,
        "empty-labels serialization must match golden baseline"
    );
}

/// For each fixture, parse the document and verify that constructing signals
/// from its actions produces stable SignalIds (same across calls).
#[test]
fn fixture_signals_produce_stable_ids() {
    for dir_name in FIXTURE_DIRS {
        for path in fixture_mcl_files(dir_name) {
            let src =
                std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
            let doc =
                mcl_parse::parse_document(&src).unwrap_or_else(|e| panic!("parse {path:?}: {e}"));

            let origin = NodeId::new(&doc.frontmatter.context);

            for (action_name, _action_def) in &doc.actions {
                let signal = MclSignal {
                    kind: SignalKind::Action {
                        name: action_name.clone(),
                        args: Default::default(),
                    },
                    span: Span::new(0, 1, 1, 1),
                    labels: LabelSet::default(),
                };

                let id1 = SignalId::from_signal(&origin, 1, "actions", &signal);
                let id2 = SignalId::from_signal(&origin, 1, "actions", &signal);

                assert_eq!(
                    id1, id2,
                    "{path:?} action '{action_name}': SignalId must be deterministic"
                );
            }

            for transition in &doc.transitions {
                let signal = MclSignal {
                    kind: SignalKind::Transition {
                        context: transition.target.clone(),
                    },
                    span: Span::new(0, 1, 1, 1),
                    labels: LabelSet::default(),
                };

                let id1 = SignalId::from_signal(&origin, 1, "transitions", &signal);
                let id2 = SignalId::from_signal(&origin, 1, "transitions", &signal);

                assert_eq!(
                    id1, id2,
                    "{path:?} transition '{}': SignalId must be deterministic",
                    transition.target
                );
            }
        }
    }
}

/// Verify that the `mcl-signal` extractor produces the same results for
/// plain extraction (no labels) as it did before R1 — empty labels.
#[test]
fn signal_extraction_backward_compat() {
    use mcl_signal::extract::extract_signals;

    let text = "The torch flickers.\nMCL::action(light_torch)\nMCL::mutate(lit, true)";
    let result = extract_signals(text);

    assert_eq!(result.signals.len(), 2, "should extract 2 signals");
    assert!(result.narration.iter().any(|l| l.contains("torch")));

    for signal in &result.signals {
        assert!(
            signal.labels.is_empty(),
            "plain extract_signals must produce empty labels, got: {:?}",
            signal.labels
        );
    }
}

/// Round-trip: serialize a signal with empty labels, deserialize, and verify
/// the deserialized signal has empty labels (backward-compatible default).
#[test]
fn serde_roundtrip_empty_labels() {
    let original = MclSignal {
        kind: SignalKind::Action {
            name: "look".into(),
            args: Default::default(),
        },
        span: Span::new(5, 10, 2, 3),
        labels: LabelSet::default(),
    };

    let json = serde_json::to_string(&original).unwrap();
    assert!(
        !json.contains("labels"),
        "empty labels should be omitted: {json}"
    );

    let back: MclSignal = serde_json::from_str(&json).unwrap();
    assert_eq!(original, back);
    assert!(back.labels.is_empty());
}
