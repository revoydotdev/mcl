use serde::Serialize;
use std::path::Path;
use std::time::Instant;

#[derive(Debug, Clone, Serialize)]
pub struct PlaytestRun {
    pub run: usize,
    pub turns: usize,
    pub completed: bool,
    pub error: Option<String>,
    pub duration_ms: u128,
    pub transcript: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct PlaytestReport {
    pub runs: Vec<PlaytestRun>,
    pub mode: String,
    pub model: String,
}

impl PlaytestReport {
    pub fn completion_rate(&self) -> f64 {
        let completed = self.runs.iter().filter(|r| r.completed).count();
        if self.runs.is_empty() {
            0.0
        } else {
            completed as f64 / self.runs.len() as f64 * 100.0
        }
    }

    pub fn avg_turns(&self) -> f64 {
        if self.runs.is_empty() {
            0.0
        } else {
            self.runs.iter().map(|r| r.turns as f64).sum::<f64>() / self.runs.len() as f64
        }
    }

    pub fn error_count(&self) -> usize {
        self.runs.iter().filter(|r| r.error.is_some()).count()
    }
}

/// Narrator backend config passed to override graph.toml defaults
#[derive(Clone)]
pub struct NarratorConfig {
    pub backend: String,
    pub model: String,
    pub api_key: String,
    pub base_url: String,
}

/// Patch the narrator node's config in a parsed GraphDef to use CLI-provided backend settings
fn patch_narrator_config(def: &mut mcl_graph::graph_def::GraphDef, cfg: &NarratorConfig) {
    use mcl_spec::value::MclValue;
    if let Some(narrator) = def.nodes.get_mut("narrator") {
        narrator
            .config
            .insert("backend".into(), MclValue::String(cfg.backend.clone()));
        narrator
            .config
            .insert("model".into(), MclValue::String(cfg.model.clone()));
        narrator
            .config
            .insert("api_key".into(), MclValue::String(cfg.api_key.clone()));
        narrator
            .config
            .insert("base_url".into(), MclValue::String(cfg.base_url.clone()));
    }
}

pub async fn run_scripted_playtest(
    graph_path: &Path,
    num_runs: usize,
    max_turns: usize,
    narrator_cfg: &NarratorConfig,
) -> PlaytestReport {
    let mut report = PlaytestReport {
        runs: Vec::new(),
        mode: "scripted".to_string(),
        model: narrator_cfg.model.clone(),
    };

    for run_idx in 0..num_runs {
        println!("\n[Run {}/{}]", run_idx + 1, num_runs);
        let start = Instant::now();

        let result = run_single_scripted(graph_path, max_turns, narrator_cfg).await;

        let duration = start.elapsed().as_millis();

        match result {
            Ok((turns, transcript)) => {
                let completed = turns >= 2;
                println!("  {} turns, {}ms", turns, duration);
                report.runs.push(PlaytestRun {
                    run: run_idx + 1,
                    turns,
                    completed,
                    error: None,
                    duration_ms: duration,
                    transcript,
                });
            }
            Err(e) => {
                let is_normal_exit = e.contains("playthrough complete") || e.contains("max turns");
                if is_normal_exit {
                    println!("  Completed, {}ms", duration);
                    report.runs.push(PlaytestRun {
                        run: run_idx + 1,
                        turns: max_turns,
                        completed: true,
                        error: None,
                        duration_ms: duration,
                        transcript: vec![],
                    });
                } else {
                    eprintln!("  ERROR: {e}");
                    report.runs.push(PlaytestRun {
                        run: run_idx + 1,
                        turns: 0,
                        completed: false,
                        error: Some(e),
                        duration_ms: duration,
                        transcript: vec![],
                    });
                }
            }
        }
    }

    report
}

async fn run_single_scripted(
    graph_path: &Path,
    max_turns: usize,
    narrator_cfg: &NarratorConfig,
) -> Result<(usize, Vec<serde_json::Value>), String> {
    use mcl_graph::{GraphDefParser, GraphExecutor, NodeFactory, TomlGraphDefParser};
    use mcl_nodes::ScriptedPlayerNode;

    let parser = TomlGraphDefParser;
    let mut def = parser
        .parse_file(graph_path)
        .map_err(|e| format!("graph parse error: {e}"))?;
    patch_narrator_config(&mut def, narrator_cfg);

    let mut factory = NodeFactory::new();
    mcl_nodes::register_default_nodes(&mut factory);

    let player = ScriptedPlayerNode::new(
        mcl_nodes::scripted_player::default_crypt_actions(),
        max_turns,
    );

    let mut executor =
        GraphExecutor::build_with_override(&def, &factory, "client", Box::new(player))
            .await
            .map_err(|e| format!("graph build error: {e}"))?;

    executor
        .run_until_quiescent()
        .await
        .map_err(|e| format!("initial run error: {e}"))?;

    match executor.run_interactive().await {
        Ok(()) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("playthrough complete") && !msg.contains("max turns") {
                return Err(msg);
            }
        }
    }

