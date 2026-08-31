//! WASM bindings: run MCL in browsers.
//!
//! Compiles MCL core functionality to WebAssembly for JavaScript use.

use wasm_bindgen::prelude::*;

/// Parse an MCL document and return it as JSON.
#[wasm_bindgen]
pub fn parse(source: &str) -> Result<String, JsValue> {
    let doc = mcl_parse::parse_document(source)
        .map_err(|e| JsValue::from_str(&format!("Parse error: {e}")))?;
    serde_json::to_string_pretty(&doc).map_err(|e| JsValue::from_str(&format!("JSON error: {e}")))
}

/// Build a system prompt from an MCL document source.
#[wasm_bindgen]
pub fn build_prompt(source: &str) -> Result<String, JsValue> {
    let doc = mcl_parse::parse_document(source)
        .map_err(|e| JsValue::from_str(&format!("Parse error: {e}")))?;
    Ok(build_system_prompt_wasm(&doc))
}

/// Extract MCL signals from LLM output text, returned as JSON.
#[wasm_bindgen]
pub fn extract_signals(text: &str) -> Result<String, JsValue> {
    let parsed = mcl_signal::extract_signals(text);
    serde_json::to_string_pretty(&parsed)
        .map_err(|e| JsValue::from_str(&format!("JSON error: {e}")))
}

/// Extract MCL signals from structured JSON LLM output, returned as JSON.
#[wasm_bindgen]
pub fn extract_signals_json(text: &str) -> Result<String, JsValue> {
    let parsed = mcl_signal::extract_signals_from_json(text);
    serde_json::to_string_pretty(&parsed)
        .map_err(|e| JsValue::from_str(&format!("JSON error: {e}")))
}

/// Validate an MCL document and return a JSON report.
#[wasm_bindgen]
pub fn validate(source: &str) -> Result<String, JsValue> {
    match mcl_parse::parse_document(source) {
        Ok(doc) => {
            let info = serde_json::json!({
                "valid": true,
                "context": doc.frontmatter.context,
                "type": doc.frontmatter.context_type,
                "actions": doc.actions.keys().collect::<Vec<_>>(),
                "entities": doc.entities.keys().collect::<Vec<_>>(),
                "items": doc.items.keys().collect::<Vec<_>>(),
                "transitions": doc.transitions.iter().map(|t| &t.target).collect::<Vec<_>>(),
            });
            Ok(info.to_string())
        }
        Err(e) => {
            let info = serde_json::json!({
                "valid": false,
                "error": format!("{e}"),
            });
            Ok(info.to_string())
        }
    }
}

/// Classify an LLM response turn against expected signal output, returned as JSON.
/// `available_actions_json` is a JSON array of action name strings.
#[wasm_bindgen]
pub fn classify_turn(
    raw_response: &str,
    expected_type: &str,
    expected_name: &str,
    available_actions_json: &str,
) -> Result<String, JsValue> {
    let actions: Vec<String> = serde_json::from_str(available_actions_json)
        .map_err(|e| JsValue::from_str(&format!("JSON error parsing actions: {e}")))?;
    let action_refs: Vec<&str> = actions.iter().map(|s| s.as_str()).collect();

    let exp_type = if expected_type.is_empty() {
        None
    } else {
        Some(expected_type)
    };
    let exp_name = if expected_name.is_empty() {
        None
    } else {
        Some(expected_name)
    };

    let result = mcl_signal::classify_turn(raw_response, exp_type, exp_name, &action_refs);
    Ok(result.to_string())
}

// ── Inline prompt builder (avoids mcl-runtime dependency) ──

