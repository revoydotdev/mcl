use mcl_spec::document::MclDocument;
use mcl_spec::signal::{MclSignal, SignalKind};

/// Validation result for a signal against a context
#[derive(Debug)]
pub enum SignalValidation {
    Valid,
    Warning(String),
    Invalid(String),
}

/// Validate a signal against the current document context
pub fn validate_signal(signal: &MclSignal, doc: &MclDocument) -> SignalValidation {
    match &signal.kind {
        SignalKind::Action { name, args } => {
            if !doc.actions.contains_key(name) {
                let available: Vec<&str> = doc.actions.keys().map(|s| s.as_str()).collect();
                return SignalValidation::Invalid(format!(
                    "unknown action `{name}`. Available: {}",
                    available.join(", ")
                ));
            }

            // Validate target if action specifies one
            let action = &doc.actions[name];
            if action.target.is_some() && !args.contains_key("target") {
                return SignalValidation::Warning(format!(
                    "action `{name}` expects a target argument"
                ));
            }

            SignalValidation::Valid
        }

        SignalKind::Transition { context } => {
            let valid_targets: Vec<&str> =
                doc.transitions.iter().map(|t| t.target.as_str()).collect();
            if !valid_targets.contains(&context.as_str()) {
                return SignalValidation::Invalid(format!(
                    "unknown transition target `{context}`. Available: {}",
                    valid_targets.join(", ")
                ));
            }
            SignalValidation::Valid
        }

        SignalKind::Dialogue { entity, .. } => {
            if !doc.entities.contains_key(entity) {
                return SignalValidation::Warning(format!(
                    "dialogue with unknown entity `{entity}`"
                ));
            }
            SignalValidation::Valid
        }

        SignalKind::Mutate { path, .. } => {
            // Check if path refers to a known state variable
            let root = path.split('.').next().unwrap_or(path);
            if !doc.state.contains_key(root) {
                return SignalValidation::Warning(format!(
                    "mutating unknown state variable `{root}`"
                ));
            }
            SignalValidation::Valid
        }

        // These signals are always valid
        SignalKind::Noop
        | SignalKind::Narrate { .. }
        | SignalKind::Query { .. }
        | SignalKind::Error { .. } => SignalValidation::Valid,
    }
}