    Ok((max_turns, vec![]))
}

pub async fn run_llm_playtest(
    graph_path: &Path,
    num_runs: usize,
    max_turns: usize,
    narrator_cfg: &NarratorConfig,
    player_backend_name: &str,
    player_model: &str,
    player_api_key: &str,
    player_base_url: &str,
) -> PlaytestReport {
    let mut report = PlaytestReport {
        runs: Vec::new(),
        mode: "llm".to_string(),
        model: narrator_cfg.model.clone(),
    };

    for run_idx in 0..num_runs {
        println!("\n[Run {}/{}]", run_idx + 1, num_runs);
        let start = Instant::now();

        let player_backend: Box<dyn mcl_llm::LlmBackend> = match player_backend_name {
            "ollama" => Box::new(mcl_llm::ollama::OllamaBackend::new(player_model)),
            _ => Box::new(
                mcl_llm::openai::OpenAiBackend::new(player_model, player_api_key)
                    .with_url(player_base_url),
            ),
        };

        let result = run_single_llm(graph_path, max_turns, player_backend, narrator_cfg).await;
        let duration = start.elapsed().as_millis();

        match result {
            Ok((turns, transcript)) => {
                println!("  {} turns, {}ms", turns, duration);
                report.runs.push(PlaytestRun {
                    run: run_idx + 1,
                    turns,
                    completed: true,
                    error: None,
                    duration_ms: duration,
                    transcript,
                });
            }
            Err(e) => {
                let is_normal = e.contains("max turns") || e.contains("playthrough complete");
                if is_normal {
                    println!("  Completed (max turns), {}ms", duration);
                    report.runs.push(PlaytestRun {
                        run: run_idx + 1,
                        turns: max_turns,
                        completed: true,
                        error: None,
                        duration_ms: duration,
                        transcript: vec![],
                    });
                } else {
                    eprintln!("  ERROR: {e}");
                    report.runs.push(PlaytestRun {
                        run: run_idx + 1,
                        turns: 0,
                        completed: false,
                        error: Some(e),
                        duration_ms: duration,
                        transcript: vec![],
                    });
                }
            }
        }
    }

    report
}

async fn run_single_llm(
    graph_path: &Path,
    max_turns: usize,
    player_backend: Box<dyn mcl_llm::LlmBackend>,
    narrator_cfg: &NarratorConfig,
) -> Result<(usize, Vec<serde_json::Value>), String> {
    use mcl_graph::{GraphDefParser, GraphExecutor, NodeFactory, TomlGraphDefParser};
    use mcl_nodes::PlayerLlmNode;

    let parser = TomlGraphDefParser;
    let mut def = parser
        .parse_file(graph_path)
        .map_err(|e| format!("graph parse error: {e}"))?;
    patch_narrator_config(&mut def, narrator_cfg);

    let mut factory = NodeFactory::new();
    mcl_nodes::register_default_nodes(&mut factory);

    let player = PlayerLlmNode::with_backend(player_backend, max_turns);

    let mut executor =
        GraphExecutor::build_with_override(&def, &factory, "client", Box::new(player))
            .await
            .map_err(|e| format!("graph build error: {e}"))?;

    executor
        .run_until_quiescent()
        .await
        .map_err(|e| format!("initial run error: {e}"))?;

    match executor.run_interactive().await {
        Ok(()) => {}
        Err(e) => {
            let msg = e.to_string();
            if !msg.contains("max turns") && !msg.contains("playthrough complete") {
                return Err(msg);
            }
        }
    }

    Ok((max_turns, vec![]))
}

pub fn print_playtest_report(report: &PlaytestReport) {
    println!("\n{}", "═".repeat(60));
    println!("  MCL Playtest Report");
    println!("{}", "═".repeat(60));
    println!("  Mode:            {}", report.mode);
    println!("  Model:           {}", report.model);
    println!("  Runs:            {}", report.runs.len());
    println!("  Completion rate: {:.0}%", report.completion_rate());
    println!("  Avg turns:       {:.1}", report.avg_turns());
    println!("  Errors:          {}", report.error_count());
    println!("{}", "─".repeat(60));

    for run in &report.runs {
        let status = if run.completed { "OK" } else { "FAIL" };
        let err = run.error.as_deref().unwrap_or("");
        println!(
            "  Run {:>2}: [{status}] {:>3} turns, {:>6}ms {}",
            run.run,
            run.turns,
            run.duration_ms,
            if err.is_empty() { "" } else { err }
        );
    }
    println!("{}", "═".repeat(60));
}

pub fn save_playtest_jsonl(report: &PlaytestReport, path: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    for run in &report.runs {
        let json = serde_json::to_string(run).map_err(|e| format!("json error: {e}"))?;
        writeln!(file, "{json}").map_err(|e| format!("write error: {e}"))?;
    }
    Ok(())
}