fn build_system_prompt_wasm(doc: &mcl_spec::document::MclDocument) -> String {
    use mcl_spec::document::KnowledgeEntry;
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
    if let Some(ref style_notes) = doc.narrator.style_notes {
        prompt.push_str(&format!("## Style Notes\n{style_notes}\n\n"));
    }

    // Entities
    if !doc.entities.is_empty() {
        prompt.push_str("## Entities Present\n");
        for (id, entity) in &doc.entities {
            let t = entity.entity_type.as_deref().unwrap_or("unknown");
            let d = entity.disposition.as_deref().unwrap_or("neutral");
            prompt.push_str(&format!("- **{id}** ({t}, {d})\n"));
        }
        prompt.push('\n');
    }

    // Items
    if !doc.items.is_empty() {
        prompt.push_str("## Items\n");
        for (id, _item) in &doc.items {
            prompt.push_str(&format!("- {id}\n"));
        }
        prompt.push('\n');
    }

    // State
    if !doc.state.is_empty() {
        prompt.push_str("## Current State\n");
        for (key, val) in &doc.state {
            prompt.push_str(&format!("- {key}: {val}\n"));
        }
        prompt.push('\n');
    }

    // Signal catalog
    prompt.push_str(
        "## MCL Signal Protocol\n\
         EVERY response MUST end with exactly ONE MCL signal on its own line.\n\n",
    );

    for (name, action) in &doc.actions {
        if action.target.is_some() {
            prompt.push_str(&format!("MCL::action({name}, target=<id>)"));
        } else {
            prompt.push_str(&format!("MCL::action({name})"));
        }
        if let Some(ref desc) = action.description {
            prompt.push_str(&format!(" -- {desc}"));
        }
        prompt.push('\n');
    }
    prompt.push_str("MCL::dialogue(<entity_id>, \"<speech>\") -- Player speaks to someone\n");
    prompt.push_str("MCL::transition(<context_id>) -- Move to a connected context\n");
    prompt.push_str("MCL::noop() -- No game action\n\n");

    // Transitions
    if !doc.transitions.is_empty() {
        prompt.push_str("## Available Transitions\n");
        for t in &doc.transitions {
            prompt.push_str(&format!("- **{}**", t.target));
            if let Some(ref dir) = t.direction {
                prompt.push_str(&format!(" ({dir})"));
            }
            if let Some(ref when) = t.when {
                prompt.push_str(&format!(" when {when}"));
            }
            prompt.push('\n');
        }
        prompt.push('\n');
    }

    // Constraints
    if !doc.constraints.is_empty() {
        prompt.push_str("## Constraints\n");
        for c in &doc.constraints {
            prompt.push_str(&format!("- {c}\n"));
        }
        prompt.push('\n');
    }

    // Rules
    prompt.push_str("## Rules\n");
    prompt.push_str(&format!("- Narrate in {voice} {tense} tense\n"));
    if !doc.narrator.forbidden.is_empty() {
        prompt.push_str(&format!(
            "- Never use: {}\n",
            doc.narrator.forbidden.join(", ")
        ));
    }
    for entry in &doc.knowledge {
        match entry {
            KnowledgeEntry::Known(fact) => {
                prompt.push_str(&format!("- You know about: {fact}\n"));
            }
            KnowledgeEntry::Unknown(_) => {
                // Filtered out -- don't tell the LLM about forbidden knowledge
            }
        }
    }

    prompt
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_DOC: &str = r#"---
context: crypt_antechamber
type: room
domain: rpg
---

# State
lit: false
door_north: locked

# Entities
- skeleton_guard:
    type: enemy
    disposition: hostile

# Actions
- look:
    description: "Look around"
- attack:
    target: enum(entities)
    description: "Attack an entity"

# Transitions
- crypt_inner:
    when: door_north == unlocked
    direction: north

# Narrator
tone: dread
voice: second_person
tense: present
context: "Ancient crypt, smell of dust and decay."
forbidden: [awesome, epic]

# Knowledge
- guards_tomb
- !secret_passage
"#;

    #[test]
    fn parse_returns_valid_json() {
        let result = parse(SAMPLE_DOC).unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(
            val["frontmatter"]["context"].as_str().unwrap(),
            "crypt_antechamber"
        );
    }

    #[test]
    fn validate_valid_doc() {
        let result = validate(SAMPLE_DOC).unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["valid"], true);
        assert_eq!(val["context"], "crypt_antechamber");
    }

    #[test]
    fn validate_invalid_doc() {
        let result = validate("not a valid mcl document").unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["valid"], false);
        assert!(!val["error"].as_str().unwrap().is_empty());
    }

    #[test]
    fn build_prompt_contains_key_sections() {
        let result = build_prompt(SAMPLE_DOC).unwrap();
        assert!(result.contains("second_person"));
        assert!(result.contains("present tense"));
        assert!(result.contains("skeleton_guard"));
        assert!(result.contains("MCL::action(look)"));
        assert!(result.contains("MCL::action(attack, target=<id>)"));
        assert!(result.contains("MCL::noop()"));
        assert!(result.contains("guards_tomb"));
        assert!(!result.contains("secret_passage"));
        assert!(result.contains("awesome, epic"));
    }

    #[test]
    fn extract_signals_returns_valid_json() {
        let text = "You swing your sword!\nMCL::action(attack, target=skeleton_guard)";
        let result = extract_signals(text).unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["signals"].as_array().unwrap().len(), 1);
        assert_eq!(val["narration"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn extract_signals_json_works() {
        let json_input = r#"{"narration":"You wait.","signals":[{"type":"noop"}]}"#;
        let result = extract_signals_json(json_input).unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["signals"].as_array().unwrap().len(), 1);
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn parse_garbage_returns_err() {
        assert!(parse("$$$ not mcl $$$").is_err());
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn parse_empty_returns_err() {
        assert!(parse("").is_err());
    }

    #[test]
    fn validate_garbage_returns_valid_false() {
        let result = validate("not valid").unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["valid"], false);
    }

    #[test]
    fn extract_signals_empty_text_returns_empty_json() {
        let result = extract_signals("").unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["signals"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn extract_signals_json_invalid_returns_empty() {
        let result = extract_signals_json("not json").unwrap();
        let val: serde_json::Value = serde_json::from_str(&result).unwrap();
        assert_eq!(val["signals"].as_array().unwrap().len(), 0);
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn build_prompt_garbage_returns_err() {
        assert!(build_prompt("$$$").is_err());
    }

    #[test]
    #[cfg(target_arch = "wasm32")]
    fn classify_turn_invalid_actions_json_returns_err() {
        assert!(classify_turn("hello", "", "", "not json array").is_err());
    }
}
