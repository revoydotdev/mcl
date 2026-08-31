use indexmap::IndexMap;
use mcl_runtime::resolver::ActionResolver;
use mcl_runtime::state::WorldState;
use mcl_spec::document::ActionDef;
use mcl_spec::signal::{MclSignal, Resolution, ResolutionOutcome, SignalKind};
use mcl_spec::value::MclValue;
use mcl_spec::MclError;
use rand::Rng;
use rand::RngCore;

/// Simple d20-style combat resolver
pub struct CombatResolver;

impl ActionResolver for CombatResolver {
    fn system_name(&self) -> &str {
        "combat_system"
    }

    fn resolve(
        &self,
        action: &ActionDef,
        signal: &MclSignal,
        state: &WorldState,
        rng: &mut dyn RngCore,
    ) -> Result<Resolution, MclError> {
        let target_id = extract_target(signal)
            .ok_or_else(|| MclError::Resolver("combat action requires a target".to_string()))?;

        // Get attacker stats (player defaults)
        let attack_bonus = state
            .get_by_str("player.attack")
            .and_then(|v| v.as_int())
            .unwrap_or(5);

        // Get defender stats
        let target_hp = state
            .get_by_str(&format!("entities.{target_id}.hp"))
            .and_then(|v| v.as_int())
            .unwrap_or(10);

        let target_ac = state
            .get_by_str(&format!("entities.{target_id}.ac"))
            .and_then(|v| v.as_int())
            .unwrap_or(10);

        // Roll d20 + attack bonus vs AC
        let roll: i64 = rng.gen_range(1..=20);
        let total = roll + attack_bonus;
        let hit = total >= target_ac;

        let mut details = IndexMap::new();
        details.insert("roll".to_string(), MclValue::Int(roll));
        details.insert("total".to_string(), MclValue::Int(total));
        details.insert("target_ac".to_string(), MclValue::Int(target_ac));
        details.insert("target".to_string(), MclValue::String(target_id.clone()));

        if hit {
            // Roll damage (d6 + str modifier)
            let damage: i64 = rng.gen_range(1..=6);
            let new_hp = (target_hp - damage).max(0);

            details.insert("damage".to_string(), MclValue::Int(damage));
            details.insert("target_hp_remaining".to_string(), MclValue::Int(new_hp));
            details.insert(
                format!("set:entities.{target_id}.hp"),
                MclValue::Int(new_hp),
            );

            let killed = new_hp <= 0;
            details.insert("killed".to_string(), MclValue::Bool(killed));

            let hint = if killed {
                format!("The attack hits for {damage} damage, killing the {target_id}!")
            } else {
                format!("The attack hits for {damage} damage. The {target_id} has {new_hp} HP remaining.")
            };

            Ok(Resolution {
                action: action.name.clone(),
                outcome: ResolutionOutcome::Success,
                details,
                narration_hint: Some(hint),
            })
        } else {
            let hint = format!(
                "The attack misses the {target_id}. (rolled {roll} + {attack_bonus} = {total} vs AC {target_ac})"
            );

            Ok(Resolution {
                action: action.name.clone(),
                outcome: ResolutionOutcome::Failure,
                details,
                narration_hint: Some(hint),
            })
        }
    }
}

