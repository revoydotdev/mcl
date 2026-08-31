use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    PathBuf::from(manifest_dir)
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

#[test]
fn debate_opening_round_parses() {
    let path = workspace_root().join("examples/debate/opening_round.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "opening_round");
    assert!(!doc.actions.is_empty());
    assert!(!doc.entities.is_empty());
}

#[test]
fn debate_closing_parses() {
    let path = workspace_root().join("examples/debate/closing.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "closing");
}

#[test]
fn quiz_question_1_parses() {
    let path = workspace_root().join("examples/quiz/question_1.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "question_1");
    assert!(doc.actions.len() >= 4); // answer_a, answer_b, answer_c, ask_hint, skip
}

#[test]
fn quiz_question_2_parses() {
    let path = workspace_root().join("examples/quiz/question_2.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "question_2");
}

#[test]
fn debate_graph_toml_parses() {
    let path = workspace_root().join("examples/debate/debate.graph.toml");
    use mcl_graph::{GraphDefParser, TomlGraphDefParser};
    let parser = TomlGraphDefParser;
    let def = parser.parse_file(&path).unwrap();
    assert_eq!(def.name, "ai_debate");
    assert!(def.nodes.len() >= 3);
}

#[test]
fn multi_persona_room_parses() {
    let path = workspace_root().join("examples/multi_persona/room.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "haunted_library");
    assert!(!doc.actions.is_empty());
}

#[test]
fn multi_persona_graph_parses() {
    let path = workspace_root().join("examples/multi_persona/gothic_advisor.graph.toml");
    use mcl_graph::{GraphDefParser, TomlGraphDefParser};
    let parser = TomlGraphDefParser;
    let def = parser.parse_file(&path).unwrap();
    assert_eq!(def.name, "multi_persona_demo");
    assert!(def.nodes.contains_key("narrator"));
    assert!(def.nodes.contains_key("advisor"));
    assert!(def.nodes.len() >= 5);
    // Narrator has lower priority (runs first)
    assert!(def.nodes["narrator"].priority < def.nodes["advisor"].priority);
}

#[test]
fn all_example_domains_validate() {
    for dir in &[
        "examples/crypt",
        "examples/debate",
        "examples/quiz",
        "examples/multi_persona",
    ] {
        let dir_path = workspace_root().join(dir);
        for entry in std::fs::read_dir(&dir_path).unwrap() {
            let entry = entry.unwrap();
            if entry.path().extension().is_some_and(|e| e == "mcl") {
                let doc = mcl_parse::parse_file(&entry.path());
                assert!(
                    doc.is_ok(),
                    "failed to parse {:?}: {:?}",
                    entry.path(),
                    doc.err()
                );
            }
        }
    }
}
