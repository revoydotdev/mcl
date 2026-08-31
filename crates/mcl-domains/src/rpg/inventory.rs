use indexmap::IndexMap;
use mcl_runtime::resolver::ActionResolver;
use mcl_runtime::state::WorldState;
use mcl_spec::document::ActionDef;
use mcl_spec::signal::{MclSignal, Resolution, ResolutionOutcome, SignalKind};
use mcl_spec::value::MclValue;
use mcl_spec::MclError;
use rand::RngCore;

/// Handles picking up, using, and dropping items
pub struct InventoryResolver;

impl ActionResolver for InventoryResolver {
    fn system_name(&self) -> &str {
        "inventory_system"
    }

    fn resolve(
        &self,
        action: &ActionDef,
        signal: &MclSignal,
        state: &WorldState,
        _rng: &mut dyn RngCore,
    ) -> Result<Resolution, MclError> {
        let item_id = extract_target(signal).ok_or_else(|| {
            MclError::Resolver("inventory action requires a target item".to_string())
        })?;

        let mut details = IndexMap::new();
        details.insert("item".to_string(), MclValue::String(item_id.clone()));

        // Get current inventory
        let inventory = state
            .get_by_str("player.inventory")
            .and_then(|v| v.as_list())
            .map(|l| l.to_vec())
            .unwrap_or_default();

        match action.name.as_str() {
            "pickup" | "take" | "grab" => {
                // Check item exists in world
                let item_key = format!("items.{item_id}");
                if state.get_by_str(&item_key).is_none() {
                    return Ok(Resolution {
                        action: action.name.clone(),
                        outcome: ResolutionOutcome::Failure,
                        details,
                        narration_hint: Some(format!("There is no {item_id} here.")),
                    });
                }

                // Check visibility condition
                let vis_key = format!("items.{item_id}.visible_when");
                if let Some(vis_condition_str) = state.get_by_str(&vis_key).and_then(|v| v.as_str())
                {
                    let condition = mcl_spec::expression::Condition::Truthy(
                        mcl_spec::expression::FieldPath::new(vec![vis_condition_str.to_string()]),
                    );
                    if !state.evaluate(&condition) {
                        return Ok(Resolution {
                            action: action.name.clone(),
                            outcome: ResolutionOutcome::Failure,
                            details,
                            narration_hint: Some(format!("You don't see a {item_id} here.")),
                        });
                    }
                }

                let already_has = inventory.iter().any(|v| v.as_str() == Some(&item_id));

                if already_has {
                    return Ok(Resolution {
                        action: action.name.clone(),
                        outcome: ResolutionOutcome::Failure,
                        details,
                        narration_hint: Some(format!("You already have the {item_id}.")),
                    });
                }

                let mut new_inv = inventory;
                new_inv.push(MclValue::String(item_id.clone()));
                details.insert("set:player.inventory".to_string(), MclValue::List(new_inv));

                Ok(Resolution {
                    action: action.name.clone(),
                    outcome: ResolutionOutcome::Success,
                    details,
                    narration_hint: Some(format!("You pick up the {item_id}.")),
                })
            }
            "drop" | "discard" => {
                let new_inv: Vec<MclValue> = inventory
                    .into_iter()
                    .filter(|v| v.as_str() != Some(&item_id))
                    .collect();

                details.insert("set:player.inventory".to_string(), MclValue::List(new_inv));

                Ok(Resolution {
                    action: action.name.clone(),
                    outcome: ResolutionOutcome::Success,
                    details,
                    narration_hint: Some(format!("You drop the {item_id}.")),
                })
            }
            "use" => {
                let has_item = inventory.iter().any(|v| v.as_str() == Some(&item_id));

                if !has_item {
                    return Ok(Resolution {
                        action: action.name.clone(),
                        outcome: ResolutionOutcome::Failure,
                        details,
                        narration_hint: Some(format!("You don't have the {item_id}.")),
                    });
                }

                let consumable_key = format!("items.{item_id}.consumable");
                let is_consumable = state
                    .get_by_str(&consumable_key)
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);

                if is_consumable {
                    let new_inv: Vec<MclValue> = inventory
                        .into_iter()
                        .filter(|v| v.as_str() != Some(&item_id))
                        .collect();
                    details.insert("set:player.inventory".to_string(), MclValue::List(new_inv));
                }

                Ok(Resolution {
                    action: action.name.clone(),
                    outcome: ResolutionOutcome::Success,
                    details,
                    narration_hint: Some(format!("You use the {item_id}.")),
                })
            }
            "examine" => {
                let desc_key = format!("items.{item_id}.description");
                let narration = state
                    .get_by_str(&desc_key)
                    .and_then(|v| v.as_str().map(|s| s.to_string()))
                    .unwrap_or_else(|| format!("You see a {item_id}."));

                Ok(Resolution {
                    action: action.name.clone(),
                    outcome: ResolutionOutcome::Success,
                    details,
                    narration_hint: Some(narration),
                })
            }
            _ => Ok(Resolution {
                action: action.name.clone(),
                outcome: ResolutionOutcome::Failure,
                details,
                narration_hint: Some("Unknown inventory action.".to_string()),
            }),
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
    use indexmap::IndexMap;
    use mcl_spec::document::ActionDef;
    use mcl_spec::label::LabelSet;
    use mcl_spec::span::Span;
    use rand::rngs::mock::StepRng;

    fn make_signal(action: &str, target: &str) -> MclSignal {
        let mut args = IndexMap::new();
        args.insert("target".into(), MclValue::String(target.into()));
        MclSignal {
            kind: SignalKind::Action {
                name: action.into(),
                args,
            },
            span: Span::empty(),
            labels: LabelSet::default(),
        }
    }

    fn make_action(name: &str) -> ActionDef {
        ActionDef {
            name: name.into(),
            requires: vec![],
            requires_any: vec![],
            target: None,
            mutates: IndexMap::new(),
            resolution: None,
            mode: None,
            args: IndexMap::new(),
            cooldown: None,
            description: None,
            trigger_phrases: Vec::new(),
            effects: Vec::new(),
            span: Span::empty(),
        }
    }

    fn resolve(
        resolver: &InventoryResolver,
        action: &str,
        target: &str,
        state: &WorldState,
    ) -> Resolution {
        let signal = make_signal(action, target);
        let action_def = make_action(action);
        let mut rng = StepRng::new(0, 0);
        resolver
            .resolve(&action_def, &signal, state, &mut rng)
            .unwrap()
    }

    fn state_with_item(item_id: &str) -> WorldState {
        let mut state = WorldState::new();
        let mut items = IndexMap::new();
        items.insert(item_id.to_string(), MclValue::Map(IndexMap::new()));
        state.set("items", MclValue::Map(items)).unwrap();
        state
    }

    #[test]
    fn pickup_item_adds_to_player_inventory() {
        let state = state_with_item("rusty_key");
        let result = resolve(&InventoryResolver, "pickup", "rusty_key", &state);

        assert_eq!(result.outcome, ResolutionOutcome::Success);
        let list = result
            .details
            .get("set:player.inventory")
            .unwrap()
            .as_list()
            .unwrap();
        assert!(list.iter().any(|v| v.as_str() == Some("rusty_key")));
    }

    #[test]
    fn pickup_nonexistent_item_fails() {
        let state = WorldState::new();
        let result = resolve(&InventoryResolver, "pickup", "unicorn_sword", &state);

        assert_eq!(result.outcome, ResolutionOutcome::Failure);
        assert_eq!(
            result.narration_hint.as_deref(),
            Some("There is no unicorn_sword here.")
        );
    }

    #[test]
    fn pickup_invisible_item_fails() {
        let mut state = WorldState::new();
        let mut item_props = IndexMap::new();
        item_props.insert(
            "visible_when".to_string(),
            MclValue::String("lit".to_string()),
        );
        let mut items = IndexMap::new();
        items.insert("rusty_key".to_string(), MclValue::Map(item_props));
        state.set("items", MclValue::Map(items)).unwrap();
        state.set("lit", MclValue::Bool(false)).unwrap();

        let result = resolve(&InventoryResolver, "pickup", "rusty_key", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Failure);
        assert_eq!(
            result.narration_hint.as_deref(),
            Some("You don't see a rusty_key here.")
        );
    }

    #[test]
    fn pickup_visible_item_succeeds() {
        let mut state = WorldState::new();
        let mut item_props = IndexMap::new();
        item_props.insert(
            "visible_when".to_string(),
            MclValue::String("lit".to_string()),
        );
        let mut items = IndexMap::new();
        items.insert("rusty_key".to_string(), MclValue::Map(item_props));
        state.set("items", MclValue::Map(items)).unwrap();
        state.set("lit", MclValue::Bool(true)).unwrap();

        let result = resolve(&InventoryResolver, "pickup", "rusty_key", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert_eq!(
            result.narration_hint.as_deref(),
            Some("You pick up the rusty_key.")
        );
    }

    #[test]
    fn use_consumable_removes_from_inventory() {
        let mut state = WorldState::new();
        state
            .set(
                "player.inventory",
                MclValue::List(vec![MclValue::String("health_potion".into())]),
            )
            .unwrap();
        let mut item_props = IndexMap::new();
        item_props.insert("consumable".into(), MclValue::Bool(true));
        let mut items = IndexMap::new();
        items.insert("health_potion".into(), MclValue::Map(item_props));
        state.set("items", MclValue::Map(items)).unwrap();

        let result = resolve(&InventoryResolver, "use", "health_potion", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        let list = result
            .details
            .get("set:player.inventory")
            .unwrap()
            .as_list()
            .unwrap();
        assert!(list.is_empty());
    }

    #[test]
    fn use_non_consumable_keeps_in_inventory() {
        let mut state = WorldState::new();
        state
            .set(
                "player.inventory",
                MclValue::List(vec![MclValue::String("rusty_key".into())]),
            )
            .unwrap();
        let mut item_props = IndexMap::new();
        item_props.insert("consumable".into(), MclValue::Bool(false));
        let mut items = IndexMap::new();
        items.insert("rusty_key".into(), MclValue::Map(item_props));
        state.set("items", MclValue::Map(items)).unwrap();

        let result = resolve(&InventoryResolver, "use", "rusty_key", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert!(
            !result.details.contains_key("set:player.inventory"),
            "non-consumable should not modify inventory"
        );
    }

    #[test]
    fn drop_item_removes_from_inventory() {
        let mut state = WorldState::new();
        state
            .set(
                "player.inventory",
                MclValue::List(vec![
                    MclValue::String("rusty_key".into()),
                    MclValue::String("torch".into()),
                ]),
            )
            .unwrap();

        let result = resolve(&InventoryResolver, "drop", "rusty_key", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        let list = result
            .details
            .get("set:player.inventory")
            .unwrap()
            .as_list()
            .unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].as_str(), Some("torch"));
    }

    #[test]
    fn examine_item_returns_description() {
        let mut state = WorldState::new();
        let mut item_props = IndexMap::new();
        item_props.insert(
            "description".into(),
            MclValue::String("An old rusty key with a worn handle.".into()),
        );
        let mut items = IndexMap::new();
        items.insert("rusty_key".into(), MclValue::Map(item_props));
        state.set("items", MclValue::Map(items)).unwrap();

        let result = resolve(&InventoryResolver, "examine", "rusty_key", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert_eq!(
            result.narration_hint.as_deref(),
            Some("An old rusty key with a worn handle.")
        );
    }

    #[test]
    fn examine_item_without_description_returns_generic() {
        let state = WorldState::new();
        let result = resolve(&InventoryResolver, "examine", "strange_rock", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Success);
        assert_eq!(
            result.narration_hint.as_deref(),
            Some("You see a strange_rock.")
        );
    }

    #[test]
    fn use_item_not_in_inventory_fails() {
        let state = WorldState::new();
        let result = resolve(&InventoryResolver, "use", "health_potion", &state);
        assert_eq!(result.outcome, ResolutionOutcome::Failure);
    }
}
