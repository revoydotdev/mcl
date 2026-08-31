//! Signal arbitration: validate incoming signals before they enter the registry.
//!
//! An [`Arbiter`] inspects a signal and its ingest context, returning a [`Verdict`].
//! Arbiters are composed into an [`ArbiterChain`] which short-circuits on the first
//! non-Accept verdict.

use std::collections::{HashSet, VecDeque};
use std::sync::Mutex;

use mcl_graph::id::NodeId;
use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::source::SourceId;

use crate::types::{SignalId, Verdict};

/// Context provided to arbiters describing where and when a signal was ingested.
#[derive(Debug, Clone)]
pub struct IngestContext {
    /// The node that emitted this signal.
    pub origin_node: NodeId,
    /// The super-step / turn number at emission time.
    pub turn: u64,
    /// The channel the signal was emitted on.
    pub channel: String,
    /// The source that produced this signal (defaults to `"system"`).
    pub source: SourceId,
}

impl Default for IngestContext {
    fn default() -> Self {
        Self {
            origin_node: NodeId::new("default"),
            turn: 0,
            channel: String::new(),
            source: SourceId::default(),
        }
    }
}

/// Trait for signal arbiters — synchronous validators that return a [`Verdict`].
pub trait Arbiter: Send + Sync {
    /// Inspect the signal and return a verdict.
    fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict;

    /// Human-readable name for diagnostics and logging.
    fn name(&self) -> &str;
}

/// Validates signal structure: required fields must be present and non-empty.
///
/// This is a permissive schema check — it only rejects signals with clearly
/// malformed structure (empty names, missing required strings). It does not
/// check semantic validity, deduplication, or action allowlists.
pub struct SchemaArbiter;

impl SchemaArbiter {
    /// Create a new schema arbiter.
    pub fn new() -> Self {
        Self
    }
}

impl Default for SchemaArbiter {
    fn default() -> Self {
        Self::new()
    }
}

impl Arbiter for SchemaArbiter {
    fn review(&self, signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
        match &signal.kind {
            SignalKind::Action { name, .. } if name.is_empty() => Verdict::Reject {
                reason: "action name must not be empty".to_string(),
            },
            SignalKind::Mutate { path, .. } if path.is_empty() => Verdict::Reject {
                reason: "mutate path must not be empty".to_string(),
            },
            SignalKind::Transition { context } if context.is_empty() => Verdict::Reject {
                reason: "transition context must not be empty".to_string(),
            },
            SignalKind::Dialogue { entity, text } => {
                if entity.is_empty() {
                    return Verdict::Reject {
                        reason: "dialogue entity must not be empty".to_string(),
                    };
                }
                if text.is_empty() {
                    return Verdict::Reject {
                        reason: "dialogue text must not be empty".to_string(),
                    };
                }
                Verdict::Accept
            }
            SignalKind::Narrate { tag } if tag.is_empty() => Verdict::Reject {
                reason: "narrate tag must not be empty".to_string(),
            },
            SignalKind::Query { path } if path.is_empty() => Verdict::Reject {
                reason: "query path must not be empty".to_string(),
            },
            SignalKind::Error { message } if message.is_empty() => Verdict::Reject {
                reason: "error message must not be empty".to_string(),
            },
            _ => Verdict::Accept,
        }
    }

    fn name(&self) -> &str {
        "schema"
    }
}

/// Runs a chain of arbiters in order, returning the first non-Accept verdict.
///
/// If every arbiter returns [`Verdict::Accept`], the chain returns [`Verdict::Accept`].
///
/// `pre_builtins` run BEFORE the built-in arbiters. These are typically
/// high-priority policies that need to intercept signals before standard
/// validation (e.g., schema checks) runs.
pub struct ArbiterChain {
    pre_builtins: Vec<Box<dyn Arbiter>>,
    arbiters: Vec<Box<dyn Arbiter>>,
}

