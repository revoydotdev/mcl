use mcl_llm::backend::{ChatMessage, ChatRole, LlmBackend};
use mcl_signal::validate::{validate_signal, SignalValidation};
use mcl_spec::signal::{Resolution, SignalKind};
use mcl_spec::MclError;
use rand::rngs::StdRng;
use rand::SeedableRng;

use crate::context::ContextManager;
use crate::prompt::{
    build_structured_system_prompt, build_system_prompt, format_resolution_feedback,
};
use crate::resolver::{resolve_mutates, ResolverRegistry};
use crate::snapshot::{BackendConfig, SessionSnapshot};
use crate::state::WorldState;

/// Configuration for session behavior
#[derive(Debug, Clone)]
pub struct SessionConfig {
    pub max_signal_retries: usize,
    pub signal_mode: SignalMode,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum SignalMode {
    #[default]
    Inline,
    Structured,
}

impl Default for SessionConfig {
    fn default() -> Self {
        Self {
            max_signal_retries: 1,
            signal_mode: SignalMode::default(),
        }
    }
}

/// Result of a single turn
#[derive(Debug)]
pub struct TurnResult {
    pub narration: Vec<String>,
    pub resolutions: Vec<Resolution>,
    pub warnings: Vec<String>,
    pub transitioned_to: Option<String>,
}

/// An MCL game session
pub struct Session {
    pub state: WorldState,
    pub contexts: ContextManager,
    pub resolvers: ResolverRegistry,
    llm: Box<dyn LlmBackend>,
    history: Vec<ChatMessage>,
    rng: StdRng,
    config: SessionConfig,
}

impl Session {
    pub fn new(llm: Box<dyn LlmBackend>) -> Self {
        Self::with_config(llm, SessionConfig::default())
    }

    pub fn with_config(llm: Box<dyn LlmBackend>, config: SessionConfig) -> Self {
        Self::with_seed(llm, config, 42)
    }

    pub fn with_seed(llm: Box<dyn LlmBackend>, config: SessionConfig, seed: u64) -> Self {
        Self {
            state: WorldState::new(),
            contexts: ContextManager::new(),
            resolvers: ResolverRegistry::new(),
            llm,
            history: Vec::new(),
            rng: StdRng::seed_from_u64(seed),
            config,
        }
    }

