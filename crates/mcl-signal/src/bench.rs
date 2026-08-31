/// Signal reliability benchmark harness.
///
/// Measures how reliably an LLM emits correct MCL signals given various
/// player inputs. Each benchmark scenario specifies expected signal output,
/// and `classify_turn()` compares actual LLM responses against expectations.
use mcl_spec::signal::SignalKind;

use crate::extract::extract_signals;

// ── Types ──────────────────────────────────────────────────────

/// Classification of a single LLM turn against expected output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TurnClassification {
    /// LLM emitted the expected signal (type and name match).
    Correct,
    /// LLM emitted an MCL:: line but it failed to parse (warnings present).
    Malformed,
    /// LLM narrated an action but emitted no signal at all.
    Missing,
    /// LLM emitted a signal for an action not in the available actions list
    /// (and not a universal signal like dialogue/noop/transition).
    Hallucinated,
}

impl std::fmt::Display for TurnClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TurnClassification::Correct => write!(f, "Correct"),
            TurnClassification::Malformed => write!(f, "Malformed"),
            TurnClassification::Missing => write!(f, "Missing"),
            TurnClassification::Hallucinated => write!(f, "Hallucinated"),
        }
    }
}

/// Aggregated reliability report for a model across all benchmark runs.
#[derive(Debug, Clone)]
pub struct ReliabilityReport {
    pub model: String,
    pub total_turns: usize,
    pub correct_signals: usize,
    pub malformed_signals: usize,
    pub missing_signals: usize,
    pub hallucinated_signals: usize,
}

impl ReliabilityReport {
    pub fn new(model: &str) -> Self {
        Self {
            model: model.to_string(),
            total_turns: 0,
            correct_signals: 0,
            malformed_signals: 0,
            missing_signals: 0,
            hallucinated_signals: 0,
        }
    }

    pub fn record(&mut self, classification: &TurnClassification) {
        self.total_turns += 1;
        match classification {
            TurnClassification::Correct => self.correct_signals += 1,
            TurnClassification::Malformed => self.malformed_signals += 1,
            TurnClassification::Missing => self.missing_signals += 1,
            TurnClassification::Hallucinated => self.hallucinated_signals += 1,
        }
    }

    /// Accuracy as a percentage (0.0 – 100.0). Returns 0.0 if no turns.
    pub fn accuracy(&self) -> f64 {
        if self.total_turns == 0 {
            return 0.0;
        }
        self.correct_signals as f64 / self.total_turns as f64 * 100.0
    }
}

/// A single benchmark scenario: player input → expected signal.
#[derive(Debug, Clone)]
pub struct BenchmarkScenario {
    /// The player's natural-language input that gets sent to the LLM.
    pub user_input: &'static str,
    /// Expected signal type: "action", "dialogue", "transition", "noop", or None.
    pub expected_signal_type: Option<&'static str>,
    /// Expected signal name: e.g. "attack", "skeleton_guard", "crypt_inner", "noop", or None.
    pub expected_signal_name: Option<&'static str>,
    /// Human-readable description for reporting.
    pub description: &'static str,
}

// ── Classifier ─────────────────────────────────────────────────

/// Classify a single LLM response turn against expected signal output.
///
/// # Arguments
/// * `raw_response` — the raw text returned by the LLM
/// * `expected_type` — expected signal type (e.g. "action", "dialogue")
/// * `expected_name` — expected signal name (e.g. "attack", "skeleton_guard")
/// * `available_actions` — list of action names valid in the current context
pub fn classify_turn(
    raw_response: &str,
    expected_type: Option<&str>,
    expected_name: Option<&str>,
    available_actions: &[&str],
) -> TurnClassification {
    let parsed = extract_signals(raw_response);

    if !parsed.warnings.is_empty() {
        return TurnClassification::Malformed;
    }

    if parsed.signals.is_empty() {
        if expected_type.is_none() || expected_type == Some("noop") || expected_name == Some("noop")
        {
            return TurnClassification::Correct;
        }
        return TurnClassification::Missing;
    }

    let signal = &parsed.signals[0];

    if let SignalKind::Action { name, .. } = &signal.kind {
        if !available_actions.contains(&name.as_str()) {
            return TurnClassification::Hallucinated;
        }
    }

    let matches = match (&signal.kind, expected_type, expected_name) {
        (SignalKind::Noop, None, _) | (SignalKind::Noop, Some("noop"), _) => true,

        (SignalKind::Action { name, .. }, Some("action"), Some(exp_name)) => {
            name == exp_name || (exp_name == "noop" && name == "noop")
        }
        (SignalKind::Action { name, .. }, Some("action"), None) => {
            available_actions.contains(&name.as_str())
        }

        (SignalKind::Dialogue { entity, .. }, Some("dialogue"), Some(exp_entity)) => {
            entity == exp_entity
        }
        (SignalKind::Dialogue { .. }, Some("dialogue"), None) => true,

        (SignalKind::Transition { context }, Some("transition"), Some(exp_ctx)) => {
            context == exp_ctx
        }
        (SignalKind::Transition { .. }, Some("transition"), None) => true,

        _ => false,
    };

    if matches {
        TurnClassification::Correct
    } else {
        TurnClassification::Missing
    }
}

