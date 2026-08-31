//! Integration tests for CLI subcommands using `assert_cmd`.
//!
//! These tests invoke the binary directly and verify command-line behavior,
//! including --help output, successful invocations, and error handling.

use assert_cmd::Command;
use predicates::prelude::*;
use std::path::PathBuf;

/// Helper function to get the workspace root directory.
fn workspace_root() -> PathBuf {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    PathBuf::from(manifest_dir)
        .parent()
        .expect("missing parent directory")
        .parent()
        .expect("missing grandparent directory (workspace root)")
        .to_path_buf()
}

#[test]
fn help_shows_version_and_subcommands() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Model Control Language runtime"))
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("Commands:"))
        .stdout(predicate::str::contains("play"))
        .stdout(predicate::str::contains("validate"))
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("graph"))
        .stdout(predicate::str::contains("dot"))
        .stdout(predicate::str::contains("bench"))
        .stdout(predicate::str::contains("playtest"))
        .stdout(predicate::str::contains("serve-mcp"))
        .stdout(predicate::str::contains("eval"))
        .stdout(predicate::str::contains("registry"));
}

#[test]
fn play_help_shows_play_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["play", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("play"))
        .stdout(predicate::str::contains("Run an interactive MCL session"))
        .stdout(predicate::str::contains("<PATH>"))
        .stdout(predicate::str::contains("--backend"))
        .stdout(predicate::str::contains("--model"))
        .stdout(predicate::str::contains("--api-key"))
        .stdout(predicate::str::contains("--base-url"))
        .stdout(predicate::str::contains("--start"))
        .stdout(predicate::str::contains("--save"))
        .stdout(predicate::str::contains("--load"))
        .stdout(predicate::str::contains("--audit-log"));
}

#[test]
fn validate_examples_crypt_succeeds() {
    let crypt_dir = workspace_root().join("examples/crypt");
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["validate", crypt_dir.to_str().expect("invalid path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok:"))
        .stdout(predicate::str::contains("document(s) validated"));
}

#[test]
fn inspect_help_shows_inspect_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["inspect", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("Show parsed document structure"))
        .stdout(predicate::str::contains("<PATH>"));
}

#[test]
fn dot_help_shows_dot_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["dot", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("dot"))
        .stdout(predicate::str::contains(
            "Export graph as DOT (Graphviz) format",
        ))
        .stdout(predicate::str::contains("<FILE>"));
}

#[test]
fn graph_help_shows_graph_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["graph", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("graph"))
        .stdout(predicate::str::contains(
            "Run a node-graph MCL session from a .graph.toml definition",
        ))
        .stdout(predicate::str::contains("<PATH>"));
}

#[test]
fn no_arguments_shows_error_and_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .assert()
        .failure()
        .stderr(predicate::str::contains("Model Control Language runtime"))
        .stderr(predicate::str::contains("Usage:"))
        .stderr(predicate::str::contains("<COMMAND>"));
}

#[test]
fn validate_nonexistent_mcl_fails() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["validate", "nonexistent.mcl"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn play_without_path_shows_error() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["play"])
        .assert()
        .failure()
        .stderr(predicate::str::contains(
            "error: the following required arguments were not provided",
        ))
        .stderr(predicate::str::contains("<PATH>"));
}

#[test]
fn inspect_nonexistent_mcl_fails() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["inspect", "nonexistent.mcl"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn dot_nonexistent_graph_toml_fails() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["dot", "nonexistent.graph.toml"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn registry_help_shows_registry_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("registry"))
        .stdout(predicate::str::contains("Inspect signal registry"))
        .stdout(predicate::str::contains("<COMMAND>"));
}

#[test]
fn registry_inspect_help_shows_inspect_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "inspect", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("inspect"))
        .stdout(predicate::str::contains("Inspect signal log entries"))
        .stdout(predicate::str::contains("<CAMPAIGN>"))
        .stdout(predicate::str::contains("--last"));
}

#[test]
fn registry_inspect_shows_entries() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]},"signal":{"kind":{"Action":{"name":"defend","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":2,"channel":"actions","timestamp":1714000001000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,2]},"signal":{"kind":{"Query":{"path":"look"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":3,"channel":"queries","timestamp":1714000002000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args(["registry", "inspect", "test-campaign", "--last", "2"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Last 2 entries"))
        .stdout(predicate::str::contains(
            "Turn 2: player from actions on action: defend",
        ))
        .stdout(predicate::str::contains(
            "Turn 3: player from queries on query: look",
        ));
}

#[test]
fn registry_query_shows_last_entries() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]},"signal":{"kind":{"Action":{"name":"defend","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":2,"channel":"actions","timestamp":1714000001000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,2]},"signal":{"kind":{"Query":{"path":"look"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":3,"channel":"queries","timestamp":1714000002000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args([
            "registry",
            "query",
            "--campaign",
            "test-campaign",
            "--last",
            "2",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("Last 2 entries"))
        .stdout(predicate::str::contains(
            "Turn 2: player from actions on action: defend",
        ))
        .stdout(predicate::str::contains(
            "Turn 3: player from queries on query: look",
        ));
}

#[test]
fn registry_query_semantic_returns_results_or_no_match() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    // Semantic query with fresh index (no vectors ingested) returns no matches
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args([
            "registry",
            "query",
            "--semantic",
            "attack",
            "--campaign",
            "test-campaign",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No matching entries found"));
}

#[test]
fn registry_query_ancestors_returns_results_or_no_match() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    // Ancestors query with fresh causality graph returns no matches
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args([
            "registry",
            "query",
            "--ancestors",
            "0000000000000000000000000000000000000000000000000000000000000000",
            "--campaign",
            "test-campaign",
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("No matching entries found"));
}

#[test]
fn registry_verify_rebuild_index_succeeds() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]},"signal":{"kind":{"Action":{"name":"defend","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":2,"channel":"actions","timestamp":1714000001000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args(["registry", "verify", "--rebuild-index", "test-campaign"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Index rebuilt from log"));
}

#[test]
fn registry_query_missing_campaign_fails() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "query", "--semantic", "test"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn registry_verify_missing_campaign_fails() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "verify"])
        .assert()
        .failure()
        .stderr(predicate::str::contains("error:"));
}

#[test]
fn registry_query_help_shows_query_flags() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "query", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--semantic"))
        .stdout(predicate::str::contains("--ancestors"))
        .stdout(predicate::str::contains("--campaign"))
        .stdout(predicate::str::contains("--turn-range"))
        .stdout(predicate::str::contains("--origin"))
        .stdout(predicate::str::contains("--last"));
}

#[test]
fn registry_verify_help_shows_verify_flags() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "verify", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("--rebuild-index"))
        .stdout(predicate::str::contains("<CAMPAIGN>"));
}

