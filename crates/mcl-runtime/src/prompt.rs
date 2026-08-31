use mcl_registry::{SignalLogEntry, SummaryEntry};
use mcl_spec::document::{KnowledgeEntry, MclDocument};

use crate::state::WorldState;

/// Build the system prompt preamble (shared between inline and structured modes)
fn build_prompt_preamble(doc: &MclDocument, state: &WorldState) -> String {
    let mut prompt = String::new();

    let voice = doc.narrator.voice.as_deref().unwrap_or("second_person");
    let tense = doc.narrator.tense.as_deref().unwrap_or("present");
    let ctx_type = &doc.frontmatter.context_type;

    prompt.push_str(&format!(
        "You are a narrator for an interactive {ctx_type}. \
         Narrate in {voice} {tense} tense.\n\n"
    ));

    if let Some(ref tone) = doc.narrator.tone {
        prompt.push_str(&format!("## Tone\n{tone}\n\n"));
    }

    if let Some(ref context) = doc.narrator.context {
        prompt.push_str(&format!("## Current Scene\n{context}\n\n"));
    }

    if let Some(ref notes) = doc.narrator.style_notes {
        prompt.push_str(&format!("## Style Notes\n{notes}\n\n"));
    }

    let filtered = crate::knowledge_filter::filter_state_for_knowledge(state, &doc.knowledge);

    if !filtered.local.is_empty() {
        prompt.push_str("## Current State\n");
        for (key, val) in &filtered.local {
            prompt.push_str(&format!("- {key}: {val}\n"));
        }
        prompt.push('\n');
    }

    if !doc.entities.is_empty() {
        prompt.push_str("## Entities Present\n");
        for (id, entity) in &doc.entities {
            let type_str = entity.entity_type.as_deref().unwrap_or("unknown");
            let disp_str = entity.disposition.as_deref().unwrap_or("neutral");
            prompt.push_str(&format!("- **{id}** ({type_str}, {disp_str})"));
            if let Some(props) = state.entities.get(id) {
                for (key, val) in props {
                    if key != "type" && key != "disposition" {
                        prompt.push_str(&format!(", {key}={val}"));
                    }
                }
            }
            prompt.push('\n');
            if !entity.knowledge.is_empty() {
                prompt.push_str(&format!(
                    "  - knows about: {}\n",
                    entity.knowledge.join(", ")
                ));
            }
        }
        prompt.push('\n');
    }

    if !doc.items.is_empty() {
        prompt.push_str("## Items\n");
        for (id, item) in &doc.items {
            prompt.push_str(&format!("- **{id}**"));
            for (key, val) in &item.properties {
                prompt.push_str(&format!(", {key}={val}"));
            }
            prompt.push('\n');
        }
        prompt.push('\n');
    }

    let valid_actions = get_valid_actions(doc, state);
    if !valid_actions.is_empty() {
        prompt.push_str("## Available Actions\nThe player can:\n");
        for (name, desc) in &valid_actions {
            if let Some(d) = desc {
                prompt.push_str(&format!("- **{name}**: {d}\n"));
            } else {
                prompt.push_str(&format!("- **{name}**\n"));
            }
        }
        prompt.push('\n');
    }

    prompt
}

fn build_rules_section(doc: &MclDocument) -> String {
    let voice = doc.narrator.voice.as_deref().unwrap_or("second_person");
    let tense = doc.narrator.tense.as_deref().unwrap_or("present");
    let mut rules = String::new();

    rules.push_str("## Rules\n");
    rules.push_str(&format!("- Always narrate in {voice} {tense} tense\n"));
    rules.push_str("- Never resolve mechanical outcomes yourself — emit the signal and the engine will resolve it\n");
    rules.push_str("- Never reveal information the player hasn't discovered\n");

    if !doc.narrator.forbidden.is_empty() {
        rules.push_str(&format!(
            "- Never use these words/phrases: {}\n",
            doc.narrator.forbidden.join(", ")
        ));
    }

    for entry in &doc.knowledge {
        if let KnowledgeEntry::Known(fact) = entry {
            rules.push_str(&format!("- You know about: {fact}\n"));
        }
    }

    for constraint in &doc.constraints {
        rules.push_str(&format!("- {constraint}\n"));
    }

    rules
}

