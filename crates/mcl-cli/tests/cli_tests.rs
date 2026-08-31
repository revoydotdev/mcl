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
fn validate_parses_single_mcl_file() {
    let path = workspace_root().join("examples/crypt/crypt_antechamber.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    assert_eq!(doc.frontmatter.context, "crypt_antechamber");
    assert!(!doc.actions.is_empty());
}

#[test]
fn validate_parses_all_crypt_files() {
    let dir = workspace_root().join("examples/crypt");
    let mut count = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let entry = entry.unwrap();
        if entry.path().extension().is_some_and(|e| e == "mcl") {
            let doc = mcl_parse::parse_file(&entry.path());
            assert!(doc.is_ok(), "failed to parse {:?}", entry.path());
            count += 1;
        }
    }
    assert!(count >= 3, "expected at least 3 .mcl files, found {count}");
}

#[test]
fn inspect_serializes_to_json() {
    let path = workspace_root().join("examples/crypt/crypt_antechamber.mcl");
    let doc = mcl_parse::parse_file(&path).unwrap();
    let json = serde_json::to_string_pretty(&doc).unwrap();
    assert!(json.contains("crypt_antechamber"));
    assert!(json.contains("actions"));
}

#[test]
fn dot_exports_graph_toml() {
    let path = workspace_root().join("examples/crypt/crypt_adventure.graph.toml");
    use mcl_graph::{export_dot, GraphDefParser, TomlGraphDefParser};
    let parser = TomlGraphDefParser;
    let def = parser.parse_file(&path).unwrap();
    let dot = export_dot(&def);
    assert!(dot.contains("digraph"));
    assert!(dot.contains("crypt_adventure"));
}
