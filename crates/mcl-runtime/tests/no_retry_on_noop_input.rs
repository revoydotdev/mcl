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
async fn noop_input_does_not_trigger_retry() {
    // Provide exactly 1 response — if retry triggers, we'll get "no more mock responses" error
    let response = "You think about your past, memories of home flooding back.";

    let mut session = make_session(vec![response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("I think about my past")
        .await
        .expect("turn should succeed with single response (no retry)");

    assert!(
        result
            .narration
            .iter()
            .any(|l| l.contains("memories") || l.contains("past")),
        "should have narration"
    );
}

#[tokio::test]
async fn ambient_input_does_not_trigger_retry() {
    let response = "The cold air of the crypt settles around you.";

    let mut session = make_session(vec![response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("I wait and listen")
        .await
        .expect("turn should succeed with single response (no retry)");

    assert!(!result.narration.is_empty(), "should have narration");
}