/// Build the system prompt from the current context and state
pub fn build_system_prompt(doc: &MclDocument, state: &WorldState) -> String {
    let mut prompt = build_prompt_preamble(doc, state);

    // ── Signal catalog with descriptions and trigger phrases ──
    prompt.push_str(
        "## MCL Signal Protocol\n\
        EVERY response MUST end with exactly ONE MCL signal on its own line.\n\
        Write your narration first, then the signal on the LAST line.\n\n\
        Choose the MOST SPECIFIC matching signal:\n\n",
    );

    for (name, action) in &doc.actions {
        if action.target.is_some() {
            prompt.push_str(&format!("MCL::action({name}, target=<id>)"));
        } else {
            prompt.push_str(&format!("MCL::action({name})"));
        }
        if let Some(ref desc) = action.description {
            prompt.push_str(&format!(" — {desc}"));
        }
        if !action.trigger_phrases.is_empty() {
            prompt.push_str(&format!(
                "\n  Use when player says: {}",
                action.trigger_phrases.join(", ")
            ));
        }
        prompt.push('\n');
    }

    // Dialogue signal
    prompt.push_str("MCL::dialogue(<entity_id>, \"<speech>\") — Player speaks, talks, or calls out to someone\n");
    prompt.push_str("  Use when player says: talk to, speak to, call out, say, ask, tell\n");

    // Transition signals
    if !doc.transitions.is_empty() {
        for t in &doc.transitions {
            if let Some(ref dir) = t.direction {
                prompt.push_str(&format!(
                    "MCL::transition({}) — Move {dir} to another area\n",
                    t.target
                ));
                prompt.push_str(&format!(
                    "  Use when player says: go {dir}, walk {dir}, move {dir}, head {dir}, enter, leave\n"
                ));
            } else {
                prompt.push_str(&format!(
                    "MCL::transition({}) — Move to another area\n",
                    t.target
                ));
            }
        }
    }

    // Noop — last and explicitly restricted
    prompt.push_str(
        "MCL::noop() — NO game action at all\n\
        \x20 Use ONLY when: player sits, waits, thinks, emotes, or says something with no game effect\n\n",
    );

    // ── Disambiguation rules ──
    prompt.push_str(
        "## Signal Selection Rules\n\
        - EVERY player input produces exactly ONE signal — never omit it\n\
        - The signal MUST be on its own line (not inline with narration)\n\
        - When two actions could match, choose the MORE SPECIFIC one\n\
        - \"examine X\" or \"inspect X\" → MCL::action(examine), NEVER noop or look\n\
        - \"talk to X\" or \"speak to X\" → MCL::dialogue(X, \"...\"), NEVER action(speak)\n\
        - \"go <direction>\" or \"walk through <door>\" → MCL::transition(...)\n\
        - \"unlock\" or \"use key on\" → MCL::action(unlock_door), not attack\n\
        - noop is ONLY for inputs with zero game effect\n\n",
    );

    // ── Examples for EVERY signal type ──
    prompt.push_str("## Examples\n");

    // Action examples — one per action
    for (name, action) in &doc.actions {
        let trigger = action
            .trigger_phrases
            .first()
            .map(|s| s.as_str())
            .unwrap_or(name.as_str());
        if action.target.is_some() {
            if let Some((entity_id, _)) = doc.entities.iter().next() {
                prompt.push_str(&format!(
                    "Player: \"{trigger} the {entity_id}\" → <narration>\\nMCL::action({name}, target={entity_id})\n"
                ));
            }
        } else {
            prompt.push_str(&format!(
                "Player: \"{trigger}\" → <narration>\\nMCL::action({name})\n"
            ));
        }
    }

    // Dialogue example
    if let Some((entity_id, _)) = doc.entities.iter().next() {
        prompt.push_str(&format!(
            "Player: \"talk to the {entity_id}\" → <narration>\\nMCL::dialogue({entity_id}, \"<what they say>\")\n"
        ));
    }

    // Transition example
    for t in &doc.transitions {
        if let Some(ref dir) = t.direction {
            prompt.push_str(&format!(
                "Player: \"go {dir}\" → <narration>\\nMCL::transition({})\n",
                t.target
            ));
        }
    }

    // Noop example
    prompt.push_str("Player: \"I sit down and think\" → <narration>\\nMCL::noop()\n\n");

    prompt.push_str(&build_rules_section(doc));

    prompt
}