// ── Default Scenarios ──────────────────────────────────────────

/// 20 benchmark scenarios based on the crypt_antechamber context.
pub static DEFAULT_SCENARIOS: &[BenchmarkScenario] = &[
    // 1-4: Attack actions
    BenchmarkScenario {
        user_input: "I swing my sword at the skeleton!",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("attack"),
        description: "Direct attack on skeleton guard",
    },
    BenchmarkScenario {
        user_input: "Attack the skeleton guard with my weapon",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("attack"),
        description: "Explicit attack with target",
    },
    BenchmarkScenario {
        user_input: "I lunge at the guard, trying to strike a fatal blow",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("attack"),
        description: "Narrative attack intent",
    },
    BenchmarkScenario {
        user_input: "Hit the skeleton",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("attack"),
        description: "Brief attack command",
    },
    // 5-6: Look action
    BenchmarkScenario {
        user_input: "Look around the room",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("look"),
        description: "Standard look command",
    },
    BenchmarkScenario {
        user_input: "What do I see here?",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("look"),
        description: "Look via question",
    },
    // 7-8: Light torch
    BenchmarkScenario {
        user_input: "I use the flint to light the torch on the wall",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("light_torch"),
        description: "Light torch with flint",
    },
    BenchmarkScenario {
        user_input: "Light the sconce",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("light_torch"),
        description: "Brief light torch command",
    },
    // 9-10: Examine
    BenchmarkScenario {
        user_input: "Examine the skeleton guard closely",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("examine"),
        description: "Examine enemy entity",
    },
    BenchmarkScenario {
        user_input: "I inspect the rusty key on the floor",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("examine"),
        description: "Examine item",
    },
    // 11-12: Take / Pickup
    BenchmarkScenario {
        user_input: "Pick up the flint",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("pickup"),
        description: "Pick up flint",
    },
    BenchmarkScenario {
        user_input: "I grab the rusty key from the ground",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("pickup"),
        description: "Pick up key",
    },
    // 13-14: Use item / Unlock
    BenchmarkScenario {
        user_input: "I try to unlock the door with the rusty key",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("unlock_door"),
        description: "Unlock door with key",
    },
    BenchmarkScenario {
        user_input: "Use the key on the northern door",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("unlock_door"),
        description: "Use item on door",
    },
    // 15-16: Transition
    BenchmarkScenario {
        user_input: "Go north through the door",
        expected_signal_type: Some("transition"),
        expected_signal_name: Some("crypt_inner"),
        description: "Transition to crypt inner",
    },
    BenchmarkScenario {
        user_input: "I walk through the iron door to the north",
        expected_signal_type: Some("transition"),
        expected_signal_name: Some("crypt_inner"),
        description: "Narrative transition",
    },
    // 17-18: Dialogue with entity
    BenchmarkScenario {
        user_input: "I call out to the skeleton guard",
        expected_signal_type: Some("dialogue"),
        expected_signal_name: Some("skeleton_guard"),
        description: "Dialogue with skeleton guard",
    },
    BenchmarkScenario {
        user_input: "\"Hello? Is anyone there?\"",
        expected_signal_type: Some("dialogue"),
        expected_signal_name: Some("skeleton_guard"),
        description: "Dialogue via quoted speech",
    },
    // 19: Noop for non-action input
    BenchmarkScenario {
        user_input: "I sit down and think about my life choices",
        expected_signal_type: Some("noop"),
        expected_signal_name: Some("noop"),
        description: "Non-action input → noop",
    },
    // 20: Edge case — ambiguous multi-word entity
    BenchmarkScenario {
        user_input: "I examine the rusty key near the skeleton",
        expected_signal_type: Some("action"),
        expected_signal_name: Some("examine"),
        description: "Multi-word entity reference in examine",
    },
];

