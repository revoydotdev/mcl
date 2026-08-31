//! CLI library: command handlers for play, graph, validate, inspect, dot, bench, playtest, eval.
//!
//! Provides the [`Cli`] and [`Commands`] clap definitions, backend construction,
//! and all command-dispatch logic. The binary (`main.rs`) is a thin wrapper that
//! parses args and calls [`run`].

pub mod bench;
pub mod eval;
pub mod playtest;
pub mod repl;

use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "mcl", version, about = "Model Control Language runtime")]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(long, global = true)]
    pub debug: bool,
}

#[derive(Subcommand)]
pub enum RegistryAction {
    /// Inspect signal log entries
    Inspect {
        /// Campaign name (corresponds to .mcl/{campaign}/ directory)
        campaign: String,

        /// Number of recent entries to show
        #[arg(long, default_value = "20")]
        last: usize,
    },

    /// Query signal registry
    Query {
        /// Semantic search text
        #[arg(long)]
        semantic: Option<String>,

        /// Ancestor signal ID (hex)
        #[arg(long)]
        ancestors: Option<String>,

        /// Turn range (e.g., "1..10")
        #[arg(long)]
        turn_range: Option<String>,

        /// Origin node ID
        #[arg(long)]
        origin: Option<String>,

        /// Number of recent entries to show (default: 10)
        #[arg(long, default_value = "10")]
        last: usize,

        /// Campaign name (corresponds to .mcl/{campaign}/ directory)
        #[arg(long)]
        campaign: String,
    },

    /// Verify registry consistency
    Verify {
        /// Campaign name (corresponds to .mcl/{campaign}/ directory)
        campaign: String,

        /// Rebuild index from log
        #[arg(long)]
        rebuild_index: bool,
    },

