//! Label egress arbiter: gates signals based on label lattice policies.
//!
//! [`LabelArbiter`] inspects a signal's [`LabelSet`] and checks each label
//! against the [`Lattice`]. Any label whose [`EgressPolicy`] is [`Deny`](mcl_spec::label::EgressPolicy::Deny)
//! causes the signal to be rejected.

use mcl_spec::label::{EgressPolicy, Label, Lattice};

use crate::arbiter::{Arbiter, IngestContext};
use crate::types::Verdict;
use mcl_spec::signal::MclSignal;

/// Arbiter that gates signal egress based on label lattice policies.
///
/// For each label in the signal's [`LabelSet`], looks up the matching
/// [`LatticeEntry`]. If any entry has `default_egress == Deny`, the signal
/// is rejected. An empty lattice admits everything.
pub struct LabelArbiter {
    lattice: Lattice,
}

impl LabelArbiter {
    /// Create a new label arbiter with the given lattice.
    pub fn new(lattice: Lattice) -> Self {
        Self { lattice }
    }

    /// Find the egress policy for a label by name.
    fn egress_for(&self, label: &Label) -> Option<EgressPolicy> {
        self.lattice
            .labels
            .iter()
            .find(|entry| entry.name == label.name)
            .map(|entry| entry.default_egress.clone())
    }
}

impl Arbiter for LabelArbiter {
    fn review(&self, signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
        for label in &signal.labels.0 {
            if let Some(policy) = self.egress_for(label) {
                if policy == EgressPolicy::Deny {
                    return Verdict::Reject {
                        reason: format!("label_violation: {}", label.name),
                    };
                }
            }
        }
        Verdict::Accept
    }

    fn name(&self) -> &str {
        "label"
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use mcl_graph::id::NodeId;
    use mcl_spec::label::{EgressPolicy, Label, LabelParam, LabelSet, LatticeEntry};
    use mcl_spec::signal::{MclSignal, SignalKind};
    use mcl_spec::source::SourceId;
    use mcl_spec::span::Span;

    use super::*;
    use crate::arbiter::Arbiter;

    fn test_span() -> Span {
        Span::new(0, 1, 1, 1)
    }

    fn test_ctx() -> IngestContext {
        IngestContext {
            origin_node: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            source: SourceId::default(),
        }
    }

    fn action_signal_with_labels(name: &str, labels: LabelSet) -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: test_span(),
            labels,
        }
    }

    fn label(name: &str) -> Label {
        Label {
            name: name.to_string(),
            params: BTreeMap::new(),
        }
    }

    fn label_with_params(name: &str, audience: &str) -> Label {
        let mut params = BTreeMap::new();
        params.insert("audience".into(), LabelParam::Ident(audience.into()));
        Label {
            name: name.to_string(),
            params,
        }
    }

    fn make_lattice(entries: Vec<LatticeEntry>) -> Lattice {
        Lattice { labels: entries }
    }

    #[test]
    fn arbiter_blocks_label_egress() {
        let lattice = make_lattice(vec![LatticeEntry {
            name: "phi".to_string(),
            dominates: vec![],
            default_egress: EgressPolicy::Deny,
            params: vec![],
            scope: mcl_spec::label::LatticeScope::Workspace,
        }]);
        let arbiter = LabelArbiter::new(lattice);

        let mut labels = LabelSet::default();
        labels.insert(label("phi"));
        let signal = action_signal_with_labels("attack", labels);
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason.contains("label_violation") && reason.contains("phi")),
            "expected Reject for @phi with Deny egress, got {verdict:?}"
        );
    }

    #[test]
    fn label_arbiter_empty_lattice_accepts_all() {
        let lattice = Lattice::default();
        let arbiter = LabelArbiter::new(lattice);

        let mut labels = LabelSet::default();
        labels.insert(label("phi"));
        let signal = action_signal_with_labels("attack", labels);
        let ctx = test_ctx();

        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "empty lattice should accept all signals"
        );
    }

    #[test]
    fn label_arbiter_allow_policy_passes() {
        let lattice = make_lattice(vec![LatticeEntry {
            name: "public".to_string(),
            dominates: vec![],
            default_egress: EgressPolicy::Allow,
            params: vec![],
            scope: mcl_spec::label::LatticeScope::Workspace,
        }]);
        let arbiter = LabelArbiter::new(lattice);

        let mut labels = LabelSet::default();
        labels.insert(label("public"));
        let signal = action_signal_with_labels("attack", labels);
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn label_join_conflicting_labels() {
        let lattice = make_lattice(vec![
            LatticeEntry {
                name: "phi".to_string(),
                dominates: vec![],
                default_egress: EgressPolicy::Deny,
                params: vec![],
                scope: mcl_spec::label::LatticeScope::Workspace,
            },
            LatticeEntry {
                name: "secret".to_string(),
                dominates: vec!["phi".to_string()],
                default_egress: EgressPolicy::Deny,
                params: vec!["audience".to_string()],
                scope: mcl_spec::label::LatticeScope::PerAudience("seller".into()),
            },
        ]);
        let arbiter = LabelArbiter::new(lattice);

        // Signal with both @phi and @secret(audience=seller) — both have Deny
        let mut labels = LabelSet::default();
        labels.insert(label("phi"));
        labels.insert(label_with_params("secret", "seller"));
        let signal = action_signal_with_labels("transfer", labels);
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject when any label has Deny egress, got {verdict:?}"
        );
    }

    #[test]
    fn label_arbiter_no_labels_accepts() {
        let lattice = make_lattice(vec![LatticeEntry {
            name: "phi".to_string(),
            dominates: vec![],
            default_egress: EgressPolicy::Deny,
            params: vec![],
            scope: mcl_spec::label::LatticeScope::Workspace,
        }]);
        let arbiter = LabelArbiter::new(lattice);

        let signal = action_signal_with_labels("attack", LabelSet::default());
        let ctx = test_ctx();

        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "signal with no labels should always be accepted"
        );
    }

    #[test]
    fn label_arbiter_unknown_label_accepts() {
        let lattice = make_lattice(vec![LatticeEntry {
            name: "phi".to_string(),
            dominates: vec![],
            default_egress: EgressPolicy::Deny,
            params: vec![],
            scope: mcl_spec::label::LatticeScope::Workspace,
        }]);
        let arbiter = LabelArbiter::new(lattice);

        // Label "unknown" is not in the lattice — no policy → accept
        let mut labels = LabelSet::default();
        labels.insert(label("unknown"));
        let signal = action_signal_with_labels("attack", labels);
        let ctx = test_ctx();

        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "labels not in lattice should be accepted (no policy)"
        );
    }

    #[test]
    fn label_arbiter_name() {
        let arbiter = LabelArbiter::new(Lattice::default());
        assert_eq!(arbiter.name(), "label");
    }

    #[test]
    fn propagation_join_via_signal() {
        // R1 §6 test 3: label set from join of two values is correct
        let mut labels_a = LabelSet::default();
        labels_a.insert(label("phi"));

        let mut labels_b = LabelSet::default();
        labels_b.insert(label("secret"));

        let joined = labels_a.join(&labels_b);
        assert!(joined.contains_name("phi"), "join should include @phi");
        assert!(
            joined.contains_name("secret"),
            "join should include @secret"
        );
        assert_eq!(
            joined.0.len(),
            2,
            "join of two distinct labels should have 2 entries"
        );
    }
}
