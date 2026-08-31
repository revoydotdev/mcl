use std::collections::HashMap;

use mcl_spec::document::ActionDef;
use mcl_spec::effects::Effect;
use mcl_spec::signal::{MclSignal, SignalKind};

/// Violation when an idempotency key is reused within a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IdempotencyViolation {
    /// The idempotency key that was reused
    pub key: String,
    /// The turn number where this key was first seen
    pub first_turn: u64,
    /// The signal_id of the first occurrence
    pub first_signal_id: String,
}

impl std::fmt::Display for IdempotencyViolation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "IdempotencyViolation: key '{}' was already used in turn {} by signal '{}'",
            self.key, self.first_turn, self.first_signal_id
        )
    }
}

impl std::error::Error for IdempotencyViolation {}

/// Tracks idempotency keys to prevent duplicate signals within a turn.
#[derive(Debug, Default)]
pub struct IdempotencyTracker {
    /// Maps idempotency key to (turn, signal_id) of first occurrence
    seen_keys: HashMap<String, (u64, String)>,
}

impl IdempotencyTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if a signal violates idempotency constraints.
    ///
    /// Returns:
    /// - `Ok(())` if the signal should be allowed (no violation)
    /// - `Err(IdempotencyViolation)` if the signal's idempotency key was already used
    ///
    /// Logic:
    /// 1. Find `Effect::IdempotencyKey { required, arg }` in action.effects
    /// 2. If not found or `required` is false → pass through (no enforcement)
    /// 3. Extract key:
    ///    - If `arg` is Some(key_name), look up `key_name` in signal's args (for Action kind)
    ///    - Else use signal kind name as the key
    /// 4. If key is empty → pass through (can't enforce without a key)
    /// 5. If key already seen → reject with IdempotencyViolation
    /// 6. Otherwise, record (turn, signal_id) and pass
    pub fn check_idempotency(
        &mut self,
        action: &ActionDef,
        signal: &MclSignal,
        turn: u64,
    ) -> Result<(), IdempotencyViolation> {
        let idempotency_effect = action.effects.iter().find_map(|e| match e {
            Effect::IdempotencyKey { required, arg } if *required => Some((required, arg)),
            _ => None,
        });

        let (required, arg_name_opt) = match idempotency_effect {
            Some((r, a)) => (*r, a),
            None => return Ok(()),
        };

        if !required {
            return Ok(());
        }

        let key = match &signal.kind {
            SignalKind::Action { name, args } => {
                if let Some(arg_name) = arg_name_opt {
                    args.get(arg_name)
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| name.clone())
                } else {
                    name.clone()
                }
            }
            _ => format!("{:?}", signal.kind),
        };

        if key.is_empty() {
            return Ok(());
        }

        let signal_id = format!("{:?}", signal.kind);
        if let Some((first_turn, first_signal_id)) = self.seen_keys.get(&key) {
            return Err(IdempotencyViolation {
                key,
                first_turn: *first_turn,
                first_signal_id: first_signal_id.clone(),
            });
        }

        self.seen_keys.insert(key, (turn, signal_id));
        Ok(())
    }

    pub fn reset(&mut self) {
        self.seen_keys.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::indexmap;
    use mcl_spec::span::Span;
    use mcl_spec::value::MclValue;

    fn test_span() -> Span {
        Span::new(0, 1, 1, 1)
    }

    fn create_action_with_idempotency(name: &str, required: bool, arg: Option<&str>) -> ActionDef {
        ActionDef {
            name: name.to_string(),
            requires: Vec::new(),
            requires_any: Vec::new(),
            target: None,
            mutates: indexmap! {},
            resolution: None,
            mode: None,
            args: indexmap! {},
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects: vec![Effect::IdempotencyKey {
                required,
                arg: arg.map(String::from),
            }],
            span: test_span(),
        }
    }

    fn create_action_signal(name: &str, args: indexmap::IndexMap<String, MclValue>) -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args,
            },
            span: test_span(),
            labels: Default::default(),
        }
    }

    #[test]
    fn effect_idempotency_key_dedup() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", true, Some("request_id"));

        let mut args1 = indexmap! {};
        args1.insert("request_id".into(), MclValue::String("req-123".into()));
        args1.insert("target".into(), MclValue::String("goblin".into()));
        let signal1 = create_action_signal("attack", args1);

        let result = tracker.check_idempotency(&action, &signal1, 1);
        assert!(result.is_ok());

        let mut args2 = indexmap! {};
        args2.insert("request_id".into(), MclValue::String("req-123".into()));
        args2.insert("target".into(), MclValue::String("orc".into()));
        let signal2 = create_action_signal("attack", args2);

        let result = tracker.check_idempotency(&action, &signal2, 1);
        assert!(result.is_err());

        let violation = result.unwrap_err();
        assert_eq!(violation.key, "req-123");
        assert_eq!(violation.first_turn, 1);
        assert!(violation.first_signal_id.contains("req-123"));
    }

    #[test]
    fn effect_idempotency_different_keys() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", true, Some("request_id"));

        let mut args1 = indexmap! {};
        args1.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal1 = create_action_signal("attack", args1);

        let result = tracker.check_idempotency(&action, &signal1, 1);
        assert!(result.is_ok());

        let mut args2 = indexmap! {};
        args2.insert("request_id".into(), MclValue::String("req-456".into()));
        let signal2 = create_action_signal("attack", args2);

        let result = tracker.check_idempotency(&action, &signal2, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn effect_idempotency_no_key_required() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", false, Some("request_id"));

        let mut args1 = indexmap! {};
        args1.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal1 = create_action_signal("attack", args1);

        let result = tracker.check_idempotency(&action, &signal1, 1);
        assert!(result.is_ok());

        let mut args2 = indexmap! {};
        args2.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal2 = create_action_signal("attack", args2);

        let result = tracker.check_idempotency(&action, &signal2, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn effect_idempotency_no_effect() {
        let mut tracker = IdempotencyTracker::new();
        let action = ActionDef {
            name: "attack".to_string(),
            requires: Vec::new(),
            requires_any: Vec::new(),
            target: None,
            mutates: indexmap! {},
            resolution: None,
            mode: None,
            args: indexmap! {},
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects: vec![Effect::StateMutating],
            span: test_span(),
        };

        let mut args = indexmap! {};
        args.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal = create_action_signal("attack", args);

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_ok());

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_ok());
    }

    #[test]
    fn effect_idempotency_key_missing_arg() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", true, Some("request_id"));

        let mut args = indexmap! {};
        args.insert("target".into(), MclValue::String("goblin".into()));
        let signal = create_action_signal("attack", args);

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_ok());

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_err());
    }

    #[test]
    fn effect_idempotency_no_arg_specified() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", true, None);

        let mut args = indexmap! {};
        args.insert("target".into(), MclValue::String("goblin".into()));
        let signal = create_action_signal("attack", args);

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_ok());

        let result = tracker.check_idempotency(&action, &signal, 1);
        assert!(result.is_err());
    }

    #[test]
    fn effect_idempotency_reset() {
        let mut tracker = IdempotencyTracker::new();
        let action = create_action_with_idempotency("attack", true, Some("request_id"));

        let mut args1 = indexmap! {};
        args1.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal1 = create_action_signal("attack", args1);

        let result = tracker.check_idempotency(&action, &signal1, 1);
        assert!(result.is_ok());

        tracker.reset();

        let mut args2 = indexmap! {};
        args2.insert("request_id".into(), MclValue::String("req-123".into()));
        let signal2 = create_action_signal("attack", args2);

        let result = tracker.check_idempotency(&action, &signal2, 2);
        assert!(result.is_ok());
    }

    #[test]
    fn effect_idempotency_violation_display() {
        let violation = IdempotencyViolation {
            key: "req-123".to_string(),
            first_turn: 5,
            first_signal_id: "Action { name: \"attack\", ... }".to_string(),
        };

        let display = format!("{}", violation);
        assert!(display.contains("req-123"));
        assert!(display.contains("5"));
        assert!(display.contains("attack"));
    }
}
