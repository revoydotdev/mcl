use mcl_runtime::prompt::build_system_prompt;
use mcl_runtime::WorldState;

fn antechamber() -> (mcl_spec::document::MclDocument, WorldState) {
    let src = include_str!("../../../examples/crypt/crypt_antechamber.mcl");
    let doc = mcl_parse::parse_document(src).unwrap();
    let state = WorldState::new();
    (doc, state)
}

#[test]
fn prompt_contains_examples_section() {
    let (doc, state) = antechamber();
    let prompt = build_system_prompt(&doc, &state);

    assert!(
        prompt.contains("## Examples"),
        "prompt should have an Examples section — got:\n{prompt}"
    );
}

#[test]
fn signal_examples_use_doc_actions() {
    let (doc, state) = antechamber();
    let prompt = build_system_prompt(&doc, &state);

    assert!(
        prompt.contains("MCL::action(look)"),
        "signal examples should reference the look action from the doc — got:\n{prompt}"
    );
}

#[test]
fn signal_examples_show_concrete_user_input_mapping() {
    let (doc, state) = antechamber();
    let prompt = build_system_prompt(&doc, &state);

    assert!(
        prompt.contains("MCL::action(attack, target="),
        "signal examples should show targeted action with target — got:\n{prompt}"
    );
}

#[test]
fn examples_appear_after_signal_protocol() {
    let (doc, state) = antechamber();
    let prompt = build_system_prompt(&doc, &state);

    let protocol_pos = prompt
        .find("## MCL Signal Protocol")
        .expect("should have Signal Protocol section");
    let examples_pos = prompt
        .find("## Examples")
        .expect("should have Examples section");
    let rules_pos = prompt.find("## Rules").expect("should have Rules section");

    assert!(
        protocol_pos < examples_pos,
        "Examples should come after Signal Protocol"
    );
    assert!(
        examples_pos < rules_pos,
        "Examples should come before Rules"
    );
}

#[test]
fn signal_examples_includes_dialogue_example() {
    let (doc, state) = antechamber();
    let prompt = build_system_prompt(&doc, &state);

    assert!(
        prompt.contains("MCL::dialogue(skeleton_guard,"),
        "signal examples should show dialogue with an entity from the doc — got:\n{prompt}"
    );
}
