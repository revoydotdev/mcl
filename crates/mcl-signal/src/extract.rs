use std::collections::BTreeMap;

use indexmap::IndexMap;
use mcl_spec::label::{Label, LabelParam, LabelSet};
use mcl_spec::signal::{MclSignal, ParsedResponse, SignalKind};
use mcl_spec::source::Source;
use mcl_spec::span::Span;
use mcl_spec::value::MclValue;

use crate::parse::parse_signal;

/// Extract MCL signals from mixed LLM output text.
/// Lines starting with `MCL::` (after optional whitespace) are signal candidates.
/// Everything else is narration.
pub fn extract_signals(text: &str) -> ParsedResponse {
    let mut response = ParsedResponse::default();

    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim();
        let line_num = (line_idx + 1) as u32;

        if trimmed.starts_with("MCL::") {
            match parse_signal(trimmed, line_num) {
                Ok(signal) => response.signals.push(signal),
                Err(e) => {
                    response.warnings.push(format!("line {line_num}: {e}"));
                    // Treat malformed signal as narration
                    response.narration.push(line.to_string());
                }
            }
        } else if !trimmed.is_empty() {
            response.narration.push(line.to_string());
        }
    }

    response
}

/// Extract MCL signals from mixed LLM output text and tag them with source labels.
///
/// Wraps [`extract_signals`] and then:
/// 1. Inherits all labels from `source.inherit_labels` onto every signal.
/// 2. Adds a synthetic `@source(id=…, trust=…)` label to every signal.
///
/// Does **not** modify the behaviour of [`extract_signals`].
pub fn extract_signals_from(text: &str, source: &Source) -> ParsedResponse {
    let mut response = extract_signals(text);
    for signal in &mut response.signals {
        for label_name in &source.inherit_labels {
            let label = Label {
                name: label_name.clone(),
                params: BTreeMap::new(),
            };
            signal.labels.insert(label);
        }

        let source_label = Label {
            name: "source".into(),
            params: {
                let mut p = BTreeMap::new();
                p.insert("id".into(), LabelParam::Str(source.id.as_str().into()));
                p.insert(
                    "trust".into(),
                    LabelParam::Ident(format!("{:?}", source.trust).to_lowercase()),
                );
                p
            },
        };
        signal.labels.insert(source_label);
    }
    response
}

/// Check if a line looks like it might be an MCL signal (for fuzzy detection)
pub fn looks_like_signal(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.starts_with("MCL::") || trimmed.starts_with("mcl::")
}

pub fn extract_signals_from_json(text: &str) -> ParsedResponse {
    let mut response = ParsedResponse::default();

    let Ok(parsed) = serde_json::from_str::<serde_json::Value>(text) else {
        if !text.trim().is_empty() {
            response.narration.push(text.to_string());
        }
        return response;
    };

    let narration = parsed
        .get("narration")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if !narration.is_empty() {
        response.narration.push(narration.to_string());
    }

    let Some(signals_arr) = parsed.get("signals").and_then(|v| v.as_array()) else {
        return response;
    };

    let synthetic_span = Span::new(0, 0, 1, 1);

    for signal_val in signals_arr {
        let Some(sig_type) = signal_val.get("type").and_then(|v| v.as_str()) else {
            response
                .warnings
                .push("signal object missing \"type\" field".to_string());
            continue;
        };

        let kind = match sig_type {
            "action" => {
                let Some(name) = signal_val.get("name").and_then(|v| v.as_str()) else {
                    response.warnings.push(format!(
                        "action signal missing \"name\" field: {signal_val}"
                    ));
                    continue;
                };
                let args = signal_val
                    .get("args")
                    .and_then(convert_json_value_to_args)
                    .unwrap_or_default();
                SignalKind::Action {
                    name: name.to_string(),
                    args,
                }
            }
            "dialogue" => {
                let Some(entity) = signal_val.get("entity").and_then(|v| v.as_str()) else {
                    response
                        .warnings
                        .push("dialogue signal missing \"entity\" field".to_string());
                    continue;
                };
                let Some(text) = signal_val.get("text").and_then(|v| v.as_str()) else {
                    response
                        .warnings
                        .push("dialogue signal missing \"text\" field".to_string());
                    continue;
                };
                SignalKind::Dialogue {
                    entity: entity.to_string(),
                    text: text.to_string(),
                }
            }
            "transition" => {
                let Some(context) = signal_val.get("context").and_then(|v| v.as_str()) else {
                    response
                        .warnings
                        .push("transition signal missing \"context\" field".to_string());
                    continue;
                };
                SignalKind::Transition {
                    context: context.to_string(),
                }
            }
            "mutate" => {
                let Some(path) = signal_val.get("path").and_then(|v| v.as_str()) else {
                    response
                        .warnings
                        .push("mutate signal missing \"path\" field".to_string());
                    continue;
                };
                let Some(v) = signal_val.get("value") else {
                    response
                        .warnings
                        .push("mutate signal missing \"value\" field".to_string());
                    continue;
                };
                let value = if let Ok(mv) = serde_json::from_value::<MclValue>(v.clone()) {
                    mv
                } else {
                    response
                        .warnings
                        .push(format!("mutate signal has invalid \"value\" field: {v}"));
                    continue;
                };
                SignalKind::Mutate {
                    path: path.to_string(),
                    value,
                }
            }
            "noop" => SignalKind::Noop,
            other => {
                response
                    .warnings
                    .push(format!("unknown signal type: \"{other}\""));
                continue;
            }
        };

        response.signals.push(MclSignal {
            kind,
            span: synthetic_span,
            labels: LabelSet::default(),
        });
    }

    response
}