/// Build the system prompt with optional registry context injection.
///
/// Extends [`build_system_prompt`] with session history and summaries
/// from the registry, when available and allowed by the retrieval policy.
/// Pass `None` for `registry_ctx` to get the same result as [`build_system_prompt`].
pub fn build_system_prompt_with_registry(
    doc: &MclDocument,
    state: &WorldState,
    registry_ctx: Option<&RegistryContext>,
) -> String {
    let mut prompt = build_system_prompt(doc, state);

    if let Some(ctx) = registry_ctx {
        if let Some(section) = build_registry_context_section(ctx, prompt.len()) {
            prompt.push_str(&section);
        }
    }

    prompt
}

pub fn build_structured_system_prompt(doc: &MclDocument, state: &WorldState) -> String {
    let mut prompt = build_prompt_preamble(doc, state);

    prompt.push_str("## Response Format\n");
    prompt.push_str(
        "You MUST respond with valid JSON in this exact format:\n\
         ```json\n\
         {\n\
           \"narration\": \"Your narrative text goes here...\",\n\
           \"signals\": [\n\
             {\"type\": \"action\", \"name\": \"<action_name>\", \"args\": {\"target\": \"<entity_id>\"}},\n\
             {\"type\": \"noop\"}\n\
           ]\n\
         }\n\
         ```\n\n",
    );

    prompt.push_str("Available signal types:\n");
    for (name, action) in &doc.actions {
        if action.target.is_some() {
            prompt.push_str(&format!(
                "- {{\"type\": \"action\", \"name\": \"{name}\", \"args\": {{\"target\": \"<entity_id>\"}}}}\n"
            ));
        } else {
            prompt.push_str(&format!(
                "- {{\"type\": \"action\", \"name\": \"{name}\"}}\n"
            ));
        }
    }
    prompt.push_str(
        "- {\"type\": \"dialogue\", \"entity\": \"<entity_id>\", \"text\": \"<speech>\"}\n",
    );
    prompt.push_str("- {\"type\": \"transition\", \"context\": \"<context_id>\"}\n");
    prompt.push_str("- {\"type\": \"mutate\", \"path\": \"<field.path>\", \"value\": <value>}\n");
    prompt.push_str("- {\"type\": \"noop\"}\n\n");

    if !doc.transitions.is_empty() {
        prompt.push_str("Available transitions:\n");
        for t in &doc.transitions {
            if let Some(ref dir) = t.direction {
                prompt.push_str(&format!(
                    "- {{\"type\": \"transition\", \"context\": \"{}\"}} — {dir}\n",
                    t.target
                ));
            } else {
                prompt.push_str(&format!(
                    "- {{\"type\": \"transition\", \"context\": \"{}\"}}\n",
                    t.target
                ));
            }
        }
        prompt.push('\n');
    }

    prompt.push_str("## JSON Example\n```json\n");
    let mut example_count = 0usize;
    for (name, action) in &doc.actions {
        if example_count >= 2 {
            break;
        }
        if action.target.is_some() {
            if let Some((entity_id, _)) = doc.entities.iter().next() {
                prompt.push_str(&format!(
                    "{{\n  \"narration\": \"You strike at the {entity_id}!\",\n  \
                     \"signals\": [\n    {{\"type\": \"action\", \"name\": \"{name}\", \"args\": {{\"target\": \"{entity_id}\"}}}}\n  ]\n}}\n"
                ));
                example_count += 1;
            }
        } else {
            prompt.push_str(&format!(
                "{{\n  \"narration\": \"You look around carefully.\",\n  \
                 \"signals\": [\n    {{\"type\": \"action\", \"name\": \"{name}\"}}\n  ]\n}}\n"
            ));
            example_count += 1;
        }
    }

    if example_count == 0 {
        prompt.push_str(
            "{\n  \"narration\": \"Nothing happens.\",\n  \
             \"signals\": [{\"type\": \"noop\"}]\n}\n",
        );
    }

    prompt.push_str("```\n\n");

    prompt.push_str("- Your ENTIRE response must be valid JSON — no text before or after\n");
    prompt.push_str(&build_rules_section(doc));

    prompt
}

