use std::path::Path;
use std::time::Instant;

use mcl_llm::LlmBackend;
use mcl_parse::parse_file;
use mcl_signal::{classify_turn, BenchmarkScenario, ReliabilityReport, DEFAULT_SCENARIOS};
use serde::Serialize;

// ── Detailed reporting ────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ScenarioResult {
    pub scenario_idx: usize,
    pub description: String,
    pub user_input: String,
    pub expected_type: Option<String>,
    pub expected_name: Option<String>,
    pub classification: String,
    pub raw_response: String,
    pub latency_ms: u128,
    pub model: String,
}

#[derive(Debug, Clone)]
pub struct DetailedReport {
    pub summary: ReliabilityReport,
    pub results: Vec<ScenarioResult>,
    pub latencies_ms: Vec<u128>,
}

impl DetailedReport {
    pub fn latency_p50(&self) -> u128 {
        percentile(&self.latencies_ms, 50)
    }
    pub fn latency_p95(&self) -> u128 {
        percentile(&self.latencies_ms, 95)
    }
}

fn percentile(values: &[u128], pct: usize) -> u128 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let idx = (pct * sorted.len() / 100).min(sorted.len() - 1);
    sorted[idx]
}

fn collect_action_names(doc: &mcl_spec::document::MclDocument) -> Vec<String> {
    doc.actions.keys().cloned().collect()
}

pub fn build_bench_system_prompt(
    doc: &mcl_spec::document::MclDocument,
    structured: bool,
) -> String {
    let state = mcl_runtime::WorldState::new();
    if structured {
        mcl_runtime::prompt::build_structured_system_prompt(doc, &state)
    } else {
        mcl_runtime::prompt::build_system_prompt(doc, &state)
    }
}

fn generate_mock_response(scenario: &BenchmarkScenario) -> String {
    match (scenario.expected_signal_type, scenario.expected_signal_name) {
        (Some("action"), Some("attack")) => {
            "You strike at the enemy!\nMCL::action(attack, target=skeleton_guard)".to_string()
        }
        (Some("action"), Some("look")) => {
            "You survey your surroundings.\nMCL::action(look)".to_string()
        }
        (Some("action"), Some("light_torch")) => {
            "You light the sconce.\nMCL::action(light_torch)".to_string()
        }
        (Some("action"), Some("examine")) => {
            "You examine the object closely.\nMCL::action(examine)".to_string()
        }
        (Some("action"), Some("pickup")) => {
            "You pick up the item.\nMCL::action(pickup, target=flint)".to_string()
        }
        (Some("action"), Some("unlock_door")) => {
            "You unlock the door.\nMCL::action(unlock_door)".to_string()
        }
        (Some("dialogue"), Some("skeleton_guard")) => r#"You call out to the figure.
MCL::dialogue(skeleton_guard, "Who are you?")"#
            .to_string(),
        (Some("transition"), Some("crypt_inner")) => {
            "You move through the door.\nMCL::transition(crypt_inner)".to_string()
        }
        (Some("noop"), Some("noop")) => {
            "You pause and consider your options.\nMCL::noop()".to_string()
        }
        _ => "You wait.\nMCL::noop()".to_string(),
    }
}

pub async fn run_benchmark(
    backend: &dyn LlmBackend,
    context_dir: &Path,
    scenario_count: Option<usize>,
    runs_per_scenario: usize,
) -> Result<ReliabilityReport, String> {
    let detailed =
        run_benchmark_detailed(backend, context_dir, scenario_count, runs_per_scenario).await?;
    Ok(detailed.summary)
}

pub async fn run_benchmark_detailed(
    backend: &dyn LlmBackend,
    context_dir: &Path,
    scenario_count: Option<usize>,
    runs_per_scenario: usize,
) -> Result<DetailedReport, String> {
    run_benchmark_detailed_named(
        backend,
        context_dir,
        scenario_count,
        runs_per_scenario,
        None,
    )
    .await
}

pub async fn run_benchmark_detailed_named(
    backend: &dyn LlmBackend,
    context_dir: &Path,
    scenario_count: Option<usize>,
    runs_per_scenario: usize,
    display_name: Option<&str>,
) -> Result<DetailedReport, String> {
    let doc = parse_file(context_dir).map_err(|e| format!("failed to parse context: {e}"))?;

    let available_actions: Vec<String> = collect_action_names(&doc);
    let available_refs: Vec<&str> = available_actions.iter().map(|s| s.as_str()).collect();
    let system_prompt = build_bench_system_prompt(&doc, false);

    let scenarios = &DEFAULT_SCENARIOS[..scenario_count.unwrap_or(DEFAULT_SCENARIOS.len())];
    let model_name = display_name.unwrap_or_else(|| backend.name()).to_string();
    let mut report = ReliabilityReport::new(&model_name);
    let mut results = Vec::new();
    let mut latencies = Vec::new();

    for (idx, scenario) in scenarios.iter().enumerate() {
        for _ in 0..runs_per_scenario {
            let start = Instant::now();
            let response = backend
                .complete(&system_prompt, &[], scenario.user_input)
                .await
                .map_err(|e| format!("LLM error on '{}': {e}", scenario.description))?;
            let latency_ms = start.elapsed().as_millis();

            let classification = classify_turn(
                &response,
                scenario.expected_signal_type,
                scenario.expected_signal_name,
                &available_refs,
            );
            report.record(&classification);
            latencies.push(latency_ms);

            results.push(ScenarioResult {
                scenario_idx: idx,
                description: scenario.description.to_string(),
                user_input: scenario.user_input.to_string(),
                expected_type: scenario.expected_signal_type.map(String::from),
                expected_name: scenario.expected_signal_name.map(String::from),
                classification: classification.to_string(),
                raw_response: response,
                latency_ms,
                model: model_name.clone(),
            });
        }
    }

    Ok(DetailedReport {
        summary: report,
        results,
        latencies_ms: latencies,
    })
}