fn convert_json_value_to_args(val: &serde_json::Value) -> Option<IndexMap<String, MclValue>> {
    let obj = val.as_object()?;
    let mut args = IndexMap::new();
    for (k, v) in obj {
        if let Ok(mv) = serde_json::from_value::<MclValue>(v.clone()) {
            args.insert(k.clone(), mv);
        }
    }
    Some(args)
}

/// Incremental update emitted by [`IncrementalExtractor`].
#[derive(Debug, Default, Clone, PartialEq)]
pub struct IncrementalUpdate {
    /// Narration lines completed in this update.
    pub narration: Vec<String>,
    /// Signals successfully extracted from completed lines.
    pub signals: Vec<MclSignal>,
    /// Warnings for malformed signal lines.
    pub warnings: Vec<String>,
}

/// Incremental signal extractor for streaming LLM output.
///
/// Feed text chunks as they arrive from the LLM. Completed lines are
/// processed immediately; incomplete trailing text is buffered until the
/// next newline arrives. Call [`flush`](Self::flush) at end-of-stream
/// to process any remaining buffered text.
///
/// Produces the same result as [`extract_signals`] for the same input.
#[derive(Debug, Default)]
pub struct IncrementalExtractor {
    buffer: String,
    line_count: u32,
}

impl IncrementalExtractor {
    /// Create a new incremental extractor.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed a chunk of streamed text.
    ///
    /// Any complete lines (delimited by `\n`) are extracted immediately.
    /// Partial trailing text is buffered for the next call.
    pub fn feed(&mut self, chunk: &str) -> IncrementalUpdate {
        self.buffer.push_str(chunk);
        self.drain_completed_lines()
    }

    /// Flush any remaining buffered text.
    ///
    /// Call this when the stream ends to process the final (possibly
    /// incomplete) line.
    pub fn flush(&mut self) -> IncrementalUpdate {
        if self.buffer.is_empty() {
            return IncrementalUpdate::default();
        }
        let remaining = std::mem::take(&mut self.buffer);
        self.process_line(&remaining)
    }

    fn drain_completed_lines(&mut self) -> IncrementalUpdate {
        let mut update = IncrementalUpdate::default();

        while let Some(newline_pos) = self.buffer.find('\n') {
            let line = self.buffer[..newline_pos].to_string();
            self.buffer = self.buffer[newline_pos + 1..].to_string();
            let line_update = self.process_line(&line);
            update.narration.extend(line_update.narration);
            update.signals.extend(line_update.signals);
            update.warnings.extend(line_update.warnings);
        }

        update
    }

    fn process_line(&mut self, line: &str) -> IncrementalUpdate {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return IncrementalUpdate::default();
        }

        self.line_count += 1;
        let line_num = self.line_count;

