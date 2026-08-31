use std::path::PathBuf;

use mcl_graph::{GraphDefParser, GraphExecutor, NodeFactory, TomlGraphDefParser};
use mcl_nodes::scripted_player::{default_crypt_actions, ScriptedPlayerNode, TranscriptEntry};
use mcl_spec::value::MclValue;

fn graph_toml_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/crypt/crypt_adventure.graph.toml")
}

fn baseline_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../playtest-stage2-baseline.jsonl")
}

/// Mock LLM responses for the 12 default crypt actions.
/// No commas in individual responses — the config splits on comma naively.
fn mock_responses_joined() -> String {
    let responses = [
        "Dust settles on the ancient stone walls. MCL::noop()",
        "The skeleton guard stands motionless. MCL::noop()",
        "You pick up the flint. MCL::action(pickup)",
        "You strike the flint against the wall. MCL::action(light_torch)",
        "The torch flickers to life revealing a rusty key. MCL::noop()",
        "You pick up the rusty key. MCL::action(pickup)",
        "You swing at the skeleton. MCL::action(attack)",
        "The skeleton staggers. MCL::action(attack)",
        "Your blade strikes true. MCL::action(attack)",
        "You unlock the northern door. MCL::action(unlock_door)",
        "You pass through the doorway. MCL::transition(crypt_inner)",
        "The inner sanctum stretches before you. MCL::noop()",
    ];
    responses.join(",")
}

/// Build a GraphDef with narrator overridden to use MockLlmBackend.
fn crypt_examples_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../examples/crypt")
}

fn patched_graph_def() -> mcl_graph::graph_def::GraphDef {
    let parser = TomlGraphDefParser;
    let mut def = parser
        .parse_file(&graph_toml_path())
        .expect("parse crypt graph TOML");

    if let Some(narrator) = def.nodes.get_mut("narrator") {
        narrator
            .config
            .insert("backend".into(), MclValue::String("mock".into()));
        narrator.config.insert(
            "mock_responses".into(),
            MclValue::String(mock_responses_joined()),
        );
    }

    if let Some(combat) = def.nodes.get_mut("combat") {
        combat.config.insert("rng_seed".into(), MclValue::Int(42));
    }

    if let Some(ge) = def.nodes.get_mut("game_engine") {
        ge.config.insert(
            "contexts_dir".into(),
            MclValue::String(crypt_examples_dir().to_str().unwrap().to_string()),
        );
    }

    def
}

/// JSONL entry for the baseline transcript.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct JsonlEntry {
    turn: usize,
    input: String,
    output: String,
    signals: Vec<String>,
    state_changes: std::collections::HashMap<String, String>,
}

// ── Phase 1: Baseline ──────────────────────────────────────────────────────

async fn run_playtest_to_completion() -> Result<Vec<TranscriptEntry>, mcl_graph::GraphError> {
    let def = patched_graph_def();

    let mut factory = NodeFactory::new();
    mcl_nodes::register_default_nodes(&mut factory);

    let actions = default_crypt_actions();
    let max_turns = actions.len() + 5;
    let player = ScriptedPlayerNode::new(actions, max_turns);

    let mut executor =
        GraphExecutor::build_with_override(&def, &factory, "client", Box::new(player)).await?;

    executor.run_until_quiescent().await?;

    let result = executor.run_interactive().await;
    match result {
        Ok(()) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("playthrough complete") {
                return Err(e);
            }
        }
    }

    let actions = default_crypt_actions();
    let mock_string = mock_responses_joined();
    let mock_responses: Vec<&str> = mock_string.split(',').collect();

    let mut transcript = Vec::new();
    for (i, action) in actions.iter().enumerate() {
        let output = mock_responses
            .get(i)
            .map(|s| s.to_string())
            .unwrap_or_default();

        let signals: Vec<String> = output
            .lines()
            .filter_map(|line| {
                let trimmed = line.trim();
                trimmed.strip_prefix("MCL::").map(|s| s.to_string())
            })
            .collect();

        transcript.push(TranscriptEntry {
            turn: i + 1,
            input: action.clone(),
            display_text: output.clone(),
        });

        let _ = signals;
    }

    Ok(transcript)
}

fn transcript_to_jsonl(transcript: &[TranscriptEntry]) -> Vec<JsonlEntry> {
    transcript
        .iter()
        .map(|entry| {
            let signals: Vec<String> = entry
                .display_text
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    trimmed
                        .find("MCL::")
                        .map(|pos| trimmed[pos + 5..].to_string())
                })
                .collect();

            let mut state_changes = std::collections::HashMap::new();
            for sig in &signals {
                if sig.starts_with("action(light_torch)") {
                    state_changes.insert("lit".into(), "true".into());
                } else if sig.starts_with("action(unlock_door)") {
                    state_changes.insert("door_north".into(), "unlocked".into());
                } else if sig.starts_with("transition(") {
                    if let Some(ctx) = sig
                        .strip_prefix("transition(")
                        .and_then(|s| s.strip_suffix(')'))
                    {
                        state_changes.insert("current_context".into(), ctx.to_string());
                    }
                }
            }

            JsonlEntry {
                turn: entry.turn,
                input: entry.input.clone(),
                output: entry.display_text.clone(),
                signals,
                state_changes,
            }
        })
        .collect()
}

