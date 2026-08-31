use mcl_llm::MockLlmBackend;
use mcl_runtime::Session;
use mcl_spec::value::MclValue;
use std::path::PathBuf;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crypt")
        .join(name)
}

fn make_session(responses: Vec<String>) -> Session {
    let llm: Box<dyn mcl_llm::LlmBackend> = Box::new(MockLlmBackend::new(responses));
    Session::new(llm)
}

fn load_both_contexts(session: &mut Session) {
    session
        .contexts
        .load_file(&example_path("crypt_antechamber.mcl"))
        .expect("load antechamber");
    session
        .contexts
        .load_file(&example_path("crypt_inner.mcl"))
        .expect("load inner");
    session
        .contexts
        .activate("crypt_antechamber", &mut session.state)
        .expect("activate antechamber");
}

#[tokio::test]
async fn transition_signal_with_no_condition_succeeds() {
    let response = [
        "You retreat back the way you came.",
        "MCL::transition(crypt_antechamber)",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_both_contexts(&mut session);
    session
        .contexts
        .activate("crypt_inner", &mut session.state)
        .expect("activate inner");

    let result = session.turn("go back").await.expect("turn should succeed");

    assert_eq!(
        result.transitioned_to.as_deref(),
        Some("crypt_antechamber"),
        "should transition to crypt_antechamber (no condition)"
    );
    assert_eq!(
        session.active_context(),
        Some("crypt_antechamber"),
        "active context should change"
    );
}

#[tokio::test]
async fn transition_signal_with_unmet_condition_is_blocked() {
    let response = [
        "You shove against the locked door.",
        "MCL::transition(crypt_inner)",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_both_contexts(&mut session);

    // door_north is "locked" by default, condition requires == "unlocked"
    let result = session
        .turn("force the door")
        .await
        .expect("turn should succeed");

    assert_eq!(
        result.transitioned_to, None,
        "transition should be blocked when condition not met"
    );
    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("condition not met") || w.contains("precondition")),
        "should warn about condition: {:?}",
        result.warnings
    );
    assert_eq!(
        session.active_context(),
        Some("crypt_antechamber"),
        "active context should not change"
    );
}

#[tokio::test]
async fn transition_signal_with_met_condition_succeeds() {
    let response = [
        "The unlocked door swings wide open.",
        "MCL::transition(crypt_inner)",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_both_contexts(&mut session);

    // Set door_north to "unlocked" to meet the precondition
    session
        .state
        .set("door_north", MclValue::String("unlocked".into()))
        .expect("set door_north");

    let result = session
        .turn("go through the door")
        .await
        .expect("turn should succeed");

    assert_eq!(
        result.transitioned_to.as_deref(),
        Some("crypt_inner"),
        "should transition when condition is met"
    );
    assert_eq!(session.active_context(), Some("crypt_inner"));
}

#[tokio::test]
async fn player_state_persists_across_transitions() {
    let response = "You step into the inner sanctum.\nMCL::transition(crypt_inner)";

    let mut session = make_session(vec![response.to_string()]);
    load_both_contexts(&mut session);

    session
        .state
        .set("door_north", MclValue::String("unlocked".into()))
        .expect("set door_north");
    session
        .state
        .set("player.hp", MclValue::Int(50))
        .expect("set player.hp");
    session
        .state
        .set("player.gold", MclValue::Int(10))
        .expect("set player.gold");

    session
        .turn("enter the inner chamber")
        .await
        .expect("turn should succeed");

    assert_eq!(
        session.state.get_by_str("player.hp"),
        Some(&MclValue::Int(50)),
        "player.hp should persist across transition"
    );
    assert_eq!(
        session.state.get_by_str("player.gold"),
        Some(&MclValue::Int(10)),
        "player.gold should persist across transition"
    );
}

#[tokio::test]
async fn local_state_resets_on_transition() {
    let response = "You move deeper into the crypt.\nMCL::transition(crypt_inner)";

    let mut session = make_session(vec![response.to_string()]);
    load_both_contexts(&mut session);

    session
        .state
        .set("door_north", MclValue::String("unlocked".into()))
        .expect("set door_north");
    // Set local state that only exists in antechamber
    session
        .state
        .set("lit", MclValue::Bool(true))
        .expect("set lit");
    session
        .state
        .set("door_north", MclValue::String("unlocked".into()))
        .expect("set door_north again");

    session.turn("go north").await.expect("turn should succeed");

    // "lit" is antechamber local state; after transition, local resets to inner's state
    assert_eq!(
        session.state.get_by_str("lit"),
        None,
        "antechamber local state 'lit' should be cleared after transition"
    );
    // crypt_inner defines "sarcophagus_open: false" and "trap_triggered: false"
    assert_eq!(
        session.state.get_by_str("sarcophagus_open"),
        Some(&MclValue::Bool(false)),
        "should have crypt_inner's local state after transition"
    );
}