    /// Execute one turn: user input -> LLM -> signal extraction -> resolution
    pub async fn turn(&mut self, user_input: &str) -> Result<TurnResult, MclError> {
        self.state.clear_changelog();

        let doc = self
            .contexts
            .active_doc()
            .ok_or_else(|| MclError::State("no active context".to_string()))?;

        // Build system prompt
        let system_prompt = if self.config.signal_mode == SignalMode::Structured {
            build_structured_system_prompt(doc, &self.state)
        } else {
            build_system_prompt(doc, &self.state)
        };

        // Call LLM
        let raw_response = self
            .llm
            .complete(&system_prompt, &self.history, user_input)
            .await?;

        // Record in history
        self.history.push(ChatMessage {
            role: ChatRole::User,
            content: user_input.to_string(),
        });
        self.history.push(ChatMessage {
            role: ChatRole::Assistant,
            content: raw_response.clone(),
        });

        // Extract signals
        let mut raw_response = raw_response;
        let mut parsed = if self.config.signal_mode == SignalMode::Structured {
            mcl_signal::extract_signals_from_json(&raw_response)
        } else {
            mcl_signal::extract_signals(&raw_response)
        };

        let mut retries_used = 0usize;
        while retries_used < self.config.max_signal_retries
            && parsed.signals.is_empty()
            && parsed.warnings.is_empty()
            && input_looks_action_oriented(user_input)
        {
            retries_used += 1;

            let doc = match self.contexts.active_doc() {
                Some(d) => d,
                None => break,
            };

            let available_signals = build_available_signals_list(doc);
            let nudge = format!(
                "\n[System: Your response should include an MCL:: signal for the action taken. \
                 The player said: '{}'. Available signals: {}]",
                user_input, available_signals
            );

            let retry_response = match self
                .llm
                .complete(&system_prompt, &self.history, &nudge)
                .await
            {
                Ok(r) => r,
                Err(_) => break,
            };

            self.history.push(ChatMessage {
                role: ChatRole::User,
                content: nudge,
            });
            self.history.push(ChatMessage {
                role: ChatRole::Assistant,
                content: retry_response.clone(),
            });

            raw_response = retry_response;
            parsed = mcl_signal::extract_signals(&raw_response);
        }

        // Process signals
        let mut resolutions = Vec::new();
        let mut warnings = parsed.warnings.clone();
        let mut transitioned_to = None;

        // Re-fetch doc since we can't hold the borrow across mutations
        for signal in &parsed.signals {
            let doc = match self.contexts.active_doc() {
                Some(d) => d,
                None => continue,
            };

            // Validate
            match validate_signal(signal, doc) {
                SignalValidation::Valid => {}
                SignalValidation::Warning(msg) => {
                    warnings.push(msg);
                }
                SignalValidation::Invalid(msg) => {
                    warnings.push(format!("invalid signal: {msg}"));
                    continue;
                }
            }

            // Resolve
            match &signal.kind {
                SignalKind::Action { name, .. } => {
                    let doc = match self.contexts.active_doc() {
                        Some(d) => d,
                        None => continue,
                    };
                    if let Some(action) = doc.actions.get(name).cloned() {
                        if let Some(ref system) = action.resolution {
                            // Delegate to registered resolver
                            if let Some(resolver) = self.resolvers.get(system) {
                                match resolver.resolve(&action, signal, &self.state, &mut self.rng)
                                {
                                    Ok(resolution) => {
                                        // Apply any state changes from the resolution
                                        for (key, val) in &resolution.details {
                                            if let Some(path) = key.strip_prefix("set:") {
                                                let _ = self.state.set(path, val.clone());
                                            }
                                        }
                                        resolutions.push(resolution);
                                    }
                                    Err(e) => {
                                        warnings.push(format!("resolver error: {e}"));
                                    }
                                }
                            } else {
                                warnings.push(format!("no resolver registered for `{system}`"));
                            }
                        } else {
                            // Direct mutates
                            match resolve_mutates(&action, &mut self.state) {
                                Ok(resolution) => resolutions.push(resolution),
                                Err(e) => warnings.push(format!("mutation error: {e}")),
                            }
                        }
                    }
                }

                SignalKind::Mutate { path, value } => {
                    let _ = self.state.set(path, value.clone());
                }

                SignalKind::Transition { context } => {
                    let doc = match self.contexts.active_doc() {
                        Some(d) => d,
                        None => continue,
                    };

                    let blocked = doc.transitions.iter().any(|t| {
                        t.target == *context
                            && t.when
                                .as_ref()
                                .is_some_and(|cond| !self.state.evaluate(cond))
                    });

                    if blocked {
                        warnings.push(format!(
                            "transition to '{context}' blocked: precondition not met"
                        ));
                    } else {
                        transitioned_to = Some(context.clone());
                    }
                }

                SignalKind::Dialogue { .. }
                | SignalKind::Narrate { .. }
                | SignalKind::Noop
                | SignalKind::Error { .. }
                | SignalKind::Query { .. } => {
                    // These don't require resolution
                }
            }
        }

        // Handle transition
        if let Some(ref target) = transitioned_to {
            self.contexts.transition(target, &mut self.state)?;
        }

        // Check auto-transitions
        if transitioned_to.is_none() {
            if let Some(target) = self.contexts.check_auto_transitions(&self.state) {
                self.contexts.transition(&target, &mut self.state)?;
                transitioned_to = Some(target);
            }
        }

        // If there were resolutions, append feedback to history for next turn
        if !resolutions.is_empty() {
            let feedback = format_resolution_feedback(&resolutions);
            self.history.push(ChatMessage {
                role: ChatRole::User,
                content: format!("[System: {feedback}]"),
            });
        }

        Ok(TurnResult {
            narration: parsed.narration,
            resolutions,
            warnings,
            transitioned_to,
        })
    }