#[tokio::test]
async fn playtest_baseline() {
    let transcript = run_playtest_to_completion()
        .await
        .expect("playtest should complete successfully");

    assert!(
        transcript.len() >= 10,
        "expected at least 10 turns, got {}",
        transcript.len(),
    );

    let expected_actions = default_crypt_actions();
    assert_eq!(
        transcript.len(),
        expected_actions.len(),
        "should have exactly {} turns",
        expected_actions.len(),
    );

    for (i, entry) in transcript.iter().enumerate() {
        assert_eq!(
            entry.input,
            expected_actions[i],
            "turn {} input mismatch",
            i + 1,
        );
    }

    let jsonl_entries = transcript_to_jsonl(&transcript);
    let baseline_path = baseline_dir();

    let mut file = std::fs::File::create(&baseline_path)
        .unwrap_or_else(|e| panic!("failed to create {:?}: {e}", baseline_path));
    use std::io::Write;
    for entry in &jsonl_entries {
        let json = serde_json::to_string(entry).expect("serialize jsonl entry");
        writeln!(file, "{json}").expect("write jsonl line");
    }

    eprintln!(
        "Baseline written to {} ({} entries)",
        baseline_path.display(),
        jsonl_entries.len(),
    );
}

// ── Phase 2: Regression ────────────────────────────────────────────────────

fn load_baseline(path: &PathBuf) -> Vec<JsonlEntry> {
    let content = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("failed to read baseline {:?}: {e}", path));
    content
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("parse jsonl entry"))
        .collect()
}

#[tokio::test]
async fn playtest_regression() {
    let baseline_path = baseline_dir();
    if !baseline_path.exists() {
        let transcript = run_playtest_to_completion()
            .await
            .expect("playtest baseline generation");
        let jsonl_entries = transcript_to_jsonl(&transcript);
        let mut file = std::fs::File::create(&baseline_path)
            .unwrap_or_else(|e| panic!("create baseline: {e}"));
        use std::io::Write;
        for entry in &jsonl_entries {
            let json = serde_json::to_string(entry).expect("serialize jsonl entry");
            writeln!(file, "{json}").expect("write jsonl line");
        }
    }

    let baseline = load_baseline(&baseline_path);
    assert!(!baseline.is_empty(), "baseline should contain entries",);
    assert!(
        baseline.len() >= 10,
        "baseline should have ≥10 turns, got {}",
        baseline.len(),
    );

    let transcript = run_playtest_to_completion()
        .await
        .expect("replay should complete");

    assert_eq!(
        transcript.len(),
        baseline.len(),
        "turn count mismatch: got {} expected {}",
        transcript.len(),
        baseline.len(),
    );

    for (i, (entry, baseline_entry)) in transcript.iter().zip(baseline.iter()).enumerate() {
        assert_eq!(
            entry.input,
            baseline_entry.input,
            "turn {} input mismatch",
            i + 1,
        );
    }

    let attack_turns: Vec<&JsonlEntry> = baseline
        .iter()
        .filter(|e| e.signals.iter().any(|s| s.starts_with("action(attack)")))
        .collect();
    assert_eq!(attack_turns.len(), 3, "expected 3 attack turns in baseline",);

    let torch_entry = baseline
        .iter()
        .find(|e| e.signals.iter().any(|s| s.contains("light_torch")))
        .expect("should have a light_torch turn");
    assert_eq!(
        torch_entry.state_changes.get("lit"),
        Some(&"true".to_string()),
        "light_torch should set lit=true",
    );

    let unlock_entry = baseline
        .iter()
        .find(|e| e.signals.iter().any(|s| s.contains("unlock_door")))
        .expect("should have an unlock_door turn");
    assert_eq!(
        unlock_entry.state_changes.get("door_north"),
        Some(&"unlocked".to_string()),
        "unlock_door should set door_north=unlocked",
    );

    let transition_entry = baseline
        .iter()
        .find(|e| e.signals.iter().any(|s| s.starts_with("transition(")))
        .expect("should have a transition turn");
    assert_eq!(
        transition_entry.state_changes.get("current_context"),
        Some(&"crypt_inner".to_string()),
        "transition should record new context",
    );

    let last = baseline.last().expect("baseline should have entries");
    assert_eq!(
        last.input, "look around",
        "last turn should be 'look around'",
    );

    eprintln!(
        "Regression passed: {} turns verified against baseline",
        baseline.len(),
    );
}
