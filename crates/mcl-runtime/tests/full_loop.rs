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
async fn full_turn_loop() {
    let response = [
        "The torch sconce flickers to life, casting dancing shadows across the ancient stone walls.",
        "MCL::mutate(lit, true)",
        "",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_antechamber(&mut session);

    let result = session
        .turn("light the torch")
        .await
        .expect("turn should succeed");

    assert!(!result.narration.is_empty(), "should have narration");
    assert!(
        result
            .narration
            .iter()
            .any(|l| l.contains("torch") || l.contains("shadow")),
        "narration should mention torch/shadows"
    );
}

#[tokio::test]
async fn malformed_signal_handling() {
    let response = [
        "The skeleton rattles ominously.",
        "MCL::action(",
        "Something else happens.",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_antechamber(&mut session);

    let result = session
        .turn("approach the skeleton")
        .await
        .expect("turn should not panic");

    assert!(
        !result.warnings.is_empty(),
        "malformed signal should produce warnings"
    );
    assert!(
        !result.narration.is_empty(),
        "malformed signal line becomes narration"
    );
}

#[tokio::test]
async fn context_transition() {
    let response = [
        "The heavy iron door groans open, revealing a passage beyond.",
        "MCL::transition(crypt_inner)",
    ]
    .join("\n");

    let mut session = make_session(vec![response]);
    load_both_contexts(&mut session);

    session
        .state
        .set(
            "door_north",
            mcl_spec::value::MclValue::String("unlocked".into()),
        )
        .expect("set door_north");

    let result = session
        .turn("go through the door")
        .await
        .expect("turn should succeed");

    assert_eq!(
        result.transitioned_to.as_deref(),
        Some("crypt_inner"),
        "should transition to crypt_inner"
    );
    assert_eq!(
        session.active_context(),
        Some("crypt_inner"),
        "session should reflect new active context"
    );
}
