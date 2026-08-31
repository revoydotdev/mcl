//! Effect compliance checking at admission time.
//!
//! Verifies that action declarations (e.g., `effect: pure`) match actual
//! runtime behavior (e.g., whether the action mutates state). Violations
//! are reported with a severity appropriate to the execution mode.

use mcl_spec::document::ActionDef;
use mcl_spec::effects::Effect;
use mcl_spec::signal::MclSignal;

/// Enforcement severity level.
#[derive(Debug, Clone, PartialEq)]
pub enum Severity {
    /// Development mode — violations may panic for fast feedback.
    Dev,
    /// Production mode — violations are rejected gracefully.
    Prod,
}

/// Describes an effect compliance violation.
#[derive(Debug, Clone, PartialEq)]
pub struct EffectViolation {
    pub action: String,
    pub declared: Vec<Effect>,
    pub actual_behavior: String,
    pub severity: Severity,
}

/// Result of an effect compliance check.
#[derive(Debug, Clone, PartialEq)]
pub enum EffectCheckResult {
    Compliant,
    Violation(EffectViolation),
}

/// Check effect compliance in Dev mode.
///
/// An action declared `effect: pure` that carries non-empty `mutates`
/// is a violation with `Severity::Dev`.
pub fn check_effect_compliance(
    action: &ActionDef,
    signal: &MclSignal,
) -> Result<EffectCheckResult, EffectViolation> {
    check_effect_compliance_for_mode(action, signal, Severity::Dev)
}

/// Check effect compliance with an explicit severity mode.
///
/// - No effects declared → `Compliant` (backward compatibility)
/// - `Pure` with non-empty `mutates` → `Violation` with the given severity
/// - `StateMutating` with non-empty `mutates` → `Compliant`
/// - `Pure` with empty `mutates` → `Compliant`
pub fn check_effect_compliance_for_mode(
    action: &ActionDef,
    _signal: &MclSignal,
    mode: Severity,
) -> Result<EffectCheckResult, EffectViolation> {
    // Backward compatibility: no effects declared → always compliant
    if action.effects.is_empty() {
        return Ok(EffectCheckResult::Compliant);
    }

    let declares_pure = action.effects.contains(&Effect::Pure);
    let declares_state_mutating = action.effects.contains(&Effect::StateMutating);
    let mutates_state = !action.mutates.is_empty();

    if declares_pure && mutates_state {
        return Err(EffectViolation {
            action: action.name.clone(),
            declared: action.effects.clone(),
            actual_behavior: "action declared pure but carries state mutations".into(),
            severity: mode,
        });
    }

    if declares_state_mutating && mutates_state {
        return Ok(EffectCheckResult::Compliant);
    }

    // Pure with no mutations, or any other combination → compliant
    Ok(EffectCheckResult::Compliant)
}

/// Infer effects for an action that has no explicit effect declarations.
///
/// Rules:
/// - Has non-empty `mutates` → `vec![Effect::StateMutating]`
/// - Has `resolution` set → `vec![Effect::Dispatch(resolution_name)]`
/// - Otherwise → `vec![Effect::Pure]`
pub fn infer_effects(action: &ActionDef) -> Vec<Effect> {
    if !action.mutates.is_empty() {
        vec![Effect::StateMutating]
    } else if let Some(ref name) = action.resolution {
        vec![Effect::Dispatch(name.clone())]
    } else {
        vec![Effect::Pure]
    }
}

#[cfg(test)]
mod tests {
    use indexmap::IndexMap;
    use mcl_spec::effects::Effect;
    use mcl_spec::span::Span;
    use mcl_spec::value::MclValue;

    use super::*;

    fn test_action(
        name: &str,
        effects: Vec<Effect>,
        mutates: IndexMap<String, MclValue>,
    ) -> ActionDef {
        ActionDef {
            name: name.into(),
            requires: Vec::new(),
            requires_any: Vec::new(),
            target: None,
            mutates,
            resolution: None,
            mode: None,
            args: IndexMap::new(),
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects,
            span: Span::empty(),
        }
    }

    fn test_signal() -> MclSignal {
        MclSignal {
            kind: mcl_spec::signal::SignalKind::Noop,
            span: Span::empty(),
            labels: mcl_spec::label::LabelSet::default(),
        }
    }

    #[test]
    fn effect_pure_action_state_attempt_panics_dev() {
        let mut mutates = IndexMap::new();
        mutates.insert("player.hp".into(), MclValue::Int(0));

        let action = test_action("heal", vec![Effect::Pure], mutates);
        let signal = test_signal();

        let result = check_effect_compliance(&action, &signal);
        assert!(result.is_err());
        let violation = result.unwrap_err();
        assert_eq!(violation.action, "heal");
        assert_eq!(violation.severity, Severity::Dev);
        assert!(violation.actual_behavior.contains("pure"));
    }

    #[test]
    fn effect_pure_action_state_attempt_rejects_prod() {
        let mut mutates = IndexMap::new();
        mutates.insert("player.hp".into(), MclValue::Int(0));

        let action = test_action("heal", vec![Effect::Pure], mutates);
        let signal = test_signal();

        let result = check_effect_compliance_for_mode(&action, &signal, Severity::Prod);
        assert!(result.is_err());
        let violation = result.unwrap_err();
        assert_eq!(violation.action, "heal");
        assert_eq!(violation.severity, Severity::Prod);
    }

    #[test]
    fn effect_state_mutating_accepted() {
        let mut mutates = IndexMap::new();
        mutates.insert("player.hp".into(), MclValue::Int(100));

        let action = test_action("heal", vec![Effect::StateMutating], mutates);
        let signal = test_signal();

        let result = check_effect_compliance(&action, &signal);
        assert_eq!(result, Ok(EffectCheckResult::Compliant));
    }

    #[test]
    fn effect_no_declare_accepted() {
        let mut mutates = IndexMap::new();
        mutates.insert("player.hp".into(), MclValue::Int(100));

        // No effects declared → backward compat, always compliant
        let action = test_action("heal", Vec::new(), mutates);
        let signal = test_signal();

        let result = check_effect_compliance(&action, &signal);
        assert_eq!(result, Ok(EffectCheckResult::Compliant));
    }
}