impl ArbiterChain {
    /// Create an empty chain.
    pub fn new() -> Self {
        Self {
            pre_builtins: Vec::new(),
            arbiters: Vec::new(),
        }
    }

    /// Add an arbiter to the end of the chain.
    pub fn add(&mut self, arbiter: Box<dyn Arbiter>) {
        self.arbiters.push(arbiter);
    }

    /// Add an arbiter to run before built-in arbiters.
    ///
    /// Pre-builtins are evaluated in the order they're added, and all are run
    /// before any built-in arbiters. If any pre_builtin returns a non-Accept
    /// verdict, the chain short-circuits and built-in arbiters are not run.
    pub fn add_pre_builtin(&mut self, arbiter: Box<dyn Arbiter>) {
        self.pre_builtins.push(arbiter);
    }

    /// Run all arbiters in sequence, returning the first non-Accept verdict.
    ///
    /// Short-circuits as soon as any arbiter returns `Reject` or `Defer`.
    /// Pre-builtins are evaluated first, followed by the built-in chain.
    pub fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict {
        // Run pre-builtins first (high-priority policies, etc.)
        for arbiter in &self.pre_builtins {
            let verdict = arbiter.review(signal, ctx);
            if verdict != Verdict::Accept {
                return verdict;
            }
        }

        // Then run the built-in arbiters
        for arbiter in &self.arbiters {
            let verdict = arbiter.review(signal, ctx);
            if verdict != Verdict::Accept {
                return verdict;
            }
        }

        Verdict::Accept
    }
}

impl Default for ArbiterChain {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// DedupArbiter
// ---------------------------------------------------------------------------

/// Maintains a sliding window of recent [`SignalId`]s and rejects duplicates.
///
/// If an incoming signal produces the same [`SignalId`] as one already in the
/// window, the arbiter returns [`Verdict::Reject`]. Otherwise the ID is pushed
/// onto the window (which is trimmed to `window_size`).
pub struct DedupArbiter {
    window_size: usize,
    window: Mutex<VecDeque<SignalId>>,
}

impl DedupArbiter {
    /// Create a new dedup arbiter with the given sliding-window capacity.
    ///
    /// `window_size` of 0 effectively disables deduplication (nothing is
    /// retained, so every signal is accepted).
    pub fn new(window_size: usize) -> Self {
        Self {
            window_size,
            window: Mutex::new(VecDeque::new()),
        }
    }
}

impl Arbiter for DedupArbiter {
    fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict {
        let id = SignalId::from_signal(&ctx.origin_node, ctx.turn, &ctx.channel, signal);

        let mut window = self
            .window
            .lock()
            .expect("DedupArbiter mutex should not be poisoned");

        if window.contains(&id) {
            return Verdict::Reject {
                reason: "duplicate".to_string(),
            };
        }

        window.push_back(id);
        while window.len() > self.window_size {
            window.pop_front();
        }

        Verdict::Accept
    }

    fn name(&self) -> &str {
        "dedup"
    }
}

// ---------------------------------------------------------------------------
// HallucinationArbiter
// ---------------------------------------------------------------------------

/// Rejects action signals whose name is not in the declared allowed-actions set.
///
/// Non-action signals (mutate, transition, dialogue, etc.) always pass through.
/// This arbiter catches "hallucinated" actions — actions the LLM invents that
/// were never declared in the current context.
pub struct HallucinationArbiter {
    allowed_actions: HashSet<String>,
}

impl HallucinationArbiter {
    /// Create a new arbiter with the given set of allowed action names.
    pub fn new(allowed_actions: HashSet<String>) -> Self {
        Self { allowed_actions }
    }
}

impl Arbiter for HallucinationArbiter {
    fn review(&self, signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
        if let SignalKind::Action { name, .. } = &signal.kind {
            if self.allowed_actions.contains(name) {
                Verdict::Accept
            } else {
                Verdict::Reject {
                    reason: format!("action not in context: {name}"),
                }
            }
        } else {
            Verdict::Accept
        }
    }

