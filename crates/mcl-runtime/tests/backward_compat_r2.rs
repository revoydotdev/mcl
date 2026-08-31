//! R2 backward compatibility: effect inference + R1 fixture structural parity.
//!
//! Verifies:
//! 1. Actions without explicit `effects:` get correct inferred effects.
//! 2. All R1 fixtures parse identically — policies and effects are empty/default.

use std::path::PathBuf;

use mcl_runtime::infer_effects;
use mcl_spec::effects::Effect;

const R1_FIXTURE_DIRS: &[&str] = &["crypt", "debate", "multi_persona", "quiz"];

fn examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples")
}

fn fixture_mcl_files(dir_name: &str) -> Vec<PathBuf> {
    let dir = examples_dir().join(dir_name);
    assert!(dir.is_dir(), "fixture dir '{dir_name}' should exist");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("read dir {dir_name}: {e}"))
        .filter_map(|entry| {
            let entry = entry.expect("read dir entry");
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "mcl") {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    files.sort();
    files
}

fn parse_fixture(path: &PathBuf) -> mcl_spec::document::MclDocument {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("read {path:?}: {e}"));
    mcl_parse::parse_document(&src).unwrap_or_else(|e| panic!("parse {path:?}: {e}"))
}

#[test]
fn backward_compat_inferred_effects() {
    for dir_name in R1_FIXTURE_DIRS {
        for path in fixture_mcl_files(dir_name) {
            let doc = parse_fixture(&path);
            for (action_name, action) in &doc.actions {
                if !action.effects.is_empty() {
                    continue;
                }
                let inferred = infer_effects(action);
                if !action.mutates.is_empty() {
                    assert_eq!(
                        inferred,
                        vec![Effect::StateMutating],
                        "{path:?} action '{action_name}': has mutates → should infer StateMutating"
                    );
                } else if let Some(ref name) = action.resolution {
                    assert_eq!(
                        inferred,
                        vec![Effect::Dispatch(name.clone())],
                        "{path:?} action '{action_name}': has resolution → should infer Dispatch"
                    );
                } else {
                    assert_eq!(
                        inferred,
                        vec![Effect::Pure],
                        "{path:?} action '{action_name}': no mutates/resolution → should infer Pure"
                    );
                }
            }
        }
    }
}

#[test]
fn backward_compat_r1_fixtures_parse_identically() {
    for dir_name in R1_FIXTURE_DIRS {
        for path in fixture_mcl_files(dir_name) {
            let doc = parse_fixture(&path);

            assert!(
                doc.policies.is_empty(),
                "{path:?}: R1 fixture should have no policies, found {}",
                doc.policies.len()
            );
            assert!(
                doc.effects.is_empty(),
                "{path:?}: R1 fixture should have no document-level effects, found {}",
                doc.effects.len()
            );

            for (action_name, action) in &doc.actions {
                assert!(
                    action.effects.is_empty(),
                    "{path:?} action '{action_name}': R1 fixture actions should have no explicit effects"
                );
            }
        }
    }
}
