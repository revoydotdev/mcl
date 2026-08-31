use mcl_policy::ast::{BindingEnv, BindingValue, CompiledPolicy};
use mcl_spec::policy::{Event, PolicyPriority, PolicyViolationAction};
use mcl_spec::signal::{MclSignal, SignalKind};

use crate::arbiter::{Arbiter, IngestContext};
use crate::types::Verdict;

pub struct PolicyArbiter {
    policies: Vec<CompiledPolicy>,
}

impl PolicyArbiter {
    pub fn new(mut policies: Vec<CompiledPolicy>) -> Self {
        policies.sort_by(|a, b| {
            let pa = matches!(a.priority, PolicyPriority::High);
            let pb = matches!(b.priority, PolicyPriority::High);
            pb.cmp(&pa)
        });
        Self { policies }
    }
}

fn signal_kind_to_event(kind: &SignalKind) -> Option<Event> {
    match kind {
        SignalKind::Action { .. } => Some(Event::SignalAdmit),
        SignalKind::Mutate { .. } => Some(Event::StateMutate),
        _ => None,
    }
}

fn build_env(signal: &MclSignal, ctx: &IngestContext) -> BindingEnv {
    let mut env = BindingEnv::new();

    let signal_fields = match &signal.kind {
        SignalKind::Action { name, args } => {
            let mut fields = vec![BindingValue::Str(name.clone())];
            if !args.is_empty() {
                fields.push(BindingValue::Str(format!("{args:?}")));
            }
            fields
        }
        SignalKind::Mutate { path, value } => {
            vec![
                BindingValue::Str(path.clone()),
                BindingValue::Str(format!("{value:?}")),
            ]
        }
        _ => vec![],
    };
    env.insert("signal", BindingValue::List(signal_fields));
    env.insert(
        "context",
        BindingValue::List(vec![
            BindingValue::Str(ctx.origin_node.as_str().to_string()),
            BindingValue::Int(ctx.turn as i64),
            BindingValue::Str(ctx.channel.clone()),
        ]),
    );

    env
}

fn violation_to_verdict(policy: &CompiledPolicy, action: &PolicyViolationAction) -> Verdict {
    match action {
        PolicyViolationAction::Reject { reason } => Verdict::Reject {
            reason: format!("policy '{}': {}", policy.name, reason),
        },
        PolicyViolationAction::Deny => Verdict::Reject {
            reason: format!("policy '{}': denied", policy.name),
        },
        PolicyViolationAction::RouteTo { channel } => Verdict::RouteTo {
            channel: channel.clone(),
            policy_id: policy.name.clone(),
        },
        PolicyViolationAction::RedactAndWarn
        | PolicyViolationAction::Rewrite
        | PolicyViolationAction::DemandConfirmation { .. }
        | PolicyViolationAction::LogOnly
        | PolicyViolationAction::AuditOnly => Verdict::Reject {
            reason: format!(
                "policy '{}': {}",
                policy.name,
                format!("{action:?}").to_lowercase()
            ),
        },
    }
}

impl Arbiter for PolicyArbiter {
    fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict {
        let event = match signal_kind_to_event(&signal.kind) {
            Some(e) => e,
            None => return Verdict::Accept,
        };

        let env = build_env(signal, ctx);

        for policy in &self.policies {
            if policy.on != event {
                continue;
            }

            if !policy.matches(&env) {
                continue;
            }

            if policy.check_requires(&env).is_err() {
                return violation_to_verdict(policy, &policy.on_violation);
            }
        }

        Verdict::Accept
    }

    fn name(&self) -> &str {
        "policy_arbiter"
    }
}

#[cfg(test)]
mod tests {
    use mcl_graph::id::NodeId;
    use mcl_spec::label::LabelSet;
    use mcl_spec::policy::{Event, Lit, PolicyExpr, PolicyPriority, PolicyViolationAction};
    use mcl_spec::signal::SignalKind;
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

