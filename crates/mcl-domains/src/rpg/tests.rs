use indexmap::IndexMap;
use mcl_runtime::resolver::ActionResolver;
use mcl_runtime::state::WorldState;
use mcl_spec::document::ActionDef;
use mcl_spec::label::LabelSet;
use mcl_spec::signal::{MclSignal, ResolutionOutcome, SignalKind};
use mcl_spec::span::Span;
use mcl_spec::value::MclValue;
use rand::rngs::mock::StepRng;

use super::combat::CombatResolver;
use super::inventory::InventoryResolver;
use super::register_rpg_resolvers;

fn make_action_signal(name: &str, target: &str) -> MclSignal {
    let mut args = IndexMap::new();
    args.insert("target".to_string(), MclValue::String(target.to_string()));
    MclSignal {
        kind: SignalKind::Action {
            name: name.to_string(),
            args,
        },
        span: Span::empty(),
        labels: LabelSet::default(),
    }
}

fn make_action_def(name: &str) -> ActionDef {
    ActionDef {
        name: name.to_string(),
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

#[test]
fn register_rpg_resolvers_registers_both_systems() {
    let mut registry = mcl_runtime::ResolverRegistry::new();
    register_rpg_resolvers(&mut registry);

    assert!(
        registry.has("combat_system"),
        "combat_system should be registered"
    );
    assert!(
        registry.has("inventory_system"),
        "inventory_system should be registered"
    );
    assert_eq!(
        registry.get("combat_system").unwrap().system_name(),
        "combat_system"
    );
    assert_eq!(
        registry.get("inventory_system").unwrap().system_name(),
        "inventory_system"
    );
}

#[test]
fn combat_resolver_hit_or_miss_returns_valid_resolution() {
    let resolver = CombatResolver;
    assert_eq!(resolver.system_name(), "combat_system");

    let mut state = WorldState::new();
    state.set("player.attack", MclValue::Int(100)).unwrap(); // guarantee hit with high attack
    state
        .set("entities.skeleton.hp", MclValue::Int(12))
        .unwrap();
    state.set("entities.skeleton.ac", MclValue::Int(5)).unwrap();

    let signal = make_action_signal("attack", "skeleton");
    let action = make_action_def("attack");

    let mut rng = StepRng::new(10, 1);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.action, "attack");
    assert_eq!(result.outcome, ResolutionOutcome::Success);
    assert!(result.details.contains_key("roll"));
    assert!(result.details.contains_key("damage"));
    assert!(result.details.contains_key("killed"));
    assert!(result.narration_hint.is_some());
}

#[test]
fn combat_resolver_miss_returns_failure() {
    let resolver = CombatResolver;

    let mut state = WorldState::new();
    state.set("player.attack", MclValue::Int(0)).unwrap(); // 0 attack bonus
    state.set("entities.golem.hp", MclValue::Int(50)).unwrap();
    state.set("entities.golem.ac", MclValue::Int(30)).unwrap(); // very high AC

    let signal = make_action_signal("attack", "golem");
    let action = make_action_def("attack");

    // StepRng returning 0 → roll will be 1 (gen_range 1..=20 min), total = 1 + 0 = 1 < 30
    let mut rng = StepRng::new(0, 0);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.outcome, ResolutionOutcome::Failure);
    assert!(result.narration_hint.as_ref().unwrap().contains("misses"));
}

#[test]
fn combat_resolver_requires_target() {
    let resolver = CombatResolver;
    let state = WorldState::new();

    // Signal without a target arg
    let signal = MclSignal {
        kind: SignalKind::Action {
            name: "attack".to_string(),
            args: IndexMap::new(),
        },
        span: Span::empty(),
        labels: LabelSet::default(),
    };
    let action = make_action_def("attack");
    let mut rng = StepRng::new(0, 0);

    let result = resolver.resolve(&action, &signal, &state, &mut rng);
    assert!(result.is_err(), "combat without target should error");
}

fn state_with_item(item_id: &str) -> WorldState {
    let mut state = WorldState::new();
    let mut items = IndexMap::new();
    items.insert(item_id.to_string(), MclValue::Map(IndexMap::new()));
    state.set("items", MclValue::Map(items)).unwrap();
    state
}

#[test]
fn inventory_resolver_pickup_adds_item() {
    let resolver = InventoryResolver;
    assert_eq!(resolver.system_name(), "inventory_system");

    let state = state_with_item("rusty_sword");
    let signal = make_action_signal("pickup", "rusty_sword");
    let action = make_action_def("pickup");
    let mut rng = StepRng::new(0, 0);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.action, "pickup");
    assert_eq!(result.outcome, ResolutionOutcome::Success);
    assert!(result
        .narration_hint
        .as_ref()
        .unwrap()
        .contains("rusty_sword"));

    let set_key = "set:player.inventory";
    assert!(result.details.contains_key(set_key));
    let list = result.details.get(set_key).unwrap().as_list().unwrap();
    assert_eq!(list.len(), 1);
}

#[test]
fn inventory_resolver_pickup_duplicate_fails() {
    let resolver = InventoryResolver;

    let mut state = state_with_item("rusty_sword");
    state
        .set(
            "player.inventory",
            MclValue::List(vec![MclValue::String("rusty_sword".to_string())]),
        )
        .unwrap();

    let signal = make_action_signal("pickup", "rusty_sword");
    let action = make_action_def("pickup");
    let mut rng = StepRng::new(0, 0);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.outcome, ResolutionOutcome::Failure);
    assert!(result
        .narration_hint
        .as_ref()
        .unwrap()
        .contains("already have"));
}

#[test]
fn inventory_resolver_drop_removes_item() {
    let resolver = InventoryResolver;

    let mut state = WorldState::new();
    state
        .set(
            "player.inventory",
            MclValue::List(vec![
                MclValue::String("rusty_sword".to_string()),
                MclValue::String("health_potion".to_string()),
            ]),
        )
        .unwrap();

    let signal = make_action_signal("drop", "rusty_sword");
    let action = make_action_def("drop");
    let mut rng = StepRng::new(0, 0);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.outcome, ResolutionOutcome::Success);
    let list = result
        .details
        .get("set:player.inventory")
        .unwrap()
        .as_list()
        .unwrap();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0], MclValue::String("health_potion".to_string()));
}

#[test]
fn inventory_resolver_unknown_action_fails() {
    let resolver = InventoryResolver;
    let state = WorldState::new();

    let signal = make_action_signal("inspect", "rusty_sword");
    let action = make_action_def("inspect");
    let mut rng = StepRng::new(0, 0);

    let result = resolver
        .resolve(&action, &signal, &state, &mut rng)
        .unwrap();

    assert_eq!(result.outcome, ResolutionOutcome::Failure);
    assert!(result
        .narration_hint
        .as_ref()
        .unwrap()
        .contains("Unknown inventory action"));
}

#[test]
fn inventory_resolver_requires_target() {
    let resolver = InventoryResolver;
    let state = WorldState::new();

    let signal = MclSignal {
        kind: SignalKind::Action {
            name: "pickup".to_string(),
            args: IndexMap::new(),
        },
        span: Span::empty(),
        labels: LabelSet::default(),
    };
    let action = make_action_def("pickup");
    let mut rng = StepRng::new(0, 0);

    let result = resolver.resolve(&action, &signal, &state, &mut rng);
    assert!(result.is_err(), "inventory without target should error");
}