fn extract_target(signal: &MclSignal) -> Option<String> {
    if let SignalKind::Action { args, .. } = &signal.kind {
        args.get("target")
            .and_then(|v| v.as_str().map(|s| s.to_string()))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::label::LabelSet;
    use mcl_spec::span::Span;
    use rand::rngs::mock::StepRng;

    fn test_span() -> Span {
        Span::new(0, 1, 1, 1)
    }

    fn test_state(
        entity_id: &str,
        entity_hp: i64,
        entity_ac: i64,
        player_attack: Option<i64>,
    ) -> WorldState {
        let mut state = WorldState::new();
        let mut entity = indexmap::IndexMap::new();
        entity.insert("hp".to_string(), MclValue::Int(entity_hp));
        entity.insert("ac".to_string(), MclValue::Int(entity_ac));
        state.entities.insert(entity_id.to_string(), entity);
        if let Some(atk) = player_attack {
            state
                .player
                .insert("attack".to_string(), MclValue::Int(atk));
        }
        state
    }

    fn attack_signal(entity_id: &str) -> MclSignal {
        let mut args = indexmap::IndexMap::new();
        args.insert(
            "target".to_string(),
            MclValue::String(entity_id.to_string()),
        );
        MclSignal {
            kind: SignalKind::Action {
                name: "attack".to_string(),
                args,
            },
            span: test_span(),
            labels: LabelSet::default(),
        }
    }

    fn attack_action() -> ActionDef {
        ActionDef {
            name: "attack".to_string(),
            requires: vec![],
            requires_any: vec![],
            target: None,
            mutates: indexmap::IndexMap::new(),
            resolution: Some("combat_system".to_string()),
            mode: None,
            args: indexmap::IndexMap::new(),
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects: Vec::new(),
            span: test_span(),
        }
    }

    // Deterministic RNG seeds (derived from rand 0.8 uniform int sampling):
    //   StepRng(initial, 0) with gen_range(1..=20) produces roll = 15
    const D20_HIT_SEED: u64 = 12_912_720_851_596_686_132;
    //   StepRng(initial, 0) with gen_range(1..=20) produces roll = 5
    const D20_MISS_SEED: u64 = 3_689_348_814_741_910_324;
    //   Increment to go from d20=15 seed to d6=3 seed
    const INC_D6_3: u64 = 11_682_937_913_349_382_690;
    //   Increment to go from d20=15 seed to d6=2 seed
    const INC_D6_2: u64 = 8_608_480_567_731_124_087;

    #[test]
    fn attack_roll_hits_when_exceeding_ac() {
        let resolver = CombatResolver;
        let action = attack_action();
        let signal = attack_signal("skeleton_guard");
        let state = test_state("skeleton_guard", 10, 12, Some(5));

        // 15 + 5 = 20 >= 12 -> hit, damage = 3
        let mut rng = StepRng::new(D20_HIT_SEED, INC_D6_3);
        let result = resolver
            .resolve(&action, &signal, &state, &mut rng)
            .unwrap();

        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert_eq!(result.details.get("roll"), Some(&MclValue::Int(15)));
        assert_eq!(result.details.get("total"), Some(&MclValue::Int(20)));
        assert_eq!(result.details.get("damage"), Some(&MclValue::Int(3)));
        assert_eq!(
            result.details.get("target_hp_remaining"),
            Some(&MclValue::Int(7))
        );
        assert_eq!(result.details.get("killed"), Some(&MclValue::Bool(false)));
    }

    #[test]
    fn attack_roll_misses_when_below_ac() {
        let resolver = CombatResolver;
        let action = attack_action();
        let signal = attack_signal("skeleton_guard");
        let state = test_state("skeleton_guard", 10, 12, Some(5));

        // 5 + 5 = 10 < 12 -> miss
        let mut rng = StepRng::new(D20_MISS_SEED, 0);
        let result = resolver
            .resolve(&action, &signal, &state, &mut rng)
            .unwrap();

        assert_eq!(result.outcome, ResolutionOutcome::Failure);
        assert_eq!(result.details.get("roll"), Some(&MclValue::Int(5)));
        assert_eq!(result.details.get("total"), Some(&MclValue::Int(10)));
        assert!(result.details.get("damage").is_none());
    }

    #[test]
    fn entity_dies_at_zero_hp() {
        let resolver = CombatResolver;
        let action = attack_action();
        let signal = attack_signal("skeleton_guard");
        let state = test_state("skeleton_guard", 2, 12, Some(5));

        // 15 + 5 = 20 >= 12 -> hit, damage = 2, new_hp = max(2-2, 0) = 0
        let mut rng = StepRng::new(D20_HIT_SEED, INC_D6_2);
        let result = resolver
            .resolve(&action, &signal, &state, &mut rng)
            .unwrap();

        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert_eq!(result.details.get("damage"), Some(&MclValue::Int(2)));
        assert_eq!(
            result.details.get("target_hp_remaining"),
            Some(&MclValue::Int(0))
        );
        assert_eq!(result.details.get("killed"), Some(&MclValue::Bool(true)));
        let hp = result
            .details
            .get("target_hp_remaining")
            .and_then(|v| v.as_int())
            .unwrap();
        assert_eq!(hp, 0);
    }

    #[test]
    fn attack_without_target_returns_error() {
        let resolver = CombatResolver;
        let action = attack_action();
        let signal = MclSignal {
            kind: SignalKind::Action {
                name: "attack".to_string(),
                args: indexmap::IndexMap::new(),
            },
            span: test_span(),
            labels: LabelSet::default(),
        };
        let state = test_state("skeleton_guard", 10, 12, Some(5));
        let mut rng = StepRng::new(D20_HIT_SEED, 0);

        let result = resolver.resolve(&action, &signal, &state, &mut rng);
        assert!(result.is_err());
    }
}
