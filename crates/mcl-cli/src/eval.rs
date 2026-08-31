use std::path::Path;

use crate::bench::{print_detailed_report, run_benchmark_detailed_named, DetailedReport};

pub struct ModelConfig {
    pub name: String,
    pub model_id: String,
    pub base_url: String,
    pub api_key: String,
}

pub async fn run_multi_model_eval(
    models: &[ModelConfig],
    context_path: &Path,
    runs_per_scenario: usize,
    output_path: Option<&Path>,
    structured: bool,
) -> Result<Vec<DetailedReport>, String> {
    let mut all_reports = Vec::new();

    for (i, model) in models.iter().enumerate() {
        println!(
            "\n[{}/{}] Evaluating: {} ({})",
            i + 1,
            models.len(),
            model.name,
            model.model_id
        );
        println!("{}", "─".repeat(60));

        let mut backend = mcl_llm::openai::OpenAiBackend::new(&model.model_id, &model.api_key)
            .with_url(&model.base_url);

        if structured {
            backend = backend.with_response_format(mcl_llm::openai::mcl_response_schema(&[]));
        }

        match run_benchmark_detailed_named(
            &backend,
            context_path,
            None,
            runs_per_scenario,
            Some(&model.name),
        )
        .await
        {
            Ok(report) => {
                print_detailed_report(&report);
                if let Some(path) = output_path {
                    if let Err(e) = append_report_jsonl(&report, path) {
                        eprintln!("  warning: failed to save JSONL: {e}");
                    }
                }
                all_reports.push(report);
            }
            Err(e) => {
                eprintln!("  ERROR: {} failed: {e}", model.name);
                eprintln!("  Skipping to next model...\n");
            }
        }
    }

    Ok(all_reports)
}

fn append_report_jsonl(report: &DetailedReport, path: &Path) -> Result<(), String> {
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(|e| format!("failed to open {}: {e}", path.display()))?;
    for result in &report.results {
        let json = serde_json::to_string(result).map_err(|e| format!("json error: {e}"))?;
        writeln!(file, "{json}").map_err(|e| format!("write error: {e}"))?;
    }
    Ok(())
}

pub fn print_comparative_table(reports: &[DetailedReport]) {
    println!("\n{}", "═".repeat(90));
    println!("  MCL Signal Protocol Evaluation — Comparative Results");
    println!("{}", "═".repeat(90));
    println!(
        "  {:<36} {:>8} {:>8} {:>6} {:>5} {:>5} {:>7} {:>7}",
        "Model", "Accuracy", "Correct", "Miss", "Mal", "Hal", "p50ms", "p95ms"
    );
    println!(
        "  {:<36} {:>8} {:>8} {:>6} {:>5} {:>5} {:>7} {:>7}",
        "────────────────────────────────────",
        "────────",
        "────────",
        "──────",
        "─────",
        "─────",
        "───────",
        "───────"
    );

    for report in reports {
        let s = &report.summary;
        println!(
            "  {:<36} {:>7.1}% {:>8} {:>6} {:>5} {:>5} {:>7} {:>7}",
            truncate_model(&s.model, 34),
            s.accuracy(),
            s.correct_signals,
            s.missing_signals,
            s.malformed_signals,
            s.hallucinated_signals,
            report.latency_p50(),
            report.latency_p95(),
        );
    }

    println!("{}", "═".repeat(90));
}

fn truncate_model(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max - 3])
    }
}
