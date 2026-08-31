use mcl_llm::MockLlmBackend;
use mcl_runtime::Session;
use mcl_spec::value::MclValue;
use std::path::PathBuf;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crypt")
        .join(name)
}

/// Build a fresh session with the same mock responses, load context, run inputs.
/// Returns the final WorldState for comparison.
async fn run_session(inputs: &[&str], mock_responses: Vec<String>) -> mcl_runtime::WorldState {
    let llm: Box<dyn mcl_llm::LlmBackend> = Box::new(MockLlmBackend::new(mock_responses));
    let mut session = Session::new(llm);

    session
        .contexts
        .load_file(&example_path("crypt_antechamber.mcl"))
        .expect("load antechamber");
    session
        .contexts
        .activate("crypt_antechamber", &mut session.state)
        .expect("activate antechamber");

    for input in inputs {
        let _ = session.turn(input).await;
    }

    session.state
}

// Noop signals skip all resolvers, so the unseeded ThreadRng is never touched.
// This isolates the LLM as the only source of non-determinism, which MockLlmBackend eliminates.

#[tokio::test]
async fn deterministic_session_noop_path() {
    let noop_response = "You look around carefully.\nMCL::noop()";
    let inputs = ["look around"];

    let first_state = run_session(&inputs, vec![noop_response.to_string()]).await;

    assert!(
        first_state.changelog.is_empty(),
        "noop path should produce no changelog entries"
    );

    for i in 0..100 {
        let state = run_session(&inputs, vec![noop_response.to_string()]).await;
        assert_eq!(
            state.changelog.len(),
            first_state.changelog.len(),
            "iteration {i}: changelog length mismatch"
        );
        for (key, val) in &first_state.local {
            assert_eq!(
                state.get_by_str(key),
                Some(val),
                "iteration {i}: local state mismatch at '{key}'"
            );
        }
        for (entity_id, props) in &first_state.entities {
            let other = state.entities.get(entity_id).expect("entity missing");
            for (prop_key, prop_val) in props {
                assert_eq!(
                    other.get(prop_key),
                    Some(prop_val),
                    "iteration {i}: entity '{entity_id}' prop '{prop_key}' mismatch"
                );
            }
        }
    }
}

#[tokio::test]
async fn deterministic_session_mutate_path() {
    let mutate_response = "You flip the switch.\nMCL::mutate(lit, true)";
    let inputs = ["flip the switch"];

    let first_state = run_session(&inputs, vec![mutate_response.to_string()]).await;

    assert_eq!(
        first_state.get_by_str("lit"),
        Some(&MclValue::Bool(true)),
        "first run should have lit = true"
    );

    for i in 0..100 {
        let state = run_session(&inputs, vec![mutate_response.to_string()]).await;
        assert_eq!(
            state.get_by_str("lit"),
            Some(&MclValue::Bool(true)),
            "iteration {i}: lit should be true"
        );
        for (key, val) in &first_state.local {
            assert_eq!(
                state.get_by_str(key),
                Some(val),
                "iteration {i}: local state mismatch at '{key}'"
            );
        }
    }
}

#[tokio::test]
async fn deterministic_session_multi_turn_noop() {
    let responses = vec![
        "You peer into the darkness.\nMCL::noop()".to_string(),
        "Nothing changes.\nMCL::noop()".to_string(),
        "Still nothing.\nMCL::noop()".to_string(),
    ];
    let inputs = ["look", "wait", "look again"];

    let first_state = run_session(&inputs, responses.clone()).await;

    for i in 0..100 {
        let state = run_session(&inputs, responses.clone()).await;
        assert_eq!(
            state.changelog.len(),
            first_state.changelog.len(),
            "iteration {i}: changelog length mismatch"
        );
        for (key, val) in &first_state.local {
            assert_eq!(
                state.get_by_str(key),
                Some(val),
                "iteration {i}: local state mismatch at '{key}'"
            );
        }
    }
}

#[tokio::test]
async fn deterministic_session_combat_path() {
    let combat_response = "You strike!\nMCL::action(attack, target=skeleton_guard)";
    let inputs = ["attack the skeleton"];

    let first_state = run_session(&inputs, vec![combat_response.to_string()]).await;

    // The skeleton should have taken damage
    let first_hp = first_state.get_by_str("entities.skeleton_guard.hp");
    assert!(
        first_hp.is_some(),
        "skeleton_guard should have hp after combat"
    );

    for i in 0..100 {
        let state = run_session(&inputs, vec![combat_response.to_string()]).await;
        assert_eq!(
            state.get_by_str("entities.skeleton_guard.hp"),
            first_hp,
            "iteration {i}: combat HP mismatch"
        );
    }
}
