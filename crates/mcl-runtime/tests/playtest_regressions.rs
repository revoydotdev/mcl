//! Playtest regression tests.
//!
//! Each test reproduces a bug found during real LLM playtesting by feeding
//! the exact (or representative) LLM output through MockLlmBackend.
//!
//! Add a new test for every playtest bug:
//! 1. Record the LLM response that triggered the failure
//! 2. Write a test here using MockLlmBackend with that response
//! 3. Fix the bug
//! 4. Verify the test passes

use mcl_llm::MockLlmBackend;
use mcl_runtime::Session;
use std::path::PathBuf;

fn example_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crypt")
        .join(name)
}

async fn session_with_responses(responses: Vec<&str>) -> Session {
    let llm: Box<dyn mcl_llm::LlmBackend> = Box::new(MockLlmBackend::new(
        responses.into_iter().map(String::from).collect(),
    ));
    let mut session = Session::new(llm);
    session
        .contexts
        .load_file(&example_path("crypt_antechamber.mcl"))
        .expect("load antechamber");
    session
        .contexts
        .activate("crypt_antechamber", &mut session.state)
        .expect("activate");
    session
}

/// Regression: LLM emits no signal at all — just plain narration.
/// The runtime should handle this gracefully (treat as implicit noop),
/// not crash or hang.
#[tokio::test]
async fn no_signal_emitted_is_handled_gracefully() {
    let mut session = session_with_responses(vec![
        "The cold stone walls drip with ancient moisture. You see nothing of interest.",
    ])
    .await;

    let result = session.turn("look around").await;
    assert!(
        result.is_ok(),
        "missing signal should not crash: {result:?}"
    );
    let turn = result.unwrap();
    assert!(
        !turn.narration.is_empty(),
        "narration should still be returned"
    );
}

/// Regression: LLM emits a malformed signal like `MCL::action(` (unclosed).
/// The runtime should warn but not panic.
#[tokio::test]
async fn malformed_signal_does_not_crash() {
    let mut session = session_with_responses(vec!["The skeleton lunges!\nMCL::action("]).await;

    let result = session.turn("attack skeleton").await;
    assert!(
        result.is_ok(),
        "malformed signal should not crash: {result:?}"
    );
}

/// Regression: LLM emits signal with unknown action name.
/// Should produce a warning, not a panic.
#[tokio::test]
async fn unknown_action_name_handled() {
    let mut session = session_with_responses(vec![
        "You try something unusual.\nMCL::action(teleport, target=skeleton_guard)",
    ])
    .await;

    let result = session.turn("teleport behind the skeleton").await;
    assert!(
        result.is_ok(),
        "unknown action should not crash: {result:?}"
    );
}

/// Regression: LLM emits valid signal with extra whitespace/formatting.
/// Common with real LLMs that add markdown formatting around signals.
#[tokio::test]
async fn signal_with_extra_whitespace() {
    let mut session =
        session_with_responses(vec!["You peer into the shadows.\n\n  MCL::noop()  \n"]).await;

    let result = session.turn("look around").await;
    assert!(
        result.is_ok(),
        "whitespace-padded signal should parse: {result:?}"
    );
}

/// Regression: LLM wraps signal in markdown code block.
/// Real LLMs sometimes do: ```MCL::action(look)```
#[tokio::test]
async fn signal_in_markdown_code_block() {
    let mut session = session_with_responses(vec![
        "You look around carefully.\n```\nMCL::action(look)\n```",
    ])
    .await;

    let result = session.turn("look around").await;
    assert!(
        result.is_ok(),
        "code-block signal should not crash: {result:?}"
    );
}

/// Regression: LLM emits multiple signals in one response.
/// Should handle all of them, not just the first.
#[tokio::test]
async fn multiple_signals_in_one_response() {
    let mut session = session_with_responses(vec![
        "The skeleton attacks and speaks!\nMCL::action(attack, target=skeleton_guard)\nMCL::dialogue(skeleton_guard, \"You shall not pass!\")",
    ])
    .await;

    let result = session.turn("provoke the skeleton").await;
    assert!(
        result.is_ok(),
        "multiple signals should not crash: {result:?}"
    );
}

/// Regression: LLM emits transition to non-existent context.
/// Should warn, not crash.
#[tokio::test]
async fn transition_to_nonexistent_context() {
    let mut session =
        session_with_responses(vec!["A portal opens!\nMCL::transition(shadow_realm)"]).await;

    let result = session.turn("enter the portal").await;
    assert!(
        result.is_ok(),
        "transition to unknown context should not crash: {result:?}"
    );
}

/// Regression: Completely empty LLM response.
#[tokio::test]
async fn empty_llm_response() {
    let mut session = session_with_responses(vec![""]).await;

    let result = session.turn("do something").await;
    assert!(
        result.is_ok(),
        "empty response should not crash: {result:?}"
    );
}

/// Regression: LLM response is just whitespace.
#[tokio::test]
async fn whitespace_only_response() {
    let mut session = session_with_responses(vec!["   \n\n   "]).await;

    let result = session.turn("do something").await;
    assert!(
        result.is_ok(),
        "whitespace response should not crash: {result:?}"
    );
}

/// Regression: Very long LLM response (> 10KB) with signal buried at end.
#[tokio::test]
async fn very_long_response_with_signal() {
    let padding = "The ancient crypt stretches endlessly before you. ".repeat(200);
    let response = format!("{padding}\nMCL::noop()");
    let mut session = session_with_responses(vec![&response]).await;

    let result = session.turn("look around").await;
    assert!(result.is_ok(), "long response should not crash: {result:?}");
}