// ── Tests ──────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    const CRYPT_ACTIONS: &[&str] = &[
        "look",
        "examine",
        "attack",
        "pickup",
        "light_torch",
        "unlock_door",
    ];

    #[test]
    fn classify_correct_signal() {
        let response = r#"You swing your blade at the undead guardian!
MCL::action(attack, target=skeleton_guard)"#;
        let result = classify_turn(response, Some("action"), Some("attack"), CRYPT_ACTIONS);
        assert_eq!(result, TurnClassification::Correct);
    }

    #[test]
    fn classify_malformed_signal() {
        let response = r#"You try to attack.
MCL::action_attack(skeleton_guard)"#;
        let result = classify_turn(response, Some("action"), Some("attack"), CRYPT_ACTIONS);
        assert_eq!(result, TurnClassification::Malformed);
    }

    #[test]
    fn classify_missing_signal() {
        let response =
            "You swing your sword at the skeleton, striking its rusted armor with a clang!";
        let result = classify_turn(response, Some("action"), Some("attack"), CRYPT_ACTIONS);
        assert_eq!(result, TurnClassification::Missing);
    }

    #[test]
    fn classify_hallucinated_signal() {
        let response = r#"You cast a powerful spell!
MCL::action(cast_fireball, target=skeleton_guard)"#;
        let result = classify_turn(
            response,
            Some("action"),
            Some("cast_fireball"),
            CRYPT_ACTIONS,
        );
        assert_eq!(result, TurnClassification::Hallucinated);
    }

    #[test]
    fn classify_noop_when_no_action_expected() {
        let response = "You sit quietly and reflect on your journey.";
        let result = classify_turn(response, Some("noop"), Some("noop"), CRYPT_ACTIONS);
        assert_eq!(result, TurnClassification::Correct);
    }

    #[test]
    fn classify_dialogue_signal() {
        let response = r#"You call out to the skeletal figure.
MCL::dialogue(skeleton_guard, "Who are you?")"#;
        let result = classify_turn(
            response,
            Some("dialogue"),
            Some("skeleton_guard"),
            CRYPT_ACTIONS,
        );
        assert_eq!(result, TurnClassification::Correct);
    }

    #[test]
    fn classify_transition_signal() {
        let response = r#"You push open the heavy iron door and step through.
MCL::transition(crypt_inner)"#;
        let result = classify_turn(
            response,
            Some("transition"),
            Some("crypt_inner"),
            CRYPT_ACTIONS,
        );
        assert_eq!(result, TurnClassification::Correct);
    }

    #[test]
    fn report_accuracy_calculation() {
        let mut report = ReliabilityReport::new("test-model");
        for _ in 0..7 {
            report.record(&TurnClassification::Correct);
        }
        for _ in 0..2 {
            report.record(&TurnClassification::Missing);
        }
        report.record(&TurnClassification::Malformed);

        assert_eq!(report.total_turns, 10);
        assert_eq!(report.correct_signals, 7);
        assert!((report.accuracy() - 70.0).abs() < f64::EPSILON);
    }

    #[test]
    fn default_scenarios_count() {
        assert_eq!(DEFAULT_SCENARIOS.len(), 20);
    }

    #[test]
    fn default_scenarios_use_crypt_context() {
        // All scenarios should reference crypt-demo entities/actions
        let has_crypt_ref = DEFAULT_SCENARIOS.iter().any(|s| {
            s.description.contains("skeleton")
                || s.description.contains("crypt")
                || s.description.contains("torch")
                || s.description.contains("key")
                || s.description.contains("flint")
                || s.description.contains("door")
        });
        assert!(
            has_crypt_ref,
            "scenarios should reference crypt demo entities"
        );

        // Check specific action references
        let has_attack = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("attack"));
        let has_look = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("look"));
        let has_light_torch = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("light_torch"));
        let has_transition = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("crypt_inner"));
        let has_dialogue = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("skeleton_guard"));
        let has_noop = DEFAULT_SCENARIOS
            .iter()
            .any(|s| s.expected_signal_name == Some("noop"));

        assert!(has_attack, "should have attack scenario");
        assert!(has_look, "should have look scenario");
        assert!(has_light_torch, "should have light_torch scenario");
        assert!(has_transition, "should have transition scenario");
        assert!(has_dialogue, "should have dialogue scenario");
        assert!(has_noop, "should have noop scenario");
    }

    #[test]
    fn classify_empty_response_as_correct_for_noop() {
        let result = classify_turn("", Some("noop"), Some("noop"), CRYPT_ACTIONS);
        assert_eq!(result, TurnClassification::Correct);
    }

    #[test]
    fn classify_narration_only_as_missing_for_action() {
        let result = classify_turn(
            "The room is dark and cold.",
            Some("action"),
            Some("look"),
            CRYPT_ACTIONS,
        );
        assert_eq!(result, TurnClassification::Missing);
    }

    #[test]
    fn report_empty_is_zero_accuracy() {
        let report = ReliabilityReport::new("empty");
        assert_eq!(report.total_turns, 0);
        assert!((report.accuracy() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn report_records_all_classifications() {
        let mut report = ReliabilityReport::new("test");
        report.record(&TurnClassification::Correct);
        report.record(&TurnClassification::Malformed);
        report.record(&TurnClassification::Missing);
        report.record(&TurnClassification::Hallucinated);

        assert_eq!(report.total_turns, 4);
        assert_eq!(report.correct_signals, 1);
        assert_eq!(report.malformed_signals, 1);
        assert_eq!(report.missing_signals, 1);
        assert_eq!(report.hallucinated_signals, 1);
    }

    #[test]
    fn classification_display() {
        assert_eq!(TurnClassification::Correct.to_string(), "Correct");
        assert_eq!(TurnClassification::Malformed.to_string(), "Malformed");
        assert_eq!(TurnClassification::Missing.to_string(), "Missing");
        assert_eq!(TurnClassification::Hallucinated.to_string(), "Hallucinated");
    }
}