#[test]
fn registry_verify_shows_consistency_report() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":{"Action":{"name":"attack","args":{}}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args(["registry", "verify", "test-campaign"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Verification report"))
        .stdout(predicate::str::contains("Entries in log:"))
        .stdout(predicate::str::contains("Entries in index:"))
        .stdout(predicate::str::contains("Status:"));
}

#[test]
fn registry_replay_help_shows_replay_usage() {
    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["registry", "replay", "--help"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Usage:"))
        .stdout(predicate::str::contains("replay"))
        .stdout(predicate::str::contains(
            "Replay signal log through arbiter chain",
        ))
        .stdout(predicate::str::contains("<CAMPAIGN>"))
        .stdout(predicate::str::contains("--turn-range"));
}

#[test]
fn registry_replay_empty_campaign() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("empty-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");
    std::fs::write(&signals_jsonl, "").expect("failed to write empty signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args(["registry", "replay", "empty-campaign"])
        .assert()
        .success()
        .stdout(predicate::str::contains("No entries"));
}

#[test]
fn registry_replay_replays_entries() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let campaign_dir = temp_dir.path().join(".mcl").join("test-campaign");
    std::fs::create_dir_all(&campaign_dir).expect("failed to create campaign dir");

    let signals_jsonl = campaign_dir.join("signals.jsonl");

    let entries = [
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0]},"signal":{"kind":"Noop","span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":1,"channel":"actions","timestamp":1714000000000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,1]},"signal":{"kind":{"Narrate":{"tag":"scene_desc"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":2,"channel":"actions","timestamp":1714000001000,"verdict":"Accept"}"#,
        r#"{"id":{"hash":[0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,0,2]},"signal":{"kind":{"Query":{"path":"look"}},"span":{"offset":0,"len":1,"line":1,"col":1}},"origin":"player","turn":3,"channel":"queries","timestamp":1714000002000,"verdict":"Accept"}"#,
    ];

    std::fs::write(&signals_jsonl, entries.join("\n")).expect("failed to write signals.jsonl");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .current_dir(temp_dir.path())
        .args(["registry", "replay", "test-campaign"])
        .assert()
        .success()
        .stdout(predicate::str::contains("Replaying 3 entries"))
        .stdout(predicate::str::contains("Replay summary"))
        .stdout(predicate::str::contains("CONSISTENT"));
}

#[test]
fn cli_policy_lint_valid_file_exits_zero() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let mcl_file = temp_dir.path().join("test_policy.mcl");

    let content = r#"---
context: policy_test
type: room
---

# Policies
policy simple_deny {
  on:           signal_admit
  on_violation: deny
}
"#;
    std::fs::write(&mcl_file, content).expect("failed to write mcl file");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["policy", "lint", mcl_file.to_str().expect("invalid path")])
        .assert()
        .success()
        .stdout(predicate::str::contains("ok: simple_deny"))
        .stdout(predicate::str::contains("policy/policies OK"));
}

#[test]
fn cli_policy_lint_catches_type_error() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let mcl_file = temp_dir.path().join("bad_policy.mcl");

    let content = r#"---
context: policy_test
type: room
---

# Policies
policy bad_path {
  on:           signal_admit
  match:        nonexistent_top_level
  on_violation: deny
}
"#;
    std::fs::write(&mcl_file, content).expect("failed to write mcl file");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args(["policy", "lint", mcl_file.to_str().expect("invalid path")])
        .assert()
        .failure()
        .stderr(predicate::str::contains("FAIL: bad_path"))
        .stderr(predicate::str::contains("policy error"));
}

#[test]
fn cli_policy_explain_prints_compiled_rule() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let mcl_file = temp_dir.path().join("explain_policy.mcl");

    let content = r#"---
context: policy_test
type: room
---

# Policies
policy spend_limit {
  on:           signal_admit
  match:        signal.args.amount
  require:      signal.args.amount <= 1000
  on_violation: reject(reason="daily_limit_exceeded")
  priority:     high
}
"#;
    std::fs::write(&mcl_file, content).expect("failed to write mcl file");

    Command::cargo_bin("mcl-cli")
        .expect("failed to find mcl-cli binary")
        .args([
            "policy",
            "explain",
            "spend_limit",
            "--file",
            mcl_file.to_str().expect("invalid path"),
        ])
        .assert()
        .success()
        .stdout(predicate::str::contains("policy: spend_limit"))
        .stdout(predicate::str::contains("on:"))
        .stdout(predicate::str::contains("match:"))
        .stdout(predicate::str::contains("require_exprs"))
        .stdout(predicate::str::contains("on_violation:"))
        .stdout(predicate::str::contains("priority:"));
}