    fn name(&self) -> &str {
        "hallucination"
    }
}

// ---------------------------------------------------------------------------
// ActionWhitelistArbiter
// ---------------------------------------------------------------------------

/// Optional arbiter that only allows action signals matching an explicit whitelist.
///
/// Non-action signals always pass through. This is stricter than
/// [`HallucinationArbiter`] in intent (whitelist = "only these may ever fire")
/// but functionally similar.
pub struct ActionWhitelistArbiter {
    whitelist: HashSet<String>,
}

impl ActionWhitelistArbiter {
    /// Create a new arbiter with the given whitelist of action names.
    pub fn new(whitelist: HashSet<String>) -> Self {
        Self { whitelist }
    }
}

impl Arbiter for ActionWhitelistArbiter {
    fn review(&self, signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
        if let SignalKind::Action { name, .. } = &signal.kind {
            if self.whitelist.contains(name) {
                Verdict::Accept
            } else {
                Verdict::Reject {
                    reason: format!("action not whitelisted: {name}"),
                }
            }
        } else {
            Verdict::Accept
        }
    }

    fn name(&self) -> &str {
        "action_whitelist"
    }
}

#[cfg(test)]
mod tests {
    use mcl_spec::label::LabelSet;
    use mcl_spec::span::Span;

    use super::*;

    fn test_ctx() -> IngestContext {
        IngestContext {
            origin_node: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            source: SourceId::default(),
        }
    }

    fn test_span() -> Span {
        Span::new(0, 1, 1, 1)
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

    fn empty_action_signal() -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: String::new(),
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

    // --- SchemaArbiter tests ---

    #[test]
    fn schema_accepts_valid_action() {
        let arbiter = SchemaArbiter::new();
        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn schema_accepts_valid_mutate() {
        let arbiter = SchemaArbiter::new();
        let signal = mutate_signal("player.hp");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn schema_accepts_noop() {
        let arbiter = SchemaArbiter::new();
        let signal = noop_signal();
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn schema_rejects_empty_action_name() {
        let arbiter = SchemaArbiter::new();
        let signal = empty_action_signal();
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for empty action name, got {verdict:?}"
        );
    }

    #[test]
    fn schema_rejects_empty_mutate_path() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Mutate {
                path: String::new(),
                value: mcl_spec::value::MclValue::Int(0),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason.contains("mutate path")),
            "expected Reject for empty mutate path, got {verdict:?}"
        );
    }

    #[test]
    fn schema_rejects_empty_transition_context() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Transition {
                context: String::new(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for empty transition context"
        );
    }

    #[test]
    fn schema_rejects_empty_dialogue_entity() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Dialogue {
                entity: String::new(),
                text: "hello".to_string(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { reason } if reason.contains("entity")),
            "expected Reject for empty dialogue entity"
        );
    }

    #[test]
    fn schema_rejects_empty_dialogue_text() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Dialogue {
                entity: "guard".to_string(),
                text: String::new(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { reason } if reason.contains("text")),
            "expected Reject for empty dialogue text"
        );
    }

