use mcl_llm::MockLlmBackend;
use mcl_runtime::session::{Session, SessionConfig, SignalMode};
use std::path::PathBuf;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crypt")
        .join(name)
}

fn make_structured_session(responses: Vec<String>) -> Session {
    let llm: Box<dyn mcl_llm::LlmBackend> = Box::new(MockLlmBackend::new(responses));
    let config = SessionConfig {
        max_signal_retries: 0,
        signal_mode: SignalMode::Structured,
    };
    Session::with_config(llm, config)
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
async fn structured_output_mode() {
    let mock_response = r#"{
        "narration": "You scan the dim antechamber carefully.",
        "signals": [
            {"type": "action", "name": "look"}
        ]
    }"#;

    let mut session = make_structured_session(vec![mock_response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("look around")
        .await
        .expect("turn should succeed");

    assert!(
        !result.narration.is_empty(),
        "should have narration from the JSON response"
    );
    assert!(
        result.narration.iter().any(|l| l.contains("antechamber")),
        "narration should mention antechamber — got: {:?}",
        result.narration
    );
    assert!(
        !result.resolutions.is_empty(),
        "structured JSON action signal should produce a resolution — warnings: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn structured_mode_falls_back_to_narration_on_bad_json() {
    let mock_response = "The skeleton guards eye sockets glow with eerie light. Nothing happens.";

    let mut session = make_structured_session(vec![mock_response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("look around")
        .await
        .expect("turn should succeed");

    assert!(
        !result.narration.is_empty(),
        "bad JSON should be treated as narration"
    );
    assert!(
        result.narration.iter().any(|l| l.contains("skeleton")),
        "narration should contain the response text — got: {:?}",
        result.narration
    );
    assert!(
        result.resolutions.is_empty(),
        "bad JSON should not produce any resolutions"
    );
}

#[tokio::test]
async fn structured_mode_dialogue_signal() {
    let mock_response = r#"{
        "narration": "The guard turns to face you.",
        "signals": [
            {"type": "dialogue", "entity": "skeleton_guard", "text": "Who dares enter?"}
        ]
    }"#;

    let mut session = make_structured_session(vec![mock_response.to_string()]);
    load_antechamber(&mut session);

    let result = session
        .turn("talk to the guard")
        .await
        .expect("turn should succeed");

    assert!(!result.narration.is_empty(), "should have narration");
    assert!(
        result.warnings.is_empty(),
        "dialogue signal should be valid — warnings: {:?}",
        result.warnings
    );
}

#[tokio::test]
async fn structured_prompt_requests_json() {
    use mcl_runtime::prompt::build_structured_system_prompt;

    let src = include_str!("../../../examples/crypt/crypt_antechamber.mcl");
    let doc = mcl_parse::parse_document(src).unwrap();
    let state = mcl_runtime::WorldState::new();

    let prompt = build_structured_system_prompt(&doc, &state);

    assert!(
        prompt.contains("\"narration\""),
        "structured prompt should mention JSON narration field — got:\n{prompt}"
    );
    assert!(
        prompt.contains("\"signals\""),
        "structured prompt should mention JSON signals field — got:\n{prompt}"
    );
    assert!(
        prompt.contains("\"type\""),
        "structured prompt should mention JSON type field — got:\n{prompt}"
    );
    assert!(
        !prompt.contains("## Signal Format"),
        "structured prompt should NOT have inline Signal Format heading — got:\n{prompt}"
    );
    assert!(
        prompt.contains("JSON"),
        "structured prompt should mention JSON — got:\n{prompt}"
    );
}