    /// Replay signal log through arbiter chain to verify verdict consistency
    Replay {
        /// Campaign name
        campaign: String,
        /// Turn range (e.g., "1..10")
        #[arg(long)]
        turn_range: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum PolicyAction {
    /// Lint policy definitions: parse, compile, type-check
    Lint {
        /// Path to .mcl file containing policies
        file: PathBuf,
    },

    /// Explain a compiled policy: show rule details
    Explain {
        /// Name of the policy to explain
        name: String,

        /// Path to .mcl file (optional if only one policy exists)
        #[arg(long)]
        file: Option<PathBuf>,
    },
}

#[derive(Subcommand)]
pub enum Commands {
    /// Run an interactive MCL session
    Play {
        /// Path to .mcl file or directory of .mcl files
        path: PathBuf,

        /// LLM backend to use
        #[arg(long, default_value = "ollama")]
        backend: String,

        /// Model name
        #[arg(long, default_value = "llama3")]
        model: String,

        /// API key (for openai/anthropic backends)
        #[arg(long, env = "MCL_API_KEY")]
        api_key: Option<String>,

        /// Base URL override (for openai-compatible endpoints)
        #[arg(long)]
        base_url: Option<String>,

        /// Enable streaming narration output
        #[arg(long)]
        streaming: bool,

        /// Starting context ID (defaults to first loaded)
        #[arg(long)]
        start: Option<String>,

        /// Save session to file on exit
        #[arg(long)]
        save: Option<PathBuf>,

        /// Load session from file
        #[arg(long)]
        load: Option<PathBuf>,

        /// Path for append-only JSONL audit log
        #[arg(long)]
        audit_log: Option<PathBuf>,
    },

    /// Validate MCL documents without running
    Validate {
        /// Path to .mcl file or directory
        path: PathBuf,
    },

    /// Show parsed document structure
    Inspect {
        /// Path to .mcl file
        path: PathBuf,
    },

    /// Run a node-graph MCL session from a .graph.toml definition
    Graph {
        /// Path to .graph.toml file
        path: PathBuf,
    },

    /// Export graph as DOT (Graphviz) format
    Dot {
        /// Graph TOML file to visualize
        #[arg(value_name = "FILE")]
        path: PathBuf,
    },

    /// Run signal reliability benchmark against an LLM
    Bench {
        /// Path to .mcl file or directory of .mcl files
        path: PathBuf,

        /// Use mock backend (no real LLM needed)
        #[arg(long)]
        mock: bool,

        /// LLM backend to use
        #[arg(long, default_value = "ollama")]
        backend: String,

        /// Model name
        #[arg(long, default_value = "llama3")]
        model: String,

        /// API key (for openai/anthropic backends)
        #[arg(long, env = "MCL_API_KEY")]
        api_key: Option<String>,

        /// Base URL override (for openai-compatible endpoints)
        #[arg(long)]
        base_url: Option<String>,

        /// Number of runs per scenario
        #[arg(long, default_value_t = 5)]
        runs: usize,

        /// Number of scenarios to run (default: all 20)
        #[arg(long)]
        count: Option<usize>,
    },

    /// Run automated playtest sessions
    Playtest {
        /// Path to .graph.toml file
        path: PathBuf,

        /// Player mode: scripted or llm
        #[arg(long, default_value = "scripted")]
        mode: String,

        /// Number of playthrough runs
        #[arg(long, default_value_t = 3)]
        runs: usize,

        /// Max turns per run
        #[arg(long, default_value_t = 20)]
        max_turns: usize,

        /// LLM backend for narrator
        #[arg(long, default_value = "openai")]
        backend: String,

        /// Narrator model
        #[arg(long, default_value = "meta-llama/llama-4-scout")]
        model: String,

        /// API key
        #[arg(long, env = "MCL_API_KEY")]
        api_key: Option<String>,

        /// Base URL
        #[arg(long, default_value = "https://openrouter.ai/api/v1")]
        base_url: String,

        /// JSONL output file
        #[arg(long)]
        output: Option<PathBuf>,
    },

    /// Serve MCL contexts as an MCP server over stdio
    ServeMcp {
        /// Directory of .mcl context files
        #[arg(long)]
        contexts: PathBuf,
        /// Starting context name
        #[arg(long, default_value = "crypt_antechamber")]
        start: String,
    },

    /// Run multi-model signal protocol evaluation
    Eval {
        /// Path to .mcl file
        path: PathBuf,

        /// Number of runs per scenario per model
        #[arg(long, default_value_t = 3)]
        runs: usize,

        /// JSONL output file for detailed results
        #[arg(long)]
        output: Option<PathBuf>,

        /// Use structured JSON output mode
        #[arg(long)]
        structured: bool,
    },

    /// Inspect signal registry
    Registry {
        #[command(subcommand)]
        action: RegistryAction,
    },

    /// Policy lint and explain subcommands
    Policy {
        #[command(subcommand)]
        action: PolicyAction,
    },
}

/// Build an [`mcl_llm::LlmBackend`] from the given backend name and credentials.
pub fn build_llm_backend(
    backend: &str,
    model: &str,
    api_key: Option<&str>,
    base_url: Option<&str>,
) -> Box<dyn mcl_llm::LlmBackend> {
    match backend {
        "ollama" => {
            let mut b = mcl_llm::ollama::OllamaBackend::new(model);
            if let Some(url) = base_url {
                b = b.with_url(url);
            }
            Box::new(b)
        }
        "openai" => {
            let key = api_key
                .map(String::from)
                .unwrap_or_else(|| std::env::var("OPENAI_API_KEY").unwrap_or_default());
            if key.is_empty() {
                eprintln!("error: --api-key or OPENAI_API_KEY required");
                std::process::exit(1);
            }
            let mut b = mcl_llm::openai::OpenAiBackend::new(model, &key);
            if let Some(url) = base_url {
                b = b.with_url(url);
            }
            Box::new(b)
        }
        "anthropic" => {
            let key = api_key
                .map(String::from)
                .unwrap_or_else(|| std::env::var("ANTHROPIC_API_KEY").unwrap_or_default());
            if key.is_empty() {
                eprintln!("error: --api-key or ANTHROPIC_API_KEY required");
                std::process::exit(1);
            }
            Box::new(mcl_llm::anthropic::AnthropicBackend::new(model, &key))
        }
        other => {
            eprintln!("unknown backend: {other}");
            eprintln!("available: ollama, openai, anthropic");
            std::process::exit(1);
        }
    }
}

/// Run a node-graph session from a `.graph.toml` file.
pub async fn cmd_graph(path: &Path) -> Result<(), mcl_graph::GraphError> {
    use mcl_graph::{GraphDefParser, GraphExecutor, NodeFactory, TomlGraphDefParser};

    let parser = TomlGraphDefParser;
    let def = parser.parse_file(path)?;
    println!(
        "loaded graph: {} ({} nodes, {} edges)",
        def.name,
        def.nodes.len(),
        def.edges.len()
    );

    let mut factory = NodeFactory::new();
    mcl_nodes::register_default_nodes(&mut factory);

    let mut executor = GraphExecutor::build(&def, &factory).await?;
    executor.run_until_quiescent().await?;
    executor.run_interactive().await
}

/// Validate one or more `.mcl` documents.
pub fn cmd_validate(path: &Path) -> Result<(), mcl_spec::MclError> {
    if path.is_dir() {
        let mut count = 0;
        for entry in std::fs::read_dir(path)? {
            let entry = entry.map_err(|e| mcl_spec::MclError::Io(e.to_string()))?;
            if entry.path().extension().is_some_and(|e| e == "mcl") {
                match mcl_parse::parse_file(&entry.path()) {
                    Ok(doc) => {
                        println!(
                            "  ok: {} ({})",
                            doc.frontmatter.context,
                            entry.path().display()
                        );
                        count += 1;
                    }
                    Err(e) => {
                        eprintln!("  FAIL: {} — {e}", entry.path().display());
                    }
                }
            }
        }
        println!("{count} document(s) validated");
    } else {
        let doc = mcl_parse::parse_file(path)?;
        println!(
            "ok: {} ({} actions, {} entities, {} transitions)",
            doc.frontmatter.context,
            doc.actions.len(),
            doc.entities.len(),
            doc.transitions.len(),
        );
    }
    Ok(())
}

/// Print a JSON representation of a parsed `.mcl` document.
pub fn cmd_inspect(path: &Path) -> Result<(), mcl_spec::MclError> {
    let doc = mcl_parse::parse_file(path)?;
    match serde_json::to_string_pretty(&doc) {
        Ok(s) => println!("{s}"),
        Err(e) => eprintln!("warning: could not serialize document: {e}"),
    }
    Ok(())
}

/// Export a `.graph.toml` file as DOT (Graphviz) format.
pub fn cmd_dot(path: &Path) -> Result<(), mcl_graph::GraphError> {
    use mcl_graph::{export_dot, GraphDefParser, TomlGraphDefParser};

    let parser = TomlGraphDefParser;
    let def = parser.parse_file(path)?;
    print!("{}", export_dot(&def));
    Ok(())
}

/// Inspect signal registry log.
fn cmd_registry_inspect(campaign: &str, last: usize) -> Result<(), Box<dyn std::error::Error>> {
    use mcl_registry::{FsyncPolicy, SignalLog};

    let log_path = std::path::PathBuf::from(format!(".mcl/{}/signals.jsonl", campaign));

    let log = SignalLog::open(&log_path, FsyncPolicy::OnShutdown)?;
    let mut entries = log.read()?;

    if entries.is_empty() {
        println!("No entries in signal log");
        return Ok(());
    }

    let to_show = last.min(entries.len());
    let start = entries.len() - to_show;
    entries.truncate(start + to_show);

    println!("Last {} entries from {}:", to_show, log_path.display());
    for entry in entries.into_iter().skip(start) {
        let verdict_str = match &entry.verdict {
            mcl_registry::Verdict::Accept => "ACCEPT".to_string(),
            mcl_registry::Verdict::Reject { reason } => {
                format!("REJECT ({})", reason)
            }
            mcl_registry::Verdict::Defer => "DEFER".to_string(),
            mcl_registry::Verdict::RouteTo { channel, policy_id } => {
                format!("ROUTE_TO({channel}) via {policy_id}")
            }
        };

        let signal_summary = match &entry.signal.kind {
            mcl_spec::signal::SignalKind::Action { name, .. } => {
                format!("action: {}", name)
            }
            mcl_spec::signal::SignalKind::Query { path, .. } => {
                format!("query: {}", path)
            }
            mcl_spec::signal::SignalKind::Mutate { path, .. } => {
                format!("mutate: {}", path)
            }
            mcl_spec::signal::SignalKind::Transition { context } => {
                format!("transition: {}", context)
            }
            mcl_spec::signal::SignalKind::Dialogue { entity, .. } => {
                format!("dialogue: {}", entity)
            }
            mcl_spec::signal::SignalKind::Narrate { tag } => {
                format!("narrate: {}", tag)
            }
            mcl_spec::signal::SignalKind::Error { message } => {
                format!("error: {}", message)
            }
            mcl_spec::signal::SignalKind::Noop => "noop".to_string(),
        };

        println!(
            "  Turn {}: {} from {} on {} | {} | {}",
            entry.turn,
            entry.origin,
            entry.channel,
            signal_summary,
            verdict_str,
            entry.id.hex()
        );
    }

    Ok(())
}

/// Query signal registry.
fn cmd_registry_query(
    campaign: &str,
    semantic: Option<String>,
    ancestors: Option<String>,
    turn_range: Option<String>,
    origin: Option<String>,
    last: usize,
) -> Result<(), Box<dyn std::error::Error>> {
    use mcl_registry::{FsyncPolicy, Registry, RegistryQuery};

    let log_path = std::path::PathBuf::from(format!(".mcl/{}/signals.jsonl", campaign));
    let mut registry = Registry::new(&log_path, FsyncPolicy::OnShutdown)?;

    let query = if let Some(text) = semantic {
        RegistryQuery::BySemantic { text, k: last }
    } else if let Some(signal_id_hex) = ancestors {
        let signal_id = mcl_registry::SignalId::from_hex(&signal_id_hex)?;
        RegistryQuery::ByAncestors { signal_id }
    } else if let Some(range_str) = turn_range {
        let parts: Vec<&str> = range_str.split("..").collect();
        if parts.len() != 2 {
            return Err(format!(
                "invalid turn-range format: expected 'start..end', got '{}'",
                range_str
            )
            .into());
        }
        let start: u64 = parts[0]
            .parse()
            .map_err(|_| format!("invalid turn range start: {}", parts[0]))?;
        let end: u64 = parts[1]
            .parse()
            .map_err(|_| format!("invalid turn range end: {}", parts[1]))?;
        RegistryQuery::ByTurnRange { start, end }
    } else if let Some(node_id_str) = origin {
        let node_id = mcl_graph::id::NodeId::new(&node_id_str);
        RegistryQuery::ByOrigin { node_id }
    } else {
        let mut entries = registry.log().read()?;
        if entries.is_empty() {
            println!("No entries in signal log");
            return Ok(());
        }
        let to_show = last.min(entries.len());
        let start = entries.len() - to_show;
        entries.truncate(start + to_show);

        println!("Last {} entries from {}:", to_show, log_path.display());
        for entry in entries.into_iter().skip(start) {
            let verdict_str = match &entry.verdict {
                mcl_registry::Verdict::Accept => "ACCEPT".to_string(),
                mcl_registry::Verdict::Reject { reason } => {
                    format!("REJECT ({})", reason)
                }
                mcl_registry::Verdict::Defer => "DEFER".to_string(),
                mcl_registry::Verdict::RouteTo { channel, policy_id } => {
                    format!("ROUTE_TO({channel}) via {policy_id}")
                }
            };

            let signal_summary = match &entry.signal.kind {
                mcl_spec::signal::SignalKind::Action { name, .. } => {
                    format!("action: {}", name)
                }
                mcl_spec::signal::SignalKind::Query { path, .. } => {
                    format!("query: {}", path)
                }
                mcl_spec::signal::SignalKind::Mutate { path, .. } => {
                    format!("mutate: {}", path)
                }
                mcl_spec::signal::SignalKind::Transition { context } => {
                    format!("transition: {}", context)
                }
                mcl_spec::signal::SignalKind::Dialogue { entity, .. } => {
                    format!("dialogue: {}", entity)
                }
                mcl_spec::signal::SignalKind::Narrate { tag } => {
                    format!("narrate: {}", tag)
                }
                mcl_spec::signal::SignalKind::Error { message } => {
                    format!("error: {}", message)
                }
                mcl_spec::signal::SignalKind::Noop => "noop".to_string(),
            };

            println!(
                "  Turn {}: {} from {} on {} | {} | {}",
                entry.turn,
                entry.origin,
                entry.channel,
                signal_summary,
                verdict_str,
                entry.id.hex()
            );
        }
        return Ok(());
    };

    let result = registry.query(query)?;
    if result.entries.is_empty() {
        println!("No matching entries found");
        return Ok(());
    }

    println!("Found {} matching entries:", result.total);
    for entry in result.entries {
        let verdict_str = match &entry.verdict {
            mcl_registry::Verdict::Accept => "ACCEPT".to_string(),
            mcl_registry::Verdict::Reject { reason } => {
                format!("REJECT ({})", reason)
            }
            mcl_registry::Verdict::Defer => "DEFER".to_string(),
            mcl_registry::Verdict::RouteTo { channel, policy_id } => {
                format!("ROUTE_TO({channel}) via {policy_id}")
            }
        };

        let signal_summary = match &entry.signal.kind {
            mcl_spec::signal::SignalKind::Action { name, .. } => {
                format!("action: {}", name)
            }
            mcl_spec::signal::SignalKind::Query { path, .. } => {
                format!("query: {}", path)
            }
            mcl_spec::signal::SignalKind::Mutate { path, .. } => {
                format!("mutate: {}", path)
            }
            mcl_spec::signal::SignalKind::Transition { context } => {
                format!("transition: {}", context)
            }
            mcl_spec::signal::SignalKind::Dialogue { entity, .. } => {
                format!("dialogue: {}", entity)
            }
            mcl_spec::signal::SignalKind::Narrate { tag } => {
                format!("narrate: {}", tag)
            }
            mcl_spec::signal::SignalKind::Error { message } => {
                format!("error: {}", message)
            }
            mcl_spec::signal::SignalKind::Noop => "noop".to_string(),
        };

        println!(
            "  Turn {}: {} from {} on {} | {} | {}",
            entry.turn,
            entry.origin,
            entry.channel,
            signal_summary,
            verdict_str,
            entry.id.hex()
        );
    }

    Ok(())
}

/// Verify registry consistency.
fn cmd_registry_verify(
    campaign: &str,
    should_rebuild_index: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    use mcl_registry::{rebuild_index, verify_consistency, FsyncPolicy, Registry};

    let log_path = std::path::PathBuf::from(format!(".mcl/{}/signals.jsonl", campaign));
    let mut registry = Registry::new(&log_path, FsyncPolicy::OnShutdown)?;

    if should_rebuild_index {
        let (log, index) = registry.log_and_index_mut();
        rebuild_index(log, index)?;
        println!("Index rebuilt from log");
    } else {
        let report = verify_consistency(registry.log(), registry.index())?;
        println!("Verification report:");
        println!("  Entries in log: {}", report.entries_in_log);
        println!("  Entries in index: {}", report.entries_in_index);
        if report.consistent {
            println!("  Status: CONSISTENT");
        } else {
            println!("  Status: INCONSISTENT");
        }
    }

    Ok(())
}

fn verdict_str(v: &mcl_registry::Verdict) -> String {
    match v {
        mcl_registry::Verdict::Accept => "ACCEPT".to_string(),
        mcl_registry::Verdict::Reject { reason } => format!("REJECT ({})", reason),
        mcl_registry::Verdict::Defer => "DEFER".to_string(),
        mcl_registry::Verdict::RouteTo { channel, policy_id } => {
            format!("ROUTE_TO({channel}) via {policy_id}")
        }
    }
}

fn cmd_registry_replay(
    campaign: &str,
    turn_range: Option<&str>,
) -> Result<(), Box<dyn std::error::Error>> {
    use mcl_registry::{
        ActionWhitelistArbiter, ArbiterChain, DedupArbiter, FsyncPolicy, HallucinationArbiter,
        IngestContext, SchemaArbiter, SignalLog,
    };
    use mcl_spec::source::SourceId;

    let log_path = std::path::PathBuf::from(format!(".mcl/{}/signals.jsonl", campaign));

    if !log_path.exists() {
        return Err(format!("campaign '{}' not found: {}", campaign, log_path.display()).into());
    }

    let log = SignalLog::open(&log_path, FsyncPolicy::OnShutdown)?;
    let mut entries = log.read()?;

    if entries.is_empty() {
        println!("No entries in signal log for campaign '{}'", campaign);
        return Ok(());
    }

    if let Some(range_str) = turn_range {
        let parts: Vec<&str> = range_str.split("..").collect();
        if parts.len() != 2 {
            return Err(format!(
                "invalid turn-range format: expected 'start..end', got '{}'",
                range_str
            )
            .into());
        }
        let start: u64 = parts[0]
            .parse()
            .map_err(|_| format!("invalid turn range start: {}", parts[0]))?;
        let end: u64 = parts[1]
            .parse()
            .map_err(|_| format!("invalid turn range end: {}", parts[1]))?;
        entries.retain(|e| e.turn >= start && e.turn <= end);
        if entries.is_empty() {
            println!("No entries in turn range {}", range_str);
            return Ok(());
        }
    }

    let mut chain = ArbiterChain::new();
    chain.add(Box::new(SchemaArbiter::new()));
    chain.add(Box::new(DedupArbiter::new(1024)));
    chain.add(Box::new(HallucinationArbiter::new(
        std::collections::HashSet::new(),
    )));
    chain.add(Box::new(ActionWhitelistArbiter::new(
        std::collections::HashSet::new(),
    )));

    println!(
        "Replaying {} entries through arbiter chain for campaign '{}'",
        entries.len(),
        campaign
    );

    let mut mismatch_count = 0u64;

    for entry in &entries {
        let ctx = IngestContext {
            origin_node: entry.origin.clone(),
            turn: entry.turn,
            channel: entry.channel.clone(),
            source: SourceId::default(),
        };

        let replay_verdict = chain.review(&entry.signal, &ctx);

        if replay_verdict != entry.verdict {
            mismatch_count += 1;
            println!(
                "  MISMATCH turn {}: was {}, now {} | {}",
                entry.turn,
                verdict_str(&entry.verdict),
                verdict_str(&replay_verdict),
                entry.id.hex()
            );
        }
    }

    println!("\nReplay summary:");
    println!("  Entries replayed: {}", entries.len());
    println!("  Mismatches: {}", mismatch_count);
    if mismatch_count == 0 {
        println!("  Status: CONSISTENT — all verdicts match");
    } else {
        println!(
            "  Status: INCONSISTENT — {}/{} verdicts differ",
            mismatch_count,
            entries.len()
        );
    }

    Ok(())
}

fn cmd_policy_lint(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let doc = mcl_parse::parse_file(path)?;

    if doc.policies.is_empty() {
        println!("no policies found in {}", path.display());
        return Ok(());
    }

    let mut error_count = 0usize;
    for (name, def) in &doc.policies {
        match mcl_policy::compile::compile_policy(def) {
            Ok(_) => println!("  ok: {name}"),
            Err(e) => {
                eprintln!("  FAIL: {name} — {e}");
                error_count += 1;
            }
        }
    }

    if error_count > 0 {
        Err(format!("{error_count} policy error(s) in {}", path.display()).into())
    } else {
        println!("{} policy/policies OK", doc.policies.len());
        Ok(())
    }
}

fn cmd_policy_explain(name: &str, file: Option<&Path>) -> Result<(), Box<dyn std::error::Error>> {
    let path = file.ok_or("policy explain requires --file <path>")?;
    let doc = mcl_parse::parse_file(path)?;

    let def = doc
        .policies
        .get(name)
        .ok_or_else(|| format!("policy `{name}` not found in {}", path.display()))?;

    let compiled = mcl_policy::compile::compile_policy(def)?;

    println!("policy: {}", compiled.name);
    println!("  on: {:?}", compiled.on);
    println!(
        "  match: {}",
        match &compiled.match_expr {
            Some(e) => format!("{e:?}"),
            None => "<always>".to_string(),
        }
    );
    println!("  require_exprs ({}):", compiled.require_exprs.len());
    for (i, expr) in compiled.require_exprs.iter().enumerate() {
        println!("    [{}] {:?}", i, expr);
    }
    println!(
        "  transform: {}",
        match &compiled.transform {
            Some(t) => format!("{t:?}"),
            None => "<none>".to_string(),
        }
    );
    println!("  on_violation: {:?}", compiled.on_violation);
    println!("  priority: {:?}", compiled.priority);

    Ok(())
}

/// Parse CLI args and dispatch to the appropriate command handler.
pub async fn run(cli: Cli) {
    match cli.command {
        Commands::Play {
            path,
            backend,
            model,
            api_key,
            base_url,
            streaming,
            start,
            save,
            load,
            audit_log,
        } => {
            let llm = build_llm_backend(&backend, &model, api_key.as_deref(), base_url.as_deref());
            if let Err(e) = repl::run(path, llm, streaming, start, save, load, audit_log).await {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }

        Commands::Validate { path } => {
            if let Err(e) = cmd_validate(&path) {
                eprintln!("validation failed: {e}");
                std::process::exit(1);
            }
        }

        Commands::Inspect { path } => {
            if let Err(e) = cmd_inspect(&path) {
                eprintln!("inspect failed: {e}");
                std::process::exit(1);
            }
        }

        Commands::Graph { path } => {
            if let Err(e) = cmd_graph(&path).await {
                let msg = e.to_string();
                if msg != "execution error: user quit" {
                    eprintln!("graph error: {msg}");
                    std::process::exit(1);
                }
            }
        }

        Commands::Dot { path } => {
            if let Err(e) = cmd_dot(&path) {
                eprintln!("dot error: {e}");
                std::process::exit(1);
            }
        }

        Commands::Bench {
            path,
            mock,
            backend,
            model,
            api_key,
            base_url,
            runs,
            count,
        } => {
            if mock {
                let report = bench::run_mock_benchmark(count, runs).await;
                match report {
                    Ok(r) => bench::print_report(&r),
                    Err(e) => {
                        eprintln!("benchmark error: {e}");
                        std::process::exit(1);
                    }
                }
            } else {
                let llm =
                    build_llm_backend(&backend, &model, api_key.as_deref(), base_url.as_deref());
                let report = bench::run_benchmark(llm.as_ref(), &path, count, runs).await;
                match report {
                    Ok(r) => bench::print_report(&r),
                    Err(e) => {
                        eprintln!("benchmark error: {e}");
                        std::process::exit(1);
                    }
                }
            }
        }

        Commands::Playtest {
            path,
            mode,
            runs,
            max_turns,
            backend,
            model,
            api_key,
            base_url,
            output,
        } => {
            let resolved_key =
                api_key.unwrap_or_else(|| std::env::var("MCL_API_KEY").unwrap_or_default());

            let narrator_cfg = playtest::NarratorConfig {
                backend: backend.clone(),
                model: model.clone(),
                api_key: resolved_key.clone(),
                base_url: base_url.clone(),
            };

            let report = match mode.as_str() {
                "llm" => {
                    playtest::run_llm_playtest(
                        &path,
                        runs,
                        max_turns,
                        &narrator_cfg,
                        &backend,
                        &model,
                        &resolved_key,
                        &base_url,
                    )
                    .await
                }
                _ => playtest::run_scripted_playtest(&path, runs, max_turns, &narrator_cfg).await,
            };

            playtest::print_playtest_report(&report);

            if let Some(ref out) = output {
                if let Err(e) = playtest::save_playtest_jsonl(&report, out) {
                    eprintln!("error saving output: {e}");
                    std::process::exit(1);
                }
                println!("Results saved to: {}", out.display());
            }
        }

        Commands::ServeMcp { contexts, start } => {
            match mcl_mcp::tools::McpToolHandler::new(&contexts, &start) {
                Ok(handler) => {
                    let mut server = mcl_mcp::server::McpServer::new(handler);
                    if let Err(e) = server.run() {
                        eprintln!("MCP server error: {e}");
                        std::process::exit(1);
                    }
                }
                Err(e) => {
                    eprintln!("Failed to initialize: {e}");
                    std::process::exit(1);
                }
            }
        }

        Commands::Eval {
            path,
            runs,
            output,
            structured,
        } => {
            let openrouter_key = std::env::var("OPENROUTER_API_KEY").unwrap_or_default();
            let zai_key = std::env::var("ZAI_API_KEY").unwrap_or_default();

            if openrouter_key.is_empty() {
                eprintln!("error: OPENROUTER_API_KEY env var required");
                std::process::exit(1);
            }

            let mut models = vec![
                eval::ModelConfig {
                    name: "Gemma 3 4B".to_string(),
                    model_id: "google/gemma-3-4b-it".to_string(),
                    base_url: "https://openrouter.ai/api/v1".to_string(),
                    api_key: openrouter_key.clone(),
                },
                eval::ModelConfig {
                    name: "Mistral Small 3.1 24B".to_string(),
                    model_id: "mistralai/mistral-small-3.1-24b-instruct".to_string(),
                    base_url: "https://openrouter.ai/api/v1".to_string(),
                    api_key: openrouter_key.clone(),
                },
                eval::ModelConfig {
                    name: "Llama 4 Scout".to_string(),
                    model_id: "meta-llama/llama-4-scout".to_string(),
                    base_url: "https://openrouter.ai/api/v1".to_string(),
                    api_key: openrouter_key,
                },
            ];

            if !zai_key.is_empty() {
                models.push(eval::ModelConfig {
                    name: "GLM-5 (Z.AI Coding)".to_string(),
                    model_id: "glm-5".to_string(),
                    base_url: "https://api.z.ai/api/coding/paas/v4".to_string(),
                    api_key: zai_key,
                });
            } else {
                eprintln!("warning: ZAI_API_KEY not set, skipping GLM-5");
            }

            let total = models.len() * 20 * runs;
            println!(
                "MCL Signal Protocol Evaluation\n\
                 Models: {}\n\
                 Scenarios: 20 × {} runs = {} LLM calls per model\n\
                 Total: {} LLM calls\n",
                models.len(),
                runs,
                20 * runs,
                total,
            );

            match eval::run_multi_model_eval(&models, &path, runs, output.as_deref(), structured)
                .await
            {
                Ok(reports) => {
                    eval::print_comparative_table(&reports);
                    if let Some(ref out) = output {
                        println!("\nDetailed results saved to: {}", out.display());
                    }
                }
                Err(e) => {
                    eprintln!("eval error: {e}");
                    std::process::exit(1);
                }
            }
        }

        Commands::Registry { action } => match action {
            RegistryAction::Inspect { campaign, last } => {
                if let Err(e) = cmd_registry_inspect(&campaign, last) {
                    eprintln!("registry inspect error: {e}");
                    std::process::exit(1);
                }
            }
            RegistryAction::Query {
                semantic,
                ancestors,
                turn_range,
                origin,
                last,
                campaign,
            } => {
                if let Err(e) =
                    cmd_registry_query(&campaign, semantic, ancestors, turn_range, origin, last)
                {
                    eprintln!("registry query error: {e}");
                    std::process::exit(1);
                }
            }
            RegistryAction::Verify {
                campaign,
                rebuild_index,
            } => {
                if let Err(e) = cmd_registry_verify(&campaign, rebuild_index) {
                    eprintln!("registry verify error: {e}");
                    std::process::exit(1);
                }
            }
            RegistryAction::Replay {
                campaign,
                turn_range,
            } => {
                if let Err(e) = cmd_registry_replay(&campaign, turn_range.as_deref()) {
                    eprintln!("registry replay error: {e}");
                    std::process::exit(1);
                }
            }
        },

        Commands::Policy { action } => match action {
            PolicyAction::Lint { file } => {
                if let Err(e) = cmd_policy_lint(&file) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
            PolicyAction::Explain { name, file } => {
                if let Err(e) = cmd_policy_explain(&name, file.as_deref()) {
                    eprintln!("{e}");
                    std::process::exit(1);
                }
            }
        },
    }
}