    /// Get the current context ID
    pub fn active_context(&self) -> Option<&str> {
        self.contexts.active_id()
    }

    pub fn history(&self) -> &[ChatMessage] {
        &self.history
    }

    pub fn snapshot(&self) -> SessionSnapshot {
        SessionSnapshot {
            world_state: self.state.clone(),
            active_context: self.active_context().map(|s| s.to_string()),
            conversation_history: self.history.clone(),
            backend_config: BackendConfig {
                backend_name: self.llm.name().to_string(),
                model: String::new(),
                api_key: None,
                base_url: None,
            },
        }
    }

    pub fn save(&self, path: &std::path::Path) -> Result<(), MclError> {
        self.snapshot().save_to_file(path)
    }

    pub fn load(&mut self, path: &std::path::Path) -> Result<BackendConfig, MclError> {
        let snap = SessionSnapshot::load_from_file(path)?;
        self.state = snap.world_state;
        self.history = snap.conversation_history;

        if let Some(ref ctx_id) = snap.active_context {
            self.contexts.activate(ctx_id, &mut self.state)?;
        }

        Ok(snap.backend_config)
    }
}

fn input_looks_action_oriented(input: &str) -> bool {
    let input_lower = input.to_lowercase();
    let action_keywords = [
        "attack", "kill", "fight", "strike", "hit", "shoot", "look", "examine", "inspect",
        "search", "scan", "take", "pick", "grab", "get", "loot", "go", "move", "walk", "run",
        "enter", "leave", "open", "close", "use", "light", "unlock", "talk", "speak", "ask",
        "tell", "say",
    ];
    action_keywords.iter().any(|kw| input_lower.contains(kw))
}

fn build_available_signals_list(doc: &mcl_spec::document::MclDocument) -> String {
    let mut signals = Vec::new();
    for name in doc.actions.keys() {
        signals.push(format!("MCL::action({name})"));
    }
    signals.push("MCL::dialogue(<entity_id>, \"<text>\")".to_string());
    signals.push("MCL::noop()".to_string());
    signals.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_llm::MockLlmBackend;
    use std::path::PathBuf;

    fn example_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../examples/crypt")
            .join(name)
    }

    fn load_antechamber(session: &mut Session) {
        session
            .contexts
            .load_file(&example_path("crypt_antechamber.mcl"))
            .expect("load antechamber");
        session
            .contexts
            .activate("crypt_antechamber", &mut session.state)
            .expect("activate antechamber");
    }

    fn load_both_contexts(session: &mut Session) {
        session
            .contexts
            .load_file(&example_path("crypt_antechamber.mcl"))
            .expect("load antechamber");
        session
            .contexts
            .load_file(&example_path("crypt_inner.mcl"))
            .expect("load inner");
        session
            .contexts
            .activate("crypt_antechamber", &mut session.state)
            .expect("activate antechamber");
    }

    #[tokio::test]
    async fn test_turn_without_active_context_returns_err() {
        let llm = Box::new(MockLlmBackend::new(vec!["response".to_string()]));
        let mut session = Session::new(llm);

        let result = session.turn("do something").await;
        assert!(
            result.is_err(),
            "turn should return Err when no active context is set"
        );
    }