/// Get list of actions whose preconditions are met
fn get_valid_actions(doc: &MclDocument, state: &WorldState) -> Vec<(String, Option<String>)> {
    let mut valid = Vec::new();

    for (name, action) in &doc.actions {
        // Check requires (all must be true)
        let requires_met = action.requires.iter().all(|c| state.evaluate(c));
        // Check requires_any (at least one must be true, or empty)
        let requires_any_met =
            action.requires_any.is_empty() || action.requires_any.iter().any(|c| state.evaluate(c));

        if requires_met && requires_any_met {
            valid.push((name.clone(), action.description.clone()));
        }
    }

    valid
}

// ---------------------------------------------------------------------------
// Registry context integration (summaries + session history)
// ---------------------------------------------------------------------------

/// Policy for when to include registry context in prompts.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SummaryRetrievalPolicy {
    /// Always include registry context in prompts.
    Always,
    /// Only include when the current prompt exceeds this character count.
    WhenOverBudget(usize),
    /// Never include registry context.
    #[default]
    Never,
}

/// Bundles registry data for prompt building.
///
/// Populated from `RegistryHandle::read()` and `SummarizerNode::summaries()`.
/// Passed to [`build_system_prompt_with_registry`] to inject session history
/// and summary context into the LLM prompt.
pub struct RegistryContext {
    /// Recent signal log entries from the registry.
    pub recent_entries: Vec<SignalLogEntry>,
    /// Summaries from the SummarizerNode.
    pub summaries: Vec<SummaryEntry>,
    /// Policy controlling when to include this data.
    pub policy: SummaryRetrievalPolicy,
}

fn format_signal_brief(signal: &mcl_spec::signal::MclSignal) -> String {
    use mcl_spec::signal::SignalKind;
    match &signal.kind {
        SignalKind::Action { name, .. } => format!("action({name})"),
        SignalKind::Dialogue { entity, .. } => format!("dialogue({entity})"),
        SignalKind::Transition { context } => format!("transition({context})"),
        SignalKind::Mutate { path, .. } => format!("mutate({path})"),
        SignalKind::Noop => "noop".to_string(),
        SignalKind::Error { message } => format!("error({message})"),
        SignalKind::Narrate { tag } => format!("narrate({tag})"),
        SignalKind::Query { path } => format!("query({path})"),
    }
}

/// Build an optional section with registry context (session history + summaries).
///
/// `budget_chars` is the current prompt size. For `WhenOverBudget(threshold)`,
/// the section is only produced when `budget_chars > threshold`. Content is
/// trimmed (oldest entries first) to fit within `budget_chars`.
pub fn build_registry_context_section(
    ctx: &RegistryContext,
    budget_chars: usize,
) -> Option<String> {
    match &ctx.policy {
        SummaryRetrievalPolicy::Never => return None,
        SummaryRetrievalPolicy::WhenOverBudget(threshold) => {
            if budget_chars <= *threshold {
                return None;
            }
        }
        SummaryRetrievalPolicy::Always => {}
    }

    if ctx.recent_entries.is_empty() && ctx.summaries.is_empty() {
        return None;
    }

    let mut section = String::new();

    // Session History — chronological, trimming oldest entries to fit
    if !ctx.recent_entries.is_empty() {
        section.push_str("## Session History\n");

        let mut lines: Vec<String> = ctx
            .recent_entries
            .iter()
            .map(|e| {
                format!(
                    "- Turn {}: {} [{}]\n",
                    e.turn,
                    format_signal_brief(&e.signal),
                    e.channel
                )
            })
            .collect();

        // Trim from front (oldest) until remaining fit within budget
        while section.len() + lines.iter().map(|l| l.len()).sum::<usize>() > budget_chars
            && lines.len() > 1
        {
            lines.remove(0);
        }

        for line in &lines {
            section.push_str(line);
        }
        section.push('\n');
    }

    // Summary — most recent summary that fits
    if !ctx.summaries.is_empty() {
        if let Some(summary) = ctx.summaries.last() {
            let summary_block = format!("## Summary\n{}\n\n", summary.text);
            if section.len() + summary_block.len() <= budget_chars {
                section.push_str(&summary_block);
            }
        }
    }

    Some(section)
}