    #[test]
    fn schema_rejects_empty_narrate_tag() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Narrate { tag: String::new() },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for empty narrate tag"
        );
    }

    #[test]
    fn schema_rejects_empty_query_path() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Query {
                path: String::new(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for empty query path"
        );
    }

    #[test]
    fn schema_rejects_empty_error_message() {
        let arbiter = SchemaArbiter::new();
        let signal = MclSignal {
            kind: SignalKind::Error {
                message: String::new(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for empty error message"
        );
    }

    // --- ArbiterChain tests ---

    #[test]
    fn chain_accepts_when_all_pass() {
        let mut chain = ArbiterChain::new();
        chain.add(Box::new(SchemaArbiter::new()));
        chain.add(Box::new(SchemaArbiter::new()));

        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(chain.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn chain_short_circuits_on_reject() {
        /// Always-rejects arbiter for testing short-circuit behavior.
        struct RejectAll;

        impl Arbiter for RejectAll {
            fn review(&self, _signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
                Verdict::Reject {
                    reason: "rejected by RejectAll".to_string(),
                }
            }

            fn name(&self) -> &str {
                "reject_all"
            }
        }

        let mut chain = ArbiterChain::new();
        chain.add(Box::new(RejectAll));
        chain.add(Box::new(SchemaArbiter::new()));

        let signal = action_signal("attack");
        let ctx = test_ctx();

        let verdict = chain.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason == "rejected by RejectAll"),
            "expected Reject from first arbiter in chain, got {verdict:?}"
        );
    }

    #[test]
    fn chain_empty_accepts_everything() {
        let chain = ArbiterChain::new();
        let signal = action_signal("anything");
        let ctx = test_ctx();

        assert_eq!(chain.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn chain_short_circuits_on_defer() {
        /// Always-defers arbiter for testing Defer short-circuit.
        struct DeferAll;

        impl Arbiter for DeferAll {
            fn review(&self, _signal: &MclSignal, _ctx: &IngestContext) -> Verdict {
                Verdict::Defer
            }

            fn name(&self) -> &str {
                "defer_all"
            }
        }

        let mut chain = ArbiterChain::new();
        chain.add(Box::new(DeferAll));
        chain.add(Box::new(SchemaArbiter::new()));

        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(chain.review(&signal, &ctx), Verdict::Defer);
    }

    #[test]
    fn schema_arbiter_name() {
        let arbiter = SchemaArbiter::new();
        assert_eq!(arbiter.name(), "schema");
    }

    // --- DedupArbiter tests ---

    #[test]
    fn dedup_accepts_first_occurrence() {
        let arbiter = DedupArbiter::new(10);
        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn dedup_rejects_duplicate() {
        let arbiter = DedupArbiter::new(10);
        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason == "duplicate"),
            "expected Reject::duplicate, got {verdict:?}"
        );
    }

    #[test]
    fn dedup_window_slides_allowing_repeat_after_eviction() {
        let arbiter = DedupArbiter::new(2);
        let ctx = test_ctx();

        let sig_a = action_signal("attack");
        let sig_b = mutate_signal("player.hp");
        let sig_c = noop_signal();

        assert_eq!(arbiter.review(&sig_a, &ctx), Verdict::Accept);
        assert_eq!(arbiter.review(&sig_b, &ctx), Verdict::Accept);
        assert_eq!(arbiter.review(&sig_c, &ctx), Verdict::Accept);

        // sig_a should have been evicted from the window (size 2, now holds sig_b, sig_c)
        assert_eq!(
            arbiter.review(&sig_a, &ctx),
            Verdict::Accept,
            "evicted signal should be accepted again"
        );
    }

    #[test]
    fn dedup_different_signals_not_duplicates() {
        let arbiter = DedupArbiter::new(10);
        let ctx = test_ctx();

        let sig_a = action_signal("attack");
        let sig_b = action_signal("defend");

        assert_eq!(arbiter.review(&sig_a, &ctx), Verdict::Accept);
        assert_eq!(arbiter.review(&sig_b, &ctx), Verdict::Accept);
    }

    #[test]
    fn dedup_name() {
        let arbiter = DedupArbiter::new(10);
        assert_eq!(arbiter.name(), "dedup");
    }

    // --- HallucinationArbiter tests ---

    #[test]
    fn hallucination_accepts_allowed_action() {
        let allowed: HashSet<String> = ["attack".into(), "defend".into()].into_iter().collect();
        let arbiter = HallucinationArbiter::new(allowed);
        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn hallucination_rejects_unknown_action() {
        let allowed: HashSet<String> = ["attack".into()].into_iter().collect();
        let arbiter = HallucinationArbiter::new(allowed);
        let signal = action_signal("teleport");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason.contains("teleport")),
            "expected Reject for unknown action, got {verdict:?}"
        );
    }

    #[test]
    fn hallucination_accepts_non_action_signal() {
        let allowed: HashSet<String> = HashSet::new();
        let arbiter = HallucinationArbiter::new(allowed);
        let signal = noop_signal();
        let ctx = test_ctx();

        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "non-action signals should always be accepted"
        );
    }

    #[test]
    fn hallucination_accepts_mutate_signal() {
        let allowed: HashSet<String> = HashSet::new();
        let arbiter = HallucinationArbiter::new(allowed);
        let signal = mutate_signal("player.hp");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn hallucination_name() {
        let arbiter = HallucinationArbiter::new(HashSet::new());
        assert_eq!(arbiter.name(), "hallucination");
    }

    // --- ActionWhitelistArbiter tests ---

    #[test]
    fn whitelist_accepts_whitelisted_action() {
        let whitelist: HashSet<String> = ["attack".into(), "heal".into()].into_iter().collect();
        let arbiter = ActionWhitelistArbiter::new(whitelist);
        let signal = action_signal("attack");
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn whitelist_rejects_non_whitelisted_action() {
        let whitelist: HashSet<String> = ["attack".into()].into_iter().collect();
        let arbiter = ActionWhitelistArbiter::new(whitelist);
        let signal = action_signal("heal");
        let ctx = test_ctx();

        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason.contains("heal")),
            "expected Reject for non-whitelisted action, got {verdict:?}"
        );
    }

    #[test]
    fn whitelist_accepts_non_action_signal() {
        let whitelist: HashSet<String> = HashSet::new();
        let arbiter = ActionWhitelistArbiter::new(whitelist);
        let signal = noop_signal();
        let ctx = test_ctx();

        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "non-action signals should always be accepted"
        );
    }

    #[test]
    fn whitelist_accepts_dialogue_signal() {
        let whitelist: HashSet<String> = HashSet::new();
        let arbiter = ActionWhitelistArbiter::new(whitelist);
        let signal = MclSignal {
            kind: SignalKind::Dialogue {
                entity: "guard".to_string(),
                text: "Halt!".to_string(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let ctx = test_ctx();

        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn whitelist_name() {
        let arbiter = ActionWhitelistArbiter::new(HashSet::new());
        assert_eq!(arbiter.name(), "action_whitelist");
    }

    #[test]
    fn policy_priority_high_runs_before_builtins() {
        use mcl_policy::ast::CompiledPolicy;
        use mcl_spec::policy::{Event, Lit, PolicyExpr, PolicyPriority, PolicyViolationAction};

        let high_priority_policy = CompiledPolicy {
            name: "high_priority_block".to_string(),
            on: Event::SignalAdmit,
            match_expr: Some(PolicyExpr::Literal {
                value: Lit::Bool(true),
            }),
            require_exprs: vec![PolicyExpr::Literal {
                value: Lit::Bool(false),
            }],
            transform: None,
            on_violation: PolicyViolationAction::Reject {
                reason: "blocked by high-priority policy".to_string(),
            },
            priority: PolicyPriority::High,
        };

        let policy_arbiter = Box::new(super::super::policy_arbiter::PolicyArbiter::new(vec![
            high_priority_policy,
        ]));
        let schema_arbiter = Box::new(SchemaArbiter::new());

        let mut chain = ArbiterChain::new();
        chain.add_pre_builtin(policy_arbiter);
        chain.add(schema_arbiter);

        let signal = action_signal("valid_action");
        let ctx = test_ctx();

        let verdict = chain.review(&signal, &ctx);

        match &verdict {
            Verdict::Reject { reason } => {
                assert!(
                    reason.contains("high_priority_block"),
                    "expected rejection from high-priority policy, got: {reason}"
                );
                assert!(
                    reason.contains("blocked by high-priority policy"),
                    "expected high-priority policy reason, got: {reason}"
                );
            }
            other => panic!("expected Reject from pre-builtin policy, got {other:?}"),
        }
    }
}