    #[tokio::test]
    async fn malformed_signal_returns_ok_with_warnings() {
        let response = "Something happens.\nMCL::action(".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("do something").await;
        assert!(
            result.is_ok(),
            "malformed signal should not error — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(!turn.warnings.is_empty());
        assert!(!turn.narration.is_empty());
    }

    #[tokio::test]
    async fn completely_garbled_llm_output_returns_ok() {
        let response = "asdf 1234 \\\\{{}} !!!".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("do something").await;
        assert!(
            result.is_ok(),
            "garbled output should not panic — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.resolutions.is_empty());
        assert!(!turn.narration.is_empty());
    }

    #[tokio::test]
    async fn empty_llm_response_returns_ok() {
        let response = String::new();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("do something").await;
        assert!(
            result.is_ok(),
            "empty response should not panic — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.narration.is_empty());
    }

    #[tokio::test]
    async fn llm_error_propagates_as_mcl_error() {
        let llm = Box::new(MockLlmBackend::new(vec![]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("do something").await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("llm"),
            "error should be Llm variant — got: {err}"
        );
    }

    #[tokio::test]
    async fn transition_to_nonexistent_context_produces_warning() {
        let response = "You go somewhere.\nMCL::transition(nonexistent_room)".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("go to nonexistent room").await;
        assert!(
            result.is_ok(),
            "invalid transition should not error — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.transitioned_to.is_none());
        assert!(
            turn.warnings
                .iter()
                .any(|w| w.contains("invalid signal") && w.contains("nonexistent_room")),
            "should warn about invalid transition — got: {turn:?}"
        );
    }

    #[tokio::test]
    async fn transition_blocked_by_precondition_succeeds_with_warning() {
        let response = "You push the door.\nMCL::transition(crypt_inner)".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_both_contexts(&mut session);

        let result = session.turn("go north").await;
        assert!(
            result.is_ok(),
            "blocked transition should still return Ok — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.transitioned_to.is_none());
        assert!(turn.warnings.iter().any(|w| w.contains("blocked")));
    }

    #[tokio::test]
    async fn signal_for_nonexistent_action_produces_no_resolution() {
        let response = "You do a thing.\nMCL::action(nonexistent_action)".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("do a nonexistent thing").await;
        assert!(
            result.is_ok(),
            "nonexistent action should not panic — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.resolutions.is_empty());
    }

    #[tokio::test]
    async fn structured_json_malformed_returns_ok() {
        use crate::SignalMode;

        let response = "this is not json {{{".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let config = SessionConfig {
            max_signal_retries: 0,
            signal_mode: SignalMode::Structured,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let result = session.turn("do something").await;
        assert!(
            result.is_ok(),
            "malformed JSON in structured mode should not panic — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.resolutions.is_empty());
        assert!(!turn.narration.is_empty());
    }

    #[tokio::test]
    async fn structured_json_empty_signals_array_returns_ok() {
        use crate::SignalMode;

        let response = r#"{"narration": "You wait.", "signals": []}"#.to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let config = SessionConfig {
            max_signal_retries: 0,
            signal_mode: SignalMode::Structured,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let result = session.turn("wait").await;
        assert!(
            result.is_ok(),
            "empty signals array should not panic — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.resolutions.is_empty());
        assert_eq!(turn.narration.len(), 1);
    }

    #[tokio::test]
    async fn mutate_signal_creates_new_state_key() {
        let response = "Something changes.\nMCL::mutate(fresh_key, 42)".to_string();
        let llm = Box::new(MockLlmBackend::new(vec![response]));
        let mut session = Session::new(llm);
        load_antechamber(&mut session);

        let result = session.turn("change something").await;
        assert!(
            result.is_ok(),
            "mutate to new key should succeed — got: {result:?}"
        );

        let val = session.state.get_by_str("fresh_key");
        assert!(val.is_some(), "fresh_key should now exist in state");
    }

    // ── Retry-on-missing-signal tests ──

    #[tokio::test]
    async fn retry_missing_signal_succeeds_on_second_attempt() {
        // First call: narration WITHOUT signal (model forgot).
        // Second call (retry): narration WITH signal.
        let no_signal =
            "The skeleton guard stands motionless, its empty eye sockets staring ahead.";
        let with_signal = "You swing your blade at the skeleton guard!\nMCL::action(attack, target=skeleton_guard)";
        let llm = Box::new(MockLlmBackend::new(vec![
            no_signal.to_string(),
            with_signal.to_string(),
        ]));
        let config = SessionConfig {
            max_signal_retries: 1,
            signal_mode: SignalMode::Inline,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let result = session.turn("attack the skeleton guard").await;
        assert!(result.is_ok(), "retry should succeed — got: {result:?}");
        let turn = result.unwrap();
        assert!(
            !turn.narration.is_empty(),
            "should have narration from the retried response"
        );
        // The retry response should have been used (contains the attack narration)
        assert!(
            turn.narration
                .iter()
                .any(|n| n.contains("swing your blade")),
            "should use retried response with signal — got: {:?}",
            turn.narration
        );
    }

    #[tokio::test]
    async fn retry_missing_signal_capped_at_one() {
        // All responses: narration without signal. Should retry exactly once,
        // then give up (total 2 LLM calls: initial + 1 retry).
        let no_signal = "Nothing happens.";
        let llm = Box::new(MockLlmBackend::new(vec![
            no_signal.to_string(),
            no_signal.to_string(),
            no_signal.to_string(), // extra — should NOT be consumed
        ]));
        let config = SessionConfig {
            max_signal_retries: 1,
            signal_mode: SignalMode::Inline,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let result = session.turn("attack the skeleton").await;
        assert!(
            result.is_ok(),
            "should not error even after retry exhaustion — got: {result:?}"
        );
        let turn = result.unwrap();
        assert!(turn.resolutions.is_empty(), "no resolutions without signal");
        // History should have exactly 4 messages: user input, assistant (no signal),
        // retry nudge (user), assistant retry (still no signal)
        assert_eq!(
            session.history().len(),
            4,
            "should have exactly 2 LLM calls (initial + 1 retry) = 4 history messages — got: {}",
            session.history().len()
        );
    }

    #[tokio::test]
    async fn no_retry_for_non_action_input() {
        // "I sit and think" is NOT action-oriented → no retry should fire.
        // Only 1 LLM call expected.
        let no_signal = "You sit in quiet contemplation.";
        let llm = Box::new(MockLlmBackend::new(vec![
            no_signal.to_string(),
            "SHOULD NOT BE CALLED".to_string(),
        ]));
        let config = SessionConfig {
            max_signal_retries: 1,
            signal_mode: SignalMode::Inline,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let result = session.turn("I sit and think about my life").await;
        assert!(result.is_ok(), "should succeed — got: {result:?}");
        // History should have exactly 2 messages (1 LLM call, no retry)
        assert_eq!(
            session.history().len(),
            2,
            "non-action input should not trigger retry — got {} history messages",
            session.history().len()
        );
    }

    #[tokio::test]
    async fn retry_uses_explicit_nudge_message() {
        // Verify the retry sends a nudge that mentions MCL:: signals
        let no_signal = "The room is quiet.";
        let llm = Box::new(MockLlmBackend::new(vec![
            no_signal.to_string(),
            "OK.\nMCL::noop()".to_string(),
        ]));
        let config = SessionConfig {
            max_signal_retries: 1,
            signal_mode: SignalMode::Inline,
        };
        let mut session = Session::with_config(llm, config);
        load_antechamber(&mut session);

        let _ = session.turn("look around").await;

        // Check that the retry nudge was added to history
        let nudge_msgs: Vec<_> = session
            .history()
            .iter()
            .filter(|m| m.content.contains("MCL::"))
            .collect();
        assert!(
            nudge_msgs
                .iter()
                .any(|m| m.content.contains("should include an MCL:: signal")),
            "retry nudge should mention MCL:: signal requirement — got: {:?}",
            nudge_msgs
        );
    }

    // ── input_looks_action_oriented heuristic tests ──

    #[test]
    fn action_oriented_keywords_detected() {
        assert!(input_looks_action_oriented("attack the skeleton"));
        assert!(input_looks_action_oriented("LOOK around"));
        assert!(input_looks_action_oriented("Take the key"));
        assert!(input_looks_action_oriented("go north"));
        assert!(input_looks_action_oriented("talk to guard"));
        assert!(input_looks_action_oriented("open the door"));
        assert!(input_looks_action_oriented("EXAMINE the altar"));
        assert!(input_looks_action_oriented("unlock door"));
    }

    #[test]
    fn non_action_input_not_detected() {
        assert!(!input_looks_action_oriented("I sit and think"));
        assert!(!input_looks_action_oriented("hmm"));
        assert!(!input_looks_action_oriented("maybe I should wait here"));
        assert!(!input_looks_action_oriented(""));
        assert!(!input_looks_action_oriented("I wonder what happened"));
    }
}
