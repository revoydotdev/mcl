//! Effect types for action side effects and runtime reasoning.
//!
//! Effects describe the side effects that an action may have, enabling
//! the runtime to reason about purity, state mutation, external interactions,
//! caching behavior, and replay safety.

use serde::{Deserialize, Serialize};

/// Effect declared on an action. Describes side effects for runtime reasoning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    Pure,
    StateMutating,
    External(Vec<EffectKind>),
    Dispatch(String),
    AuditVerbose,
    AuditMinimal,
    Cacheable,
    IdempotencyKey { required: bool, arg: Option<String> },
    ReplaySafe,
    ReplayUnsafe,
}

/// Kinds of external effects.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Io,
    Network,
    Filesystem,
    Time,
    Random,
    Deterministic,
    Nondeterministic,
}

/// A set of effects on an action, with convenience query methods.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EffectSet(pub Vec<Effect>);

impl EffectSet {
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn is_pure(&self) -> bool {
        self.0.contains(&Effect::Pure)
    }

    pub fn is_state_mutating(&self) -> bool {
        self.0.contains(&Effect::StateMutating)
    }

    pub fn has_external(&self) -> bool {
        self.0.iter().any(|e| matches!(e, Effect::External(_)))
    }

    pub fn is_cacheable(&self) -> bool {
        self.0.contains(&Effect::Cacheable)
    }

    pub fn is_replay_safe(&self) -> bool {
        self.0.contains(&Effect::ReplaySafe) && !self.0.contains(&Effect::ReplayUnsafe)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn effect_pure_roundtrip() {
        let effect = Effect::Pure;
        let json = serde_json::to_string(&effect).expect("serialize");
        assert_eq!(json, "\"pure\"");
        let back: Effect = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(effect, back);
    }

    #[test]
    fn effect_state_mutating_roundtrip() {
        let effect = Effect::StateMutating;
        let json = serde_json::to_string(&effect).expect("serialize");
        assert_eq!(json, "\"state_mutating\"");
        let back: Effect = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(effect, back);
    }

    #[test]
    fn effect_set_pure_detection() {
        let pure_set = EffectSet(vec![Effect::Pure]);
        assert!(pure_set.is_pure());

        let state_set = EffectSet(vec![Effect::StateMutating]);
        assert!(!state_set.is_pure());
    }

    #[test]
    fn effect_external_kinds() {
        let effect = Effect::External(vec![EffectKind::Network, EffectKind::Io]);
        let json = serde_json::to_string(&effect).expect("serialize");
        let back: Effect = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(effect, back);
        assert!(json.contains("\"network\""));
        assert!(json.contains("\"io\""));
    }

    #[test]
    fn effect_idempotency_key() {
        let effect = Effect::IdempotencyKey {
            required: true,
            arg: Some("request_id".into()),
        };
        let json = serde_json::to_string(&effect).expect("serialize");
        let back: Effect = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(effect, back);
        assert!(json.contains("\"required\":true"));
        assert!(json.contains("\"arg\":\"request_id\""));
    }

    #[test]
    fn effect_dispatch_roundtrip() {
        let effect = Effect::Dispatch("event:name".into());
        let json = serde_json::to_string(&effect).expect("serialize");
        assert_eq!(json, "{\"dispatch\":\"event:name\"}");
        let back: Effect = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(effect, back);
    }

    #[test]
    fn effect_set_empty() {
        let set = EffectSet::default();
        assert!(set.is_empty());
        assert!(!set.is_pure());
        assert!(!set.is_state_mutating());
        assert!(!set.has_external());
        assert!(!set.is_cacheable());
        assert!(!set.is_replay_safe());
    }

    #[test]
    fn effect_set_state_mutating() {
        let set = EffectSet(vec![Effect::StateMutating]);
        assert!(!set.is_empty());
        assert!(!set.is_pure());
        assert!(set.is_state_mutating());
    }

    #[test]
    fn effect_set_has_external() {
        let set = EffectSet(vec![Effect::External(vec![EffectKind::Network])]);
        assert!(!set.is_empty());
        assert!(set.has_external());
    }

    #[test]
    fn effect_set_cacheable() {
        let set = EffectSet(vec![Effect::Pure, Effect::Cacheable]);
        assert!(set.is_pure());
        assert!(set.is_cacheable());
    }

    #[test]
    fn effect_set_replay_safe() {
        let safe_set = EffectSet(vec![Effect::ReplaySafe]);
        assert!(safe_set.is_replay_safe());

        let unsafe_set = EffectSet(vec![Effect::ReplayUnsafe]);
        assert!(!unsafe_set.is_replay_safe());

        let mixed_set = EffectSet(vec![Effect::ReplaySafe, Effect::ReplayUnsafe]);
        assert!(!mixed_set.is_replay_safe());
    }

    #[test]
    fn effect_set_serialization() {
        let set = EffectSet(vec![
            Effect::Pure,
            Effect::StateMutating,
            Effect::External(vec![EffectKind::Network]),
        ]);
        let json = serde_json::to_string(&set).expect("serialize");
        let back: EffectSet = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(set, back);
    }
}