    fn action_signal(name: &str) -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        }
    }

    fn mutate_signal(path: &str) -> MclSignal {
        MclSignal {
            kind: SignalKind::Mutate {
                path: path.to_string(),
                value: mcl_spec::value::MclValue::Int(42),
            },
            span: test_span(),
            labels: LabelSet::default(),
        }
    }

    fn noop_signal() -> MclSignal {
        MclSignal {
            kind: SignalKind::Noop,
            span: test_span(),
            labels: LabelSet::default(),
        }
    }

    fn deny_policy(name: &str, event: Event) -> CompiledPolicy {
        CompiledPolicy {
            name: name.to_string(),
            on: event,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: PolicyPriority::Default,
        }
    }

    #[test]
    fn policy_arbiter_matching_policy_fires() {
        let policy = deny_policy("deny_all_actions", Event::SignalAdmit);
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = action_signal("attack");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        match &verdict {
            Verdict::Reject { reason } => {
                assert!(reason.contains("deny_all_actions"));
                assert!(reason.contains("denied"));
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn policy_arbiter_non_matching_skipped() {
        let policy = deny_policy("only_mutate", Event::StateMutate);
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn policy_arbiter_route_to_verdict() {
        let policy = CompiledPolicy {
            name: "route_policy".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::RouteTo {
                channel: "review_queue".to_string(),
            },
            priority: PolicyPriority::Default,
        };
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = action_signal("suspicious_action");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        match &verdict {
            Verdict::RouteTo { channel, policy_id } => {
                assert_eq!(channel, "review_queue");
                assert_eq!(policy_id, "route_policy");
            }
            other => panic!("expected RouteTo, got {other:?}"),
        }
    }

    #[test]
    fn policy_arbiter_no_signal_event_match_accepts() {
        let arbiter = PolicyArbiter::new(vec![deny_policy("deny_all", Event::SignalAdmit)]);

        let signal = noop_signal();
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn policy_arbiter_empty_policies_accepts() {
        let arbiter = PolicyArbiter::new(vec![]);

        let signal = action_signal("anything");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn policy_arbiter_high_priority_first() {
        let low_policy = CompiledPolicy {
            name: "low_priority".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Reject {
                reason: "low reason".to_string(),
            },
            priority: PolicyPriority::Default,
        };
        let high_policy = CompiledPolicy {
            name: "high_priority".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: PolicyPriority::High,
        };

        let arbiter = PolicyArbiter::new(vec![low_policy, high_policy]);

        let signal = action_signal("attack");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        match &verdict {
            Verdict::Reject { reason } => {
                assert!(
                    reason.contains("high_priority"),
                    "expected high_priority to fire first, got: {reason}"
                );
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn policy_arbiter_passing_requires_accepts() {
        let policy = CompiledPolicy {
            name: "always_pass".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(true),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Deny,
            priority: PolicyPriority::Default,
        };
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = action_signal("safe_action");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn policy_arbiter_reject_with_reason() {
        let policy = CompiledPolicy {
            name: "reject_with_reason".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Reject {
                reason: "amount exceeds daily limit".to_string(),
            },
            priority: PolicyPriority::Default,
        };
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = action_signal("transfer");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        match &verdict {
            Verdict::Reject { reason } => {
                assert!(reason.contains("reject_with_reason"));
                assert!(reason.contains("amount exceeds daily limit"));
            }
            other => panic!("expected Reject, got {other:?}"),
        }
    }

    #[test]
    fn policy_arbiter_mutate_signal_matches_state_mutate_event() {
        let policy = deny_policy("deny_all_mutates", Event::StateMutate);
        let arbiter = PolicyArbiter::new(vec![policy]);

        let signal = mutate_signal("player.hp");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for matching mutate policy, got {verdict:?}"
        );
    }

    #[test]
    fn policy_arbiter_name() {
        let arbiter = PolicyArbiter::new(vec![]);
        assert_eq!(arbiter.name(), "policy_arbiter");
    }

    /// M-F: A policy violation must persist its `policy_id` AND the matched
    /// expression bindings to the signal log so an auditor can reconstruct
    /// *why* the policy fired (R2 §2.3). The pre-existing
    /// `policy_arbiter_reject_with_reason` test only verified the in-memory
    /// `Verdict`; this test additionally drives the violation through
    /// `SignalLog::append_with_policy_bindings`, reads the entry back from
    /// disk, and asserts the bindings are preserved alongside the verdict.
    #[test]
    fn policy_violation_audit_persists_bindings() {
        use crate::log::SignalLog;
        use crate::types::FsyncPolicy;
        use tempfile::NamedTempFile;

        // Build a Reject policy and matching arbiter.
        let policy = CompiledPolicy {
            name: "spend_limit".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Reject {
                reason: "amount exceeds daily_limit".to_string(),
            },
            priority: PolicyPriority::Default,
        };
        let arbiter = PolicyArbiter::new(vec![policy]);
        let signal = action_signal("transfer");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        let policy_id_in_reason = matches!(
            &verdict,
            Verdict::Reject { reason } if reason.contains("spend_limit")
        );
        assert!(policy_id_in_reason, "verdict reason must carry policy_id");

        // Capture a representative slice of the policy's expression environment
        // as the bindings. In production this would come from the eval frame;
        // for the audit-persistence contract test, the source is irrelevant —
        // what matters is that whatever the arbiter writes survives the trip
        // through the signal log.
        let bindings = serde_json::json!({
            "policy_id": "spend_limit",
            "matched_expr": "amount > daily_limit",
            "signal.kind": "action(transfer)",
            "ctx.turn": ctx.turn,
        });

        // Drive the verdict + bindings through the audit log on disk.
        let tmp = NamedTempFile::new().expect("temp file");
        let mut log = SignalLog::open(tmp.path(), FsyncPolicy::EveryStep).expect("open");
        let written = log
            .append_with_policy_bindings(
                signal,
                ctx.origin_node.clone(),
                ctx.turn,
                ctx.channel.clone(),
                verdict,
                Some(bindings.clone()),
            )
            .expect("append");
        assert_eq!(
            written.policy_bindings,
            Some(bindings.clone()),
            "in-memory entry must carry the bindings"
        );

        // Read the entry back from disk and assert the audit log preserves
        // both the policy_id (in the verdict) and the bindings sidecar.
        let entries = log.read().expect("read back");
        assert_eq!(entries.len(), 1, "exactly one entry persisted");
        let persisted = &entries[0];
        match &persisted.verdict {
            Verdict::Reject { reason } => {
                assert!(
                    reason.contains("spend_limit"),
                    "persisted verdict reason must carry policy_id, got: {reason}"
                );
            }
            other => panic!("expected persisted Reject, got {other:?}"),
        }
        assert_eq!(
            persisted.policy_bindings,
            Some(bindings),
            "audit log must preserve the policy expression bindings"
        );
    }
}