        if trimmed.starts_with("MCL::") {
            match parse_signal(trimmed, line_num) {
                Ok(signal) => IncrementalUpdate {
                    signals: vec![signal],
                    ..Default::default()
                },
                Err(e) => IncrementalUpdate {
                    warnings: vec![format!("line {line_num}: {e}")],
                    narration: vec![line.to_string()],
                    signals: vec![],
                },
            }
        } else {
            IncrementalUpdate {
                narration: vec![line.to_string()],
                ..Default::default()
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::label::LabelParam;
    use mcl_spec::signal::SignalKind;
    use mcl_spec::source::{Source, SourceId, Trust};

    #[test]
    fn extract_mixed_content() {
        let text = r#"The skeleton's jaw creaks open, empty eye sockets fixing on you.

MCL::dialogue(skeleton_guard, "Who dares disturb my eternal rest?")

You grip your sword tighter, the chill of the crypt seeping through your armor.

MCL::action(attack, target=skeleton_guard)"#;

        let result = extract_signals(text);
        assert_eq!(result.signals.len(), 2);
        assert_eq!(result.narration.len(), 2);
        assert!(result.warnings.is_empty());

        assert!(
            matches!(&result.signals[0].kind, SignalKind::Dialogue { entity, .. } if entity == "skeleton_guard")
        );
        assert!(
            matches!(&result.signals[1].kind, SignalKind::Action { name, .. } if name == "attack")
        );
    }

    #[test]
    fn extract_no_signals() {
        let text = "Just some regular narration text.\nWith multiple lines.";
        let result = extract_signals(text);
        assert_eq!(result.signals.len(), 0);
        assert_eq!(result.narration.len(), 2);
    }

    #[test]
    fn extract_malformed_signal_becomes_warning() {
        let text = "MCL::unknownsignal(bad)\nSome narration.";
        let result = extract_signals(text);
        assert_eq!(result.signals.len(), 0);
        assert_eq!(result.warnings.len(), 1);
        // Malformed signal treated as narration
        assert_eq!(result.narration.len(), 2);
    }

    #[test]
    fn extract_signals_only() {
        let text = "MCL::noop()\nMCL::transition(next_room)";
        let result = extract_signals(text);
        assert_eq!(result.signals.len(), 2);
        assert_eq!(result.narration.len(), 0);
    }

    // ── Structured JSON extraction tests ──

    #[test]
    fn extract_json_correct_action() {
        let json = r#"{
            "narration": "The skeleton guard lunges forward!",
            "signals": [
                {"type": "action", "name": "attack", "args": {"target": "skeleton_guard"}}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        assert_eq!(result.signals.len(), 1);
        assert!(
            matches!(&result.signals[0].kind, SignalKind::Action { name, args } if name == "attack" && args.get("target").is_some_and(|v| v.as_str() == Some("skeleton_guard")))
        );
        assert_eq!(result.narration.len(), 1);
        assert_eq!(result.narration[0], "The skeleton guard lunges forward!");
        assert!(result.warnings.is_empty());
    }

    #[test]
    fn extract_json_correct_dialogue() {
        let json = r#"{
            "narration": "The guard speaks.",
            "signals": [
                {"type": "dialogue", "entity": "guard", "text": "Halt, who goes there?"}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        assert_eq!(result.signals.len(), 1);
        assert!(
            matches!(&result.signals[0].kind, SignalKind::Dialogue { entity, text } if entity == "guard" && text == "Halt, who goes there?")
        );
    }

    #[test]
    fn extract_json_correct_transition() {
        let json = r#"{
            "narration": "You descend the stairs.",
            "signals": [
                {"type": "transition", "context": "room_2"}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        assert_eq!(result.signals.len(), 1);
        assert!(
            matches!(&result.signals[0].kind, SignalKind::Transition { context } if context == "room_2")
        );
    }

    #[test]
    fn extract_json_correct_noop() {
        let json = r#"{
            "narration": "You wait.",
            "signals": [
                {"type": "noop"}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        assert_eq!(result.signals.len(), 1);
        assert!(matches!(&result.signals[0].kind, SignalKind::Noop));
    }

    #[test]
    fn extract_json_malformed_json() {
        let text = "This is not JSON at all! Just plain text narration.";
        let result = extract_signals_from_json(text);
        assert!(result.signals.is_empty());
        assert!(!result.narration.is_empty());
        // The entire text should be treated as narration
        assert!(result.narration.iter().any(|l| l.contains("not JSON")));
    }

    #[test]
    fn extract_json_missing_signals_field() {
        let json = r#"{"narration": "Hello world"}"#;
        let result = extract_signals_from_json(json);
        assert!(result.signals.is_empty());
        assert_eq!(result.narration.len(), 1);
        assert_eq!(result.narration[0], "Hello world");
    }

    #[test]
    fn extract_json_empty_signals_array() {
        let json = r#"{
            "narration": "Nothing happens.",
            "signals": []
        }"#;
        let result = extract_signals_from_json(json);
        assert!(result.signals.is_empty());
        assert_eq!(result.narration.len(), 1);
        assert_eq!(result.narration[0], "Nothing happens.");
    }

    #[test]
    fn extract_json_unknown_signal_type() {
        let json = r#"{
            "narration": "Something happens.",
            "signals": [
                {"type": "frobnicate", "foo": "bar"}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        assert!(result.signals.is_empty());
        assert!(!result.warnings.is_empty());
        assert!(result.warnings.iter().any(|w| w.contains("frobnicate")));
    }

    #[test]
    fn extract_json_malformed_signal_in_array() {
        let json = r#"{
            "narration": "Mixed results.",
            "signals": [
                {"type": "action", "name": "look"},
                {"type": "action"}
            ]
        }"#;
        let result = extract_signals_from_json(json);
        // The first signal should be valid (action with no args is fine)
        assert!(result
            .signals
            .iter()
            .any(|s| matches!(&s.kind, SignalKind::Action { name, .. } if name == "look")));
        // The second should produce a warning
        assert!(!result.warnings.is_empty());
    }

    // ── Incremental extraction tests ──

    /// Helper: run incremental extraction and collect all updates into a
    /// single `ParsedResponse`-equivalent for comparison with batch mode.
    fn incremental_extract(chunks: &[&str]) -> (Vec<MclSignal>, Vec<String>, Vec<String>) {
        let mut ext = IncrementalExtractor::new();
        let mut all_signals = Vec::new();
        let mut all_narration = Vec::new();
        let mut all_warnings = Vec::new();

        for chunk in chunks {
            let upd = ext.feed(chunk);
            all_signals.extend(upd.signals);
            all_narration.extend(upd.narration);
            all_warnings.extend(upd.warnings);
        }

        let final_upd = ext.flush();
        all_signals.extend(final_upd.signals);
        all_narration.extend(final_upd.narration);
        all_warnings.extend(final_upd.warnings);

        (all_signals, all_narration, all_warnings)
    }

    #[test]
    fn incremental_matches_batch() {
        let text = "The skeleton's jaw creaks open.\n\nMCL::dialogue(skeleton_guard, \"Who dares?\")\n\nYou grip your sword.\n\nMCL::action(attack, target=skeleton_guard)";

        let batch = extract_signals(text);

        let chars: Vec<&str> = text.split_inclusive('\n').collect();
        let (signals, narration, warnings) = incremental_extract(&chars);

        assert_eq!(signals.len(), batch.signals.len());
        assert_eq!(narration.len(), batch.narration.len());
        assert_eq!(warnings.len(), batch.warnings.len());

        for (inc, bat) in signals.iter().zip(&batch.signals) {
            assert_eq!(
                std::mem::discriminant(&inc.kind),
                std::mem::discriminant(&bat.kind)
            );
        }
        assert_eq!(narration, batch.narration);
    }

    #[test]
    fn incremental_emits_partial_narration() {
        let mut ext = IncrementalExtractor::new();

        let upd = ext.feed("Hello ");
        assert!(upd.narration.is_empty());
        assert!(upd.signals.is_empty());

        let upd = ext.feed("world\n");
        assert_eq!(upd.narration.len(), 1);
        assert_eq!(upd.narration[0], "Hello world");
    }

    #[test]
    fn incremental_handles_partial_signal() {
        let mut ext = IncrementalExtractor::new();

        let upd = ext.feed("MCL::act");
        assert!(upd.signals.is_empty());

        let upd = ext.feed("ion(attack, target=goblin)\n");
        assert_eq!(upd.signals.len(), 1);
        assert!(
            matches!(&upd.signals[0].kind, SignalKind::Action { name, .. } if name == "attack")
        );
    }

    #[test]
    fn incremental_flush_emits_remaining() {
        let mut ext = IncrementalExtractor::new();

        ext.feed("Some narration without newline");
        assert!(ext.flush().narration.len() == 1);
        assert_eq!(ext.flush().narration.len(), 0);

        let mut ext2 = IncrementalExtractor::new();
        ext2.feed("MCL::noop()");
        let upd = ext2.flush();
        assert_eq!(upd.signals.len(), 1);
        assert!(matches!(&upd.signals[0].kind, SignalKind::Noop));
    }

    #[test]
    fn incremental_empty_chunks() {
        let mut ext = IncrementalExtractor::new();

        let upd = ext.feed("");
        assert!(upd.narration.is_empty());
        assert!(upd.signals.is_empty());
        assert!(upd.warnings.is_empty());

        let upd = ext.feed("Text\n");
        assert_eq!(upd.narration.len(), 1);

        let upd = ext.feed("");
        assert!(upd.narration.is_empty());

        let upd = ext.flush();
        assert!(upd.narration.is_empty());
    }

    #[test]
    fn incremental_multiple_signals() {
        let mut ext = IncrementalExtractor::new();

        let upd = ext.feed("MCL::noop()\nMCL::transition(crypt_inner)\n");
        assert_eq!(upd.signals.len(), 2);
        assert!(matches!(&upd.signals[0].kind, SignalKind::Noop));
        assert!(
            matches!(&upd.signals[1].kind, SignalKind::Transition { context } if context == "crypt_inner")
        );
    }

    #[test]
    fn incremental_mixed_content_chunked() {
        let text = "Narration line 1.\n\nMCL::dialogue(guard, \"Halt!\")\nNarration line 2.\nMCL::action(open, target=door)";

        let batch = extract_signals(text);

        let chunks = &[
            "Narration line 1.\n\nMCL::",
            "dialogue(guard, \"Halt!\")\n",
            "Narration line 2.\nMCL::action(open, target=door)",
        ];
        let (signals, narration, warnings) = incremental_extract(chunks);

        assert_eq!(signals.len(), batch.signals.len());
        assert_eq!(narration, batch.narration);
        assert_eq!(warnings.len(), batch.warnings.len());
    }

    #[test]
    fn incremental_malformed_signal_same_as_batch() {
        let text = "MCL::unknownsignal(bad)\nSome narration.";
        let batch = extract_signals(text);

        let chunks: Vec<&str> = text.split_inclusive('\n').collect();
        let (signals, narration, warnings) = incremental_extract(&chunks);

        assert_eq!(signals.len(), batch.signals.len());
        assert_eq!(warnings.len(), batch.warnings.len());
        assert_eq!(narration, batch.narration);
    }

    // ── extract_signals_from tests ──

    fn untrusted_source() -> Source {
        Source {
            id: SourceId::from("webhook"),
            trust: Trust::Untrusted,
            inherit_labels: vec![],
            verify: None,
        }
    }

    #[test]
    fn extract_signals_from_tags_untrusted() {
        let source = untrusted_source();
        let text = "MCL::action(attack, target=goblin)\nMCL::noop()";
        let result = extract_signals_from(text, &source);

        assert_eq!(result.signals.len(), 2);
        for signal in &result.signals {
            assert!(signal.labels.contains_name("source"));
        }

        let first = &result.signals[0];
        let source_label = first.labels.0.iter().find(|l| l.name == "source").unwrap();
        assert_eq!(
            source_label.params.get("id"),
            Some(&LabelParam::Str("webhook".into()))
        );
        assert_eq!(
            source_label.params.get("trust"),
            Some(&LabelParam::Ident("untrusted".into()))
        );
    }

    #[test]
    fn extract_signals_from_inherits_labels() {
        let source = Source {
            id: SourceId::from("api"),
            trust: Trust::External,
            inherit_labels: vec!["sensitive".into(), "pii".into()],
            verify: None,
        };
        let text = "MCL::action(open, target=door)";
        let result = extract_signals_from(text, &source);

        assert_eq!(result.signals.len(), 1);
        let signal = &result.signals[0];
        assert!(signal.labels.contains_name("sensitive"));
        assert!(signal.labels.contains_name("pii"));
        assert!(signal.labels.contains_name("source"));
    }

    #[test]
    fn extract_signals_backward_compat() {
        let text = "Narration line.\nMCL::action(attack, target=orc)";
        let plain = extract_signals(text);
        let source = untrusted_source();
        let tagged = extract_signals_from(text, &source);

        assert_eq!(plain.signals.len(), tagged.signals.len());
        assert_eq!(plain.narration, tagged.narration);
        assert_eq!(plain.warnings, tagged.warnings);

        assert!(
            plain.signals[0].labels.is_empty(),
            "extract_signals should produce empty labels"
        );
        assert!(
            !tagged.signals[0].labels.is_empty(),
            "extract_signals_from should tag with source"
        );
    }
}
