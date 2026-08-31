use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use mcl_llm::LlmBackend;
use mcl_runtime::{AuditLog, Session};
use mcl_spec::MclError;

pub async fn run(
    path: PathBuf,
    llm: Box<dyn LlmBackend>,
    streaming: bool,
    start_context: Option<String>,
    save_path: Option<PathBuf>,
    load_path: Option<PathBuf>,
    audit_log_path: Option<PathBuf>,
) -> Result<(), MclError> {
    let mut session = Session::new(llm);

    if path.is_dir() {
        session.contexts.load_directory(&path)?;
    } else {
        session.contexts.load_file(&path)?;
    }

    mcl_domains::rpg::register_rpg_resolvers(&mut session.resolvers);

    if let Some(ref load_file) = load_path {
        match session.load(load_file) {
            Ok(_) => println!("Loaded session from {}", load_file.display()),
            Err(e) => eprintln!("warning: failed to load session: {e}"),
        }
    }

    let context_ids = session.contexts.context_ids();
    if context_ids.is_empty() {
        return Err(MclError::State("no contexts loaded".to_string()));
    }

    let start_id = start_context.unwrap_or_else(|| context_ids[0].to_string());

    if session.active_context().is_none() {
        session.contexts.activate(&start_id, &mut session.state)?;
    }

    if let Some(ref log_path) = audit_log_path {
        match AuditLog::open(log_path) {
            Ok(log) => {
                session.state.set_audit_log(log);
                println!("Audit log: {}", log_path.display());
            }
            Err(e) => eprintln!("warning: failed to open audit log: {e}"),
        }
    }

    if let Some(doc) = session.contexts.active_doc() {
        println!("\n=== MCL: {} ===", doc.frontmatter.context);
        if let Some(ref ctx) = doc.narrator.context {
            println!("\n{ctx}");
        }
    } else {
        println!("\n=== MCL ===");
    }
    println!("\nType your actions. Commands: /quit, /state, /context, /help\n");

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        let ctx_name = session.active_context().unwrap_or("?");
        print!("[{ctx_name}] > ");
        stdout.flush().ok();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() || input.is_empty() {
            break;
        }
        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "/quit" | "/q" | "/exit" => {
                println!("Farewell.");
                break;
            }
            "/state" => {
                println!("\n--- State ---");
                for (k, v) in &session.state.local {
                    println!("  {k}: {v}");
                }
                if !session.state.player.is_empty() {
                    println!("  player:");
                    for (k, v) in &session.state.player {
                        println!("    {k}: {v}");
                    }
                }
                for (id, props) in &session.state.entities {
                    println!("  entity {id}:");
                    for (k, v) in props {
                        println!("    {k}: {v}");
                    }
                }
                println!("---\n");
                continue;
            }
            "/context" => {
                if let Some(doc) = session.contexts.active_doc() {
                    println!("\nContext: {}", doc.frontmatter.context);
                    println!("Type: {}", doc.frontmatter.context_type);
                    println!(
                        "Actions: {}",
                        doc.actions
                            .keys()
                            .map(|s| s.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    println!(
                        "Transitions: {}",
                        doc.transitions
                            .iter()
                            .map(|t| t.target.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                    println!();
                } else {
                    println!("\nNo active context.\n");
                }
                continue;
            }
            "/help" => {
                println!("\nCommands:");
                println!("  /quit     — exit the session");
                println!("  /state    — show current state");
                println!("  /context  — show current context info");
                println!("  /help     — this message");
                println!("\nAnything else is sent to the LLM as player input.\n");
                continue;
            }
            _ => {}
        }

        match session.turn(input).await {
            Ok(result) => {
                if streaming {
                    for line in &result.narration {
                        print!("{line}");
                        stdout.flush().ok();
                    }
                    println!();
                } else {
                    for line in &result.narration {
                        println!("{line}");
                    }
                }

                for res in &result.resolutions {
                    if let Some(ref hint) = res.narration_hint {
                        println!("\n[{}: {}]", res.action, hint);
                    }
                }

                for warning in &result.warnings {
                    eprintln!("[warning: {warning}]");
                }

                if let Some(ref target) = result.transitioned_to {
                    println!("\n--- Entering {target} ---");
                    if let Some(doc) = session.contexts.active_doc() {
                        if let Some(ref ctx) = doc.narrator.context {
                            println!("{ctx}");
                        }
                    }
                }

                println!();
            }
            Err(e) => {
                eprintln!("[error: {e}]");
            }
        }
    }

    if let Some(ref save_file) = save_path {
        if let Err(e) = session.save(save_file) {
            eprintln!("warning: failed to save session: {e}");
        } else {
            println!("Session saved to {}", save_file.display());
        }
    }

    Ok(())
}