pub fn save_report_jsonl(report: &DetailedReport, path: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::File::create(path)
        .map_err(|e| format!("failed to create {}: {e}", path.display()))?;
    for result in &report.results {
        let json = serde_json::to_string(result).map_err(|e| format!("json error: {e}"))?;
        writeln!(file, "{json}").map_err(|e| format!("write error: {e}"))?;
    }
    Ok(())
}

pub fn print_detailed_report(report: &DetailedReport) {
    print_report(&report.summary);

    // Per-scenario breakdown
    println!("\n{}", "─".repeat(80));
    println!(
        "  {:<4} {:<40} {:<12} {:>8}",
        "#", "Scenario", "Result", "Latency"
    );
    println!(
        "  {:<4} {:<40} {:<12} {:>8}",
        "────", "────────────────────────────────────────", "────────────", "────────"
    );

    for result in &report.results {
        let marker = if result.classification == "Correct" {
            "+"
        } else {
            "x"
        };
        println!(
            "  [{marker}] {:<3} {:<40} {:<12} {:>6}ms",
            result.scenario_idx + 1,
            truncate(&result.description, 38),
            result.classification,
            result.latency_ms,
        );
    }

    println!("{}", "─".repeat(80));
    println!(
        "  Latency: p50={}ms  p95={}ms",
        report.latency_p50(),
        report.latency_p95()
    );
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}

pub async fn run_mock_benchmark(
    scenario_count: Option<usize>,
    runs_per_scenario: usize,
) -> Result<ReliabilityReport, String> {
    let available_refs: Vec<&str> = vec![
        "look",
        "examine",
        "attack",
        "pickup",
        "light_torch",
        "unlock_door",
    ];

    let scenarios = &DEFAULT_SCENARIOS[..scenario_count.unwrap_or(DEFAULT_SCENARIOS.len())];
    let mut report = ReliabilityReport::new("mock");

    for scenario in scenarios {
        for _ in 0..runs_per_scenario {
            let response = generate_mock_response(scenario);
            let classification = classify_turn(
                &response,
                scenario.expected_signal_type,
                scenario.expected_signal_name,
                &available_refs,
            );
            report.record(&classification);
        }
    }

    Ok(report)
}

pub fn print_report(report: &ReliabilityReport) {
    println!("\n{}", "═".repeat(60));
    println!("  Signal Reliability Benchmark Report");
    println!("{}", "═".repeat(60));
    println!("  Model:  {}", report.model);
    println!("  Turns:  {}", report.total_turns);
    println!("  Accuracy: {:.1}%", report.accuracy());
    println!("{}", "─".repeat(60));
    println!("  {:<20} {:>10}", "Classification", "Count");
    println!("  {:<20} {:>10}", "────────────────────", "──────────");
    println!("  {:<20} {:>10}", "Correct", report.correct_signals);
    println!("  {:<20} {:>10}", "Malformed", report.malformed_signals);
    println!("  {:<20} {:>10}", "Missing", report.missing_signals);
    println!(
        "  {:<20} {:>10}",
        "Hallucinated", report.hallucinated_signals
    );
    println!("{}", "═".repeat(60));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mock_response_for_attack_is_correct() {
        let scenario = BenchmarkScenario {
            user_input: "Attack the skeleton!",
            expected_signal_type: Some("action"),
            expected_signal_name: Some("attack"),
            description: "test",
        };
        let response = generate_mock_response(&scenario);
        assert!(response.contains("MCL::action(attack"));
    }

    #[test]
    fn mock_response_for_noop_is_correct() {
        let scenario = BenchmarkScenario {
            user_input: "I sit down",
            expected_signal_type: Some("noop"),
            expected_signal_name: Some("noop"),
            description: "test",
        };
        let response = generate_mock_response(&scenario);
        assert!(response.contains("MCL::noop()"));
    }

    #[test]
    fn mock_response_for_dialogue_is_correct() {
        let scenario = BenchmarkScenario {
            user_input: "Hello?",
            expected_signal_type: Some("dialogue"),
            expected_signal_name: Some("skeleton_guard"),
            description: "test",
        };
        let response = generate_mock_response(&scenario);
        assert!(response.contains("MCL::dialogue(skeleton_guard"));
    }

    #[tokio::test]
    async fn mock_benchmark_runs_all_scenarios() {
        let report = run_mock_benchmark(None, 1)
            .await
            .expect("mock bench failed");
        assert_eq!(report.total_turns, 20);
        assert_eq!(report.correct_signals, 20);
    }

    #[tokio::test]
    async fn mock_benchmark_respects_scenario_count() {
        let report = run_mock_benchmark(Some(5), 2)
            .await
            .expect("mock bench failed");
        assert_eq!(report.total_turns, 10);
    }

    #[tokio::test]
    async fn mock_benchmark_report_accuracy() {
        let report = run_mock_benchmark(None, 1)
            .await
            .expect("mock bench failed");
        assert!((report.accuracy() - 100.0).abs() < f64::EPSILON);
    }
}
