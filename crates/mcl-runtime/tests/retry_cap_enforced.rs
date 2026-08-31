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
async fn retry_is_capped_at_one() {
    let first = "You ready your weapon against the skeleton.";
    let second = "The skeleton watches you, its empty sockets gleaming faintly.";

    let mut session = make_session(vec![first.to_string(), second.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("attack the skeleton guard")
        .await
        .expect("turn should not error — retry should be capped, not attempt a 3rd call");

    assert!(
        !result.narration.is_empty(),
        "should have narration even when no signal is produced"
    );
}
