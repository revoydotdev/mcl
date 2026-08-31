use mcl_registry::SignalId;
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::span::Span;

#[test]
fn signal_id_stability_empty_labels() {
    let origin = mcl_graph::id::NodeId::new("test_node");

    let signal_with_labels = MclSignal {
        kind: SignalKind::Transition {
            context: "room_2".into(),
        },
        span: Span::new(0, 1, 1, 1),
        labels: LabelSet::default(),
    };

    let id1 = SignalId::from_signal(&origin, 1, "test_channel", &signal_with_labels);

    let signal_without_labels_field = serde_json::json!({
        "kind": {
            "Transition": {
                "context": "room_2"
            }
        },
        "span": {
            "offset": 0,
            "len": 1,
            "line": 1,
            "col": 1
        }
    });

    let signal_without_labels: MclSignal =
        serde_json::from_value(signal_without_labels_field).unwrap();

    let id2 = SignalId::from_signal(&origin, 1, "test_channel", &signal_without_labels);

    assert_eq!(
        id1, id2,
        "SignalId hash must be identical for empty labels vs no labels field"
    );
}

#[test]
fn signal_serde_golden() {
    let signal = MclSignal {
        kind: SignalKind::Transition {
            context: "room_2".into(),
        },
        span: Span::new(0, 1, 1, 1),
        labels: LabelSet::default(),
    };

    let json_bytes = serde_json::to_vec(&signal).unwrap();

    let golden_bytes = std::fs::read("tests/signal_serde_baseline.json").unwrap();

    assert_eq!(
        json_bytes, golden_bytes,
        "Current serialization must match golden baseline"
    );
}