/// Format resolution results for the LLM to narrate
pub fn format_resolution_feedback(resolutions: &[mcl_spec::Resolution]) -> String {
    let mut feedback = String::new();

    for res in resolutions {
        feedback.push_str(&format!("[Resolution: {} — {}", res.action, res.outcome));
        for (key, val) in &res.details {
            feedback.push_str(&format!(", {key}={val}"));
        }
        feedback.push_str("]\n");

        if let Some(ref hint) = res.narration_hint {
            feedback.push_str(&format!("[Narrate: {hint}]\n"));
        }
    }

    feedback
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_graph::id::NodeId;
    use mcl_registry::{SignalId, SummaryEntry, SummaryLevel, Verdict};
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::{MclSignal, SignalKind};
    use mcl_spec::span::Span;
    use mcl_spec::value::MclValue;

    fn antechamber() -> (MclDocument, WorldState) {
        let src = include_str!("../../../examples/crypt/crypt_antechamber.mcl");
        let doc = mcl_parse::parse_document(src).unwrap();
        let state = WorldState::new();
        (doc, state)
    }

    #[test]
    fn system_prompt_includes_knowledge_boundaries() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("You know about: guards_tomb"),
            "prompt should list known facts — got:\n{prompt}"
        );
        assert!(
            !prompt.contains("You do NOT know about:"),
            "prompt should not leak unknown fact names — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_includes_narrator_directives() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("## Tone"),
            "prompt should have a Tone heading — got:\n{prompt}"
        );
        assert!(
            prompt.contains("dread"),
            "prompt should mention the dread tone — got:\n{prompt}"
        );
        assert!(
            prompt.contains("second_person"),
            "prompt should reference second_person voice — got:\n{prompt}"
        );
        assert!(
            prompt.contains("present"),
            "prompt should reference present tense — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_includes_signal_protocol() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("## MCL Signal Protocol"),
            "prompt should have Signal Protocol heading — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::action(look)"),
            "prompt should show action signal format — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::action(attack, target=<id>)"),
            "prompt should show targeted action signal — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::noop()"),
            "prompt should show noop signal — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::dialogue("),
            "prompt should show dialogue signal — got:\n{prompt}"
        );
        assert!(
            prompt.contains("## Signal Selection Rules"),
            "prompt should have disambiguation rules — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_includes_forbidden_words() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("epic"),
            "prompt should list forbidden word 'epic' — got:\n{prompt}"
        );
        assert!(
            prompt.contains("awesome"),
            "prompt should list forbidden word 'awesome' — got:\n{prompt}"
        );
        assert!(
            prompt.contains("incredible"),
            "prompt should list forbidden word 'incredible' — got:\n{prompt}"
        );
        assert!(
            prompt.contains("Never use these words"),
            "prompt should have the forbidden words label — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_shows_valid_actions() {
        let (doc, mut state) = antechamber();

        let prompt_empty = build_system_prompt(&doc, &state);
        let empty_actions = prompt_empty
            .split("## Available Actions")
            .nth(1)
            .and_then(|s| s.split("## ").next())
            .unwrap_or("");
        assert!(
            !empty_actions.contains("light_torch"),
            "light_torch should not be available without flint — got:\n{prompt_empty}"
        );

        let mut inv = indexmap::IndexMap::new();
        inv.insert("flint".to_string(), MclValue::Bool(true));
        state
            .local
            .insert("inventory".to_string(), MclValue::Map(inv));
        let prompt_with_flint = build_system_prompt(&doc, &state);

        let flint_actions = prompt_with_flint
            .split("## Available Actions")
            .nth(1)
            .and_then(|s| s.split("## ").next())
            .unwrap_or("");

        assert!(
            prompt_with_flint.contains("## Available Actions"),
            "prompt should have Available Actions heading — got:\n{prompt_with_flint}"
        );
        assert!(
            prompt_with_flint.contains("light_torch"),
            "light_torch should appear when flint is in inventory — got:\n{prompt_with_flint}"
        );
        assert!(
            flint_actions.contains("light_torch"),
            "light_torch should appear in Available Actions when flint is present — got:\n{flint_actions}"
        );
        assert!(
            flint_actions.contains("look"),
            "look should always be available — got:\n{flint_actions}"
        );
    }

    #[test]
    fn system_prompt_excludes_unknown_knowledge_from_state() {
        let (doc, mut state) = antechamber();
        state.set("secret_passage", MclValue::Bool(true)).unwrap();
        let prompt = build_system_prompt(&doc, &state);
        assert!(
            !prompt.contains("secret_passage"),
            "prompt should NOT contain secret_passage — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_includes_known_knowledge_in_state() {
        let (doc, mut state) = antechamber();
        state.set("guards_tomb", MclValue::Bool(true)).unwrap();
        let prompt = build_system_prompt(&doc, &state);
        assert!(
            prompt.contains("guards_tomb"),
            "prompt should contain guards_tomb — got:\n{prompt}"
        );
    }

    #[test]
    fn system_prompt_shows_entity_state() {
        let (doc, mut state) = antechamber();

        state.load_entities(&doc.entities);
        state
            .set("entities.skeleton_guard.hp", MclValue::Int(7))
            .unwrap();

        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("## Entities Present"),
            "prompt should have Entities Present heading — got:\n{prompt}"
        );
        assert!(
            prompt.contains("skeleton_guard"),
            "prompt should list skeleton_guard — got:\n{prompt}"
        );
        assert!(
            prompt.contains("enemy"),
            "prompt should show entity type 'enemy' — got:\n{prompt}"
        );
        assert!(
            prompt.contains("hostile"),
            "prompt should show disposition 'hostile' — got:\n{prompt}"
        );
        assert!(
            prompt.contains("hp=7"),
            "prompt should show entity hp from state — got:\n{prompt}"
        );
        assert!(
            prompt.contains("knows about: guards_tomb"),
            "prompt should show entity knowledge boundaries — got:\n{prompt}"
        );
    }

    #[test]
    fn few_shot_examples_in_system_prompt() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("## Examples"),
            "prompt should have an Examples section — got:\n{prompt}"
        );

        assert!(
            prompt.contains("MCL::action(look)"),
            "prompt should show a look action example — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::action(attack, target=skeleton_guard)"),
            "prompt should show attack with target in few-shot examples — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::dialogue(skeleton_guard,"),
            "prompt should show dialogue example with entity — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::transition(crypt_inner)"),
            "prompt should show transition example — got:\n{prompt}"
        );
        assert!(
            prompt.contains("MCL::noop()"),
            "prompt should show noop example — got:\n{prompt}"
        );

        let examples_section = prompt
            .split("## Examples")
            .nth(1)
            .and_then(|s| s.split("## ").next())
            .unwrap_or("");
        assert!(
            !examples_section.is_empty(),
            "Examples section should have content"
        );

        let example_count = examples_section.matches("Player:").count();
        assert!(
            example_count >= 4,
            "should have at least 4 few-shot examples (actions + dialogue + transition + noop) — got {example_count}"
        );
    }

    #[test]
    fn few_shot_examples_include_trigger_phrases() {
        let (doc, state) = antechamber();
        let prompt = build_system_prompt(&doc, &state);

        assert!(
            prompt.contains("Player:"),
            "few-shot examples should use Player: prefix format — got:\n{prompt}"
        );

        let examples_section = prompt
            .split("## Examples")
            .nth(1)
            .and_then(|s| s.split("## ").next())
            .unwrap_or("");
        assert!(
            examples_section.contains("→"),
            "few-shot examples should use → separator — got:\n{examples_section}"
        );
        assert!(
            examples_section.contains("\\nMCL::"),
            "few-shot examples should show signal on own line via \\nMCL:: — got:\n{examples_section}"
        );
    }

    // --- Registry context tests ---

    fn make_log_entry(turn: u64) -> SignalLogEntry {
        let origin = NodeId::new("player");
        let signal = MclSignal {
            kind: SignalKind::Action {
                name: format!("action_{turn}"),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        };
        SignalLogEntry {
            id: SignalId::from_signal(&origin, turn, "actions", &signal),
            signal,
            origin,
            turn,
            channel: "actions".to_string(),
            timestamp: turn * 1000,
            verdict: Verdict::Accept,
            policy_bindings: None,
        }
    }

    #[test]
    fn registry_context_empty_returns_none() {
        let ctx = RegistryContext {
            recent_entries: vec![],
            summaries: vec![],
            policy: SummaryRetrievalPolicy::Always,
        };
        assert!(
            build_registry_context_section(&ctx, 1000).is_none(),
            "empty context should return None"
        );
    }

    #[test]
    fn registry_context_policy_never_returns_none() {
        let ctx = RegistryContext {
            recent_entries: vec![make_log_entry(1)],
            summaries: vec![],
            policy: SummaryRetrievalPolicy::Never,
        };
        assert!(
            build_registry_context_section(&ctx, 10000).is_none(),
            "Never policy should always return None"
        );
    }

    #[test]
    fn registry_context_always_with_summaries() {
        let ctx = RegistryContext {
            recent_entries: vec![],
            summaries: vec![SummaryEntry {
                level: SummaryLevel::Turn,
                turn_start: 1,
                turn_end: 3,
                text: "Player explored the antechamber".to_string(),
                char_count: 31,
            }],
            policy: SummaryRetrievalPolicy::Always,
        };
        let section = build_registry_context_section(&ctx, 1000)
            .expect("Always policy with summaries should return Some");
        assert!(
            section.contains("## Summary"),
            "should contain Summary heading — got:\n{section}"
        );
        assert!(
            section.contains("Player explored the antechamber"),
            "should contain summary text — got:\n{section}"
        );
    }

    #[test]
    fn registry_context_over_budget_below_threshold() {
        let ctx = RegistryContext {
            recent_entries: vec![make_log_entry(1)],
            summaries: vec![],
            policy: SummaryRetrievalPolicy::WhenOverBudget(1000),
        };
        assert!(
            build_registry_context_section(&ctx, 500).is_none(),
            "below threshold should return None"
        );
    }

    #[test]
    fn registry_context_over_budget_trims_older_entries() {
        let entries: Vec<SignalLogEntry> = (1..=5).map(make_log_entry).collect();
        let ctx = RegistryContext {
            recent_entries: entries,
            summaries: vec![],
            policy: SummaryRetrievalPolicy::WhenOverBudget(50),
        };
        let section =
            build_registry_context_section(&ctx, 80).expect("over budget should return Some");
        assert!(
            section.contains("## Session History"),
            "should contain Session History heading — got:\n{section}"
        );
        let turn_count = section.matches("Turn").count();
        assert!(
            turn_count < 5,
            "should have trimmed some entries, got {turn_count}"
        );
        assert!(
            turn_count >= 1,
            "should have at least 1 entry, got {turn_count}"
        );
    }

    #[test]
    fn build_system_prompt_with_registry_adds_context() {
        let (doc, state) = antechamber();
        let ctx = RegistryContext {
            recent_entries: vec![make_log_entry(1)],
            summaries: vec![],
            policy: SummaryRetrievalPolicy::Always,
        };
        let with_registry = build_system_prompt_with_registry(&doc, &state, Some(&ctx));
        let without_registry = build_system_prompt(&doc, &state);

        assert!(
            with_registry.len() > without_registry.len(),
            "registry version should be longer"
        );
        assert!(
            with_registry.contains("## Session History"),
            "should contain Session History heading — got:\n{with_registry}"
        );
    }
}
