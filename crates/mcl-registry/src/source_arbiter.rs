//! Source trust arbiter: gates signal admission based on source trust level.
//!
//! [`SourceArbiter`] consults the [`SourceConfig`] to look up the trust level of
//! the source that produced a signal. Untrusted sources are only allowed to
//! emit actions from a configurable allowlist (default: safe read-only actions).

use std::collections::HashSet;

use mcl_spec::signal::{MclSignal, SignalKind};
use mcl_spec::source::{SourceConfig, SourceId, Trust};

use crate::arbiter::{Arbiter, IngestContext};
use crate::types::Verdict;

/// Default read-only actions allowed from untrusted sources.
const DEFAULT_UNTRUSTED_ALLOWLIST: &[&str] = &["examine", "look", "observe", "read"];

/// Arbiter that gates signal admission based on source trust level.
///
/// Looks up the source's [`Trust`] level from the [`SourceConfig`]. Sources
/// not present in the config default to [`Trust::System`] (fully trusted).
/// Untrusted sources may only emit actions on the allowlist.
pub struct SourceArbiter {
    config: SourceConfig,
    untrusted_allowlist: HashSet<String>,
}

impl SourceArbiter {
    /// Create a new source arbiter with the given source configuration.
    pub fn new(config: SourceConfig) -> Self {
        Self {
            config,
            untrusted_allowlist: DEFAULT_UNTRUSTED_ALLOWLIST
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
        }
    }

    /// Override the set of actions allowed from untrusted sources.
    pub fn with_untrusted_allowlist(mut self, actions: Vec<String>) -> Self {
        self.untrusted_allowlist = actions.into_iter().collect();
        self
    }

    /// Resolve the trust level for a source ID.
    ///
    /// Sources not found in the config default to [`Trust::System`].
    fn trust_for(&self, source: &SourceId) -> Trust {
        self.config
            .get(source)
            .map(|s| s.trust)
            .unwrap_or(Trust::System)
    }
}

impl Arbiter for SourceArbiter {
    fn review(&self, signal: &MclSignal, ctx: &IngestContext) -> Verdict {
        let trust = self.trust_for(&ctx.source);

        if trust == Trust::Untrusted {
            // Only action signals are gated; non-action signals pass.
            if let SignalKind::Action { name, .. } = &signal.kind {
                if !self.untrusted_allowlist.contains(name) {
                    return Verdict::Reject {
                        reason: format!("untrusted_source: action '{name}' not allowed"),
                    };
                }
            }
        }

        Verdict::Accept
    }

    fn name(&self) -> &str {
        "source"
    }
}

#[cfg(test)]
mod tests {
    use mcl_graph::id::NodeId;
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::SignalKind;
    use mcl_spec::source::{Source, SourceId, Trust};
    use mcl_spec::span::Span;

    use super::*;
    use crate::arbiter::Arbiter;

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

    fn noop_signal() -> MclSignal {
        MclSignal {
            kind: SignalKind::Noop,
            span: test_span(),
            labels: LabelSet::default(),
        }
    }

    fn ctx_with_source(source: &str) -> IngestContext {
        IngestContext {
            origin_node: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            source: SourceId::from(source),
        }
    }

    fn make_config(sources: Vec<(SourceId, Trust)>) -> SourceConfig {
        sources
            .into_iter()
            .map(|(id, trust)| {
                (
                    id.clone(),
                    Source {
                        id,
                        trust,
                        inherit_labels: Vec::new(),
                        verify: None,
                    },
                )
            })
            .collect()
    }

    #[test]
    fn source_arbiter_drops_untrusted_action() {
        let config = make_config(vec![(SourceId::from("webhook"), Trust::Untrusted)]);
        let arbiter = SourceArbiter::new(config);

        // Untrusted source with disallowed action → reject
        let signal = action_signal("transfer");
        let ctx = ctx_with_source("webhook");
        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { ref reason } if reason.contains("untrusted_source")),
            "expected Reject for untrusted 'transfer', got {verdict:?}"
        );

        // Untrusted source with allowlisted action → accept
        let signal = action_signal("examine");
        let verdict = arbiter.review(&signal, &ctx);
        assert_eq!(verdict, Verdict::Accept);
    }

    #[test]
    fn source_arbiter_default_trust_system() {
        // Empty config — source not found defaults to System trust
        let config = SourceConfig::new();
        let arbiter = SourceArbiter::new(config);

        let signal = action_signal("transfer");
        let ctx = ctx_with_source("unknown_source");
        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "unknown source should default to System trust and be admitted"
        );
    }

    #[test]
    fn source_arbiter_trusted_source_all_actions() {
        let config = make_config(vec![(SourceId::from("player"), Trust::User)]);
        let arbiter = SourceArbiter::new(config);

        let signal = action_signal("transfer");
        let ctx = ctx_with_source("player");
        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);
    }

    #[test]
    fn source_arbiter_non_action_passes_untrusted() {
        let config = make_config(vec![(SourceId::from("webhook"), Trust::Untrusted)]);
        let arbiter = SourceArbiter::new(config);

        // Non-action signals always pass, even from untrusted sources
        let signal = noop_signal();
        let ctx = ctx_with_source("webhook");
        assert_eq!(
            arbiter.review(&signal, &ctx),
            Verdict::Accept,
            "non-action signals should pass even from untrusted sources"
        );
    }

    #[test]
    fn source_arbiter_name() {
        let arbiter = SourceArbiter::new(SourceConfig::new());
        assert_eq!(arbiter.name(), "source");
    }

    #[test]
    fn source_arbiter_custom_allowlist() {
        let config = make_config(vec![(SourceId::from("api"), Trust::Untrusted)]);
        let arbiter = SourceArbiter::new(config).with_untrusted_allowlist(vec!["ping".to_string()]);

        let ctx = ctx_with_source("api");

        // "ping" is on custom allowlist → accept
        let signal = action_signal("ping");
        assert_eq!(arbiter.review(&signal, &ctx), Verdict::Accept);

        // "examine" is on default allowlist but NOT on custom → reject
        let signal = action_signal("examine");
        let verdict = arbiter.review(&signal, &ctx);
        assert!(
            matches!(verdict, Verdict::Reject { .. }),
            "expected Reject for 'examine' with custom allowlist, got {verdict:?}"
        );
    }
}
