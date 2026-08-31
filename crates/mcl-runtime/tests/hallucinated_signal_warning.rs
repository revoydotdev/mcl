use mcl_llm::MockLlmBackend;
use mcl_runtime::Session;
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

fn load_antechamber(session: &mut Session) {
    session
        .contexts
        .load_file(&example_path("crypt_antechamber.mcl"))
        .expect("load antechamber");
    session
        .contexts
        .activate("crypt_antechamber", &mut session.state)
        .expect("activate antechamber");
}

#[tokio::test]
async fn unknown_action_produces_warning() {
    let response = "You attempt to fly.\nMCL::action(fly)";

    let mut session = make_session(vec![response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("I try to fly away")
        .await
        .expect("turn should succeed");

    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("unknown action") && w.contains("fly")),
        "hallucinated action 'fly' should produce a warning — got: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn dialogue_with_nonexistent_entity_produces_warning() {
    let response =
        "You call out to the darkness.\nMCL::dialogue(ghost_king, \"I demand you show yourself!\")";

    let mut session = make_session(vec![response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("I call out to the ghost king")
        .await
        .expect("turn should succeed");

    assert!(
        result
            .warnings
            .iter()
            .any(|w| w.contains("unknown entity") && w.contains("ghost_king")),
        "dialogue with nonexistent entity 'ghost_king' should produce a warning — got: {:?}",
        result.warnings
    );
}
