use std::path::PathBuf;

use async_trait::async_trait;
use mcl_graph::context::NodeContext;
use mcl_graph::error::GraphError;
use mcl_graph::message::Packet;
use mcl_graph::node::{ChannelSpec, GovernanceKind, GoverningNode, Node, NodeCapability};
use mcl_llm::backend::LlmBackend;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum RollupPolicy {
    TurnWindow(u64),
    SceneBoundary(String),
    TokenBudget(usize),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SummaryLevel {
    Turn,
    Scene,
    Session,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SummaryEntry {
    pub level: SummaryLevel,
    pub turn_start: u64,
    pub turn_end: u64,
    pub text: String,
    pub char_count: usize,
}

impl SummaryEntry {}

pub struct SummarizerNode {
    llm: Box<dyn LlmBackend>,
    summaries_dir: PathBuf,
    policy: RollupPolicy,
    turn_buffer: Vec<(u64, String)>,
    current_turn: u64,
    summaries: Vec<SummaryEntry>,
    scene_count: u64,
    scene_buffer: Vec<SummaryEntry>,
    scene_boundary_signal: Option<String>,
    system_prompt: String,
}

impl SummarizerNode {
    pub fn new(llm: Box<dyn LlmBackend>, summaries_dir: PathBuf, policy: RollupPolicy) -> Self {
        let scene_boundary = match &policy {
            RollupPolicy::SceneBoundary(sig) => Some(sig.clone()),
            _ => None,
        };

        Self {
            llm,
            summaries_dir,
            policy,
            turn_buffer: Vec::new(),
            current_turn: 0,
            summaries: Vec::new(),
            scene_count: 0,
            scene_buffer: Vec::new(),
            scene_boundary_signal: scene_boundary,
            system_prompt: "You are a summarizer for a game session. Produce a concise, \
                factual summary of the events described. Focus on actions taken, state \
                changes, and key decisions. Do not add commentary or speculation."
                .to_string(),
        }
    }

    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    pub fn summaries(&self) -> &[SummaryEntry] {
        &self.summaries
    }

    fn batch_text(&self) -> String {
        self.turn_buffer
            .iter()
            .map(|(turn, text)| format!("[turn {}] {}", turn, text))
            .collect::<Vec<_>>()
            .join("\n")
    }

    async fn summarize_turn(&mut self) -> Result<SummaryEntry, GraphError> {
        let text = self.batch_text();
        if text.is_empty() {
            return Err(GraphError::NodeError("empty turn buffer".to_string()));
        }

        let turn_start = self.turn_buffer.first().expect("buffer non-empty").0;
        let turn_end = self.turn_buffer.last().expect("buffer non-empty").0;

        let user_message = format!("Summarize the following events:\n\n{}", text);
        let summary_text = self
            .llm
            .complete(&self.system_prompt, &[], &user_message)
            .await
            .map_err(|e| GraphError::NodeError(format!("LLM error: {}", e)))?;

        let char_count = summary_text.len();
        let entry = SummaryEntry {
            level: SummaryLevel::Turn,
            turn_start,
            turn_end,
            text: summary_text,
            char_count,
        };

        self.turn_buffer.clear();
        Ok(entry)
    }

    fn should_rollup_turn(&self) -> bool {
        match &self.policy {
            RollupPolicy::TurnWindow(n) => {
                if self.turn_buffer.is_empty() {
                    return false;
                }
                let first_turn = self.turn_buffer.first().expect("checked").0;
                let last_turn = self.turn_buffer.last().expect("checked").0;
                last_turn - first_turn + 1 >= *n
            }
            RollupPolicy::SceneBoundary(_) => !self.turn_buffer.is_empty(),
            RollupPolicy::TokenBudget(budget) => {
                let total_chars: usize = self.turn_buffer.iter().map(|(_, t)| t.len()).sum();
                total_chars / 4 >= *budget
            }
        }
    }

    fn should_rollup_scene(&self) -> bool {
        match &self.policy {
            RollupPolicy::TurnWindow(n) => self.scene_buffer.len() >= (*n as usize),
            RollupPolicy::SceneBoundary(_) => !self.scene_buffer.is_empty(),
            RollupPolicy::TokenBudget(_) => !self.scene_buffer.is_empty(),
        }
    }

    async fn summarize_scene(&mut self) -> Result<SummaryEntry, GraphError> {
        if self.scene_buffer.is_empty() {
            return Err(GraphError::NodeError("empty scene buffer".to_string()));
        }

        let turn_start = self
            .scene_buffer
            .first()
            .expect("buffer non-empty")
            .turn_start;
        let turn_end = self.scene_buffer.last().expect("buffer non-empty").turn_end;

        let scene_text: String = self
            .scene_buffer
            .iter()
            .map(|s| format!("[turns {}-{}] {}", s.turn_start, s.turn_end, s.text))
            .collect::<Vec<_>>()
            .join("\n");

        let user_message = format!(
            "Summarize the following scene (composed of turn summaries):\n\n{}",
            scene_text
        );

        let summary_text = self
            .llm
            .complete(&self.system_prompt, &[], &user_message)
            .await
            .map_err(|e| GraphError::NodeError(format!("LLM error: {}", e)))?;

        let char_count = summary_text.len();
        let entry = SummaryEntry {
            level: SummaryLevel::Scene,
            turn_start,
            turn_end,
            text: summary_text,
            char_count,
        };

        self.scene_buffer.clear();
        self.scene_count += 1;
        Ok(entry)
    }

    fn detect_scene_boundary(&self, text: &str) -> bool {
        match &self.scene_boundary_signal {
            Some(sig) => text.contains(sig),
            None => false,
        }
    }

    fn persist_summary(&self, entry: &SummaryEntry) -> Result<(), GraphError> {
        std::fs::create_dir_all(&self.summaries_dir)
            .map_err(|e| GraphError::Io(format!("create summaries dir: {}", e)))?;

        let filename = match entry.level {
            SummaryLevel::Turn => {
                format!("turn-{:06}.json", entry.turn_start)
            }
            SummaryLevel::Scene => {
                format!("scene-{:06}.json", self.scene_count.saturating_sub(1))
            }
            SummaryLevel::Session => "session.json".to_string(),
        };

        let path = self.summaries_dir.join(filename);
        let json = serde_json::to_string_pretty(entry)
            .map_err(|e| GraphError::NodeError(format!("serialize summary: {}", e)))?;

        std::fs::write(&path, json).map_err(|e| GraphError::Io(format!("write summary: {}", e)))?;

        Ok(())
    }
}

#[async_trait]
impl Node for SummarizerNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "summarizer".into(),
            description: "Batches signals per turn and produces compressed summaries via LLM"
                .into(),
            subscriptions: vec![ChannelSpec::wildcard()],
            publications: vec![ChannelSpec::new("summaries")],
        }
    }

    async fn init(&mut self, ctx: &mut NodeContext) -> Result<(), GraphError> {
        if let Some(prompt) = ctx.config_str("system_prompt") {
            self.system_prompt = prompt.to_string();
        }

        if let Some(custom_dir) = ctx.config_str("summaries_dir") {
            self.summaries_dir = PathBuf::from(custom_dir);
        }

        std::fs::create_dir_all(&self.summaries_dir)
            .map_err(|e| GraphError::Io(format!("create summaries dir: {}", e)))?;

        log::info!(
            "SummarizerNode initialized: policy={:?}, dir={}",
            self.policy,
            self.summaries_dir.display()
        );

        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        packets: &[Packet],
        ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        for pkt in packets {
            let turn = pkt.id; // packet ID is used as turn proxy
            self.current_turn = turn;

            let text = pkt
                .text_content()
                .unwrap_or("<non-text packet>")
                .to_string();

            if self.detect_scene_boundary(&text) && !self.turn_buffer.is_empty() {
                if let Ok(entry) = self.summarize_turn().await {
                    self.persist_summary(&entry)?;
                    self.scene_buffer.push(entry);
                    if self.should_rollup_scene() {
                        if let Ok(scene_entry) = self.summarize_scene().await {
                            self.persist_summary(&scene_entry)?;
                            self.summaries.push(scene_entry);
                        }
                    }
                }
                continue;
            }

            self.turn_buffer.push((turn, text));

            if self.should_rollup_turn() {
                if let Ok(entry) = self.summarize_turn().await {
                    self.persist_summary(&entry)?;
                    self.scene_buffer.push(entry.clone());
                    self.summaries.push(entry);

                    if self.should_rollup_scene() {
                        if let Ok(scene_entry) = self.summarize_scene().await {
                            self.persist_summary(&scene_entry)?;
                            self.summaries.push(scene_entry);
                        }
                    }
                }
            }
        }

        if let Some(last) = self.summaries.last() {
            ctx.emit_text("summaries", &last.text);
        }

        Ok(())
    }

    async fn shutdown(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
        if !self.turn_buffer.is_empty() {
            if let Ok(entry) = self.summarize_turn().await {
                self.persist_summary(&entry)?;
                self.scene_buffer.push(entry.clone());
                self.summaries.push(entry);
            }
        }

        if !self.scene_buffer.is_empty() {
            if let Ok(scene_entry) = self.summarize_scene().await {
                self.persist_summary(&scene_entry)?;
                self.summaries.push(scene_entry);
            }
        }

        if !self.summaries.is_empty() {
            let turn_start = self.summaries.first().expect("checked").turn_start;
            let turn_end = self.summaries.last().expect("checked").turn_end;
            let all_text: String = self
                .summaries
                .iter()
                .map(|s| s.text.as_str())
                .collect::<Vec<_>>()
                .join("\n\n");

            let user_message = format!(
                "Produce a final session summary from these turn and scene summaries:\n\n{}",
                all_text
            );

            match self
                .llm
                .complete(&self.system_prompt, &[], &user_message)
                .await
            {
                Ok(text) => {
                    let session_entry = SummaryEntry {
                        level: SummaryLevel::Session,
                        turn_start,
                        turn_end,
                        text,
                        char_count: all_text.len(),
                    };
                    self.persist_summary(&session_entry)?;
                    self.summaries.push(session_entry);
                }
                Err(e) => {
                    log::warn!("session summary failed: {}", e);
                }
            }
        }

        Ok(())
    }
}

impl GoverningNode for SummarizerNode {
    fn governance_kind(&self) -> GovernanceKind {
        GovernanceKind::Summarizer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_graph::id::NodeId;
    use mcl_llm::MockLlmBackend;
    use mcl_spec::value::MclValue;
    use tempfile::TempDir;

    fn test_packet(channel: &str, id: u64, text: &str) -> Packet {
        Packet::new(
            channel,
            NodeId::new("source"),
            id,
            MclValue::String(text.to_string()),
        )
    }

    fn test_ctx() -> NodeContext {
        use indexmap::IndexMap;
        NodeContext::new(NodeId::new("summarizer"), IndexMap::new())
    }

    #[tokio::test]
    async fn turn_summary_produced_after_n_turns() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        // 3 canned responses: turn1 summary, turn2 summary, scene summary
        let llm = MockLlmBackend::new(vec![
            "Turn 1-3 summary".to_string(),
            "Turn 4-6 summary".to_string(),
            "Scene summary".to_string(),
        ]);

        let mut node =
            SummarizerNode::new(Box::new(llm), summaries_dir, RollupPolicy::TurnWindow(3));

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        // Send 3 packets (one per turn, IDs 1-3)
        let packets: Vec<Packet> = (1..=3)
            .map(|i| test_packet("actions", i, &format!("Action {}", i)))
            .collect();

        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        let summaries = node.summaries();
        assert_eq!(
            summaries.len(),
            1,
            "should have 1 turn summary after 3 turns"
        );
        assert_eq!(summaries[0].level, SummaryLevel::Turn);
        assert_eq!(summaries[0].turn_start, 1);
        assert_eq!(summaries[0].turn_end, 3);
        assert_eq!(summaries[0].text, "Turn 1-3 summary");
    }

    #[tokio::test]
    async fn summaries_are_deterministic_with_mock() {
        let tmp1 = TempDir::new().expect("temp dir");
        let tmp2 = TempDir::new().expect("temp dir");

        let llm1 = MockLlmBackend::new(vec!["Summary A".to_string()]);
        let llm2 = MockLlmBackend::new(vec!["Summary A".to_string()]);

        let mut node1 = SummarizerNode::new(
            Box::new(llm1),
            tmp1.path().join("summaries"),
            RollupPolicy::TurnWindow(2),
        );
        let mut node2 = SummarizerNode::new(
            Box::new(llm2),
            tmp2.path().join("summaries"),
            RollupPolicy::TurnWindow(2),
        );

        let mut ctx = test_ctx();

        node1.init(&mut ctx).await.expect("init1");
        node2.init(&mut ctx).await.expect("init2");

        let packets: Vec<Packet> = (1..=2)
            .map(|i| test_packet("actions", i, &format!("Action {}", i)))
            .collect();

        node1
            .process("actions", &packets, &mut ctx)
            .await
            .expect("process1");
        node2
            .process("actions", &packets, &mut ctx)
            .await
            .expect("process2");

        assert_eq!(
            node1.summaries().len(),
            node2.summaries().len(),
            "same number of summaries"
        );
        assert_eq!(
            node1.summaries()[0].text,
            node2.summaries()[0].text,
            "summary text must be identical"
        );
    }

    #[tokio::test]
    async fn summaries_persisted_to_disk() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        let llm = MockLlmBackend::new(vec!["Persisted summary".to_string()]);

        let mut node = SummarizerNode::new(
            Box::new(llm),
            summaries_dir.clone(),
            RollupPolicy::TurnWindow(2),
        );

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        let packets: Vec<Packet> = (1..=2)
            .map(|i| test_packet("actions", i, &format!("Action {}", i)))
            .collect();

        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        let summary_file = summaries_dir.join("turn-000001.json");
        assert!(summary_file.exists(), "summary file should exist on disk");

        let content = std::fs::read_to_string(&summary_file).expect("read file");
        let entry: SummaryEntry = serde_json::from_str(&content).expect("parse");
        assert_eq!(entry.text, "Persisted summary");
        assert_eq!(entry.level, SummaryLevel::Turn);
    }

    #[tokio::test]
    async fn scene_summary_produced_on_boundary_signal() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        // responses: turn summary for first batch, scene summary
        let llm = MockLlmBackend::new(vec![
            "Scene 1 turn summary".to_string(),
            "Scene 1 overall summary".to_string(),
        ]);

        let mut node = SummarizerNode::new(
            Box::new(llm),
            summaries_dir,
            RollupPolicy::SceneBoundary("SCENE_BREAK".to_string()),
        );

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        // Normal packets, then a boundary signal
        let packets = vec![
            test_packet("actions", 1, "Action 1"),
            test_packet("actions", 2, "Action 2"),
            test_packet("actions", 3, "SCENE_BREAK"),
        ];

        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        let summaries = node.summaries();
        assert!(
            summaries.len() >= 2,
            "should have turn + scene summary, got {}",
            summaries.len()
        );

        let has_scene = summaries.iter().any(|s| s.level == SummaryLevel::Scene);
        assert!(has_scene, "should have a scene-level summary");
    }

    #[tokio::test]
    async fn governance_kind_is_summarizer() {
        let tmp = TempDir::new().expect("temp dir");
        let llm = MockLlmBackend::new(vec!["summary".to_string()]);

        let node = SummarizerNode::new(
            Box::new(llm),
            tmp.path().join("summaries"),
            RollupPolicy::TurnWindow(5),
        );

        assert_eq!(node.governance_kind(), GovernanceKind::Summarizer);
    }

    #[tokio::test]
    async fn wildcard_capability() {
        let tmp = TempDir::new().expect("temp dir");
        let llm = MockLlmBackend::new(vec!["summary".to_string()]);

        let node = SummarizerNode::new(
            Box::new(llm),
            tmp.path().join("summaries"),
            RollupPolicy::TurnWindow(5),
        );

        let cap = node.capability();
        assert_eq!(cap.type_name, "summarizer");
        assert_eq!(cap.subscriptions.len(), 1);
        assert!(cap.subscriptions[0].is_wildcard());
    }

    #[tokio::test]
    async fn token_budget_rollup() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        // Each text is 40 chars -> 10 estimated tokens. Budget = 20 tokens = 80 chars.
        let llm = MockLlmBackend::new(vec!["Token budget summary".to_string()]);

        let mut node =
            SummarizerNode::new(Box::new(llm), summaries_dir, RollupPolicy::TokenBudget(20));

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        // 3 packets of 40 chars each = 120 chars = 30 estimated tokens > 20 budget
        let long_text = "a".repeat(40);
        let packets: Vec<Packet> = (1..=3)
            .map(|i| test_packet("actions", i, &format!("{}-{}", long_text, i)))
            .collect();

        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        assert!(
            !node.summaries().is_empty(),
            "token budget should trigger rollup"
        );
    }

    #[tokio::test]
    async fn shutdown_flushes_remaining_buffer() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        // shutdown needs: turn summary, then scene summary, then session summary
        let llm = MockLlmBackend::new(vec![
            "Flushed turn summary".to_string(),
            "Flushed scene summary".to_string(),
            "Flushed session summary".to_string(),
        ]);

        let mut node =
            SummarizerNode::new(Box::new(llm), summaries_dir, RollupPolicy::TurnWindow(5));

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        // Only 1 turn — not enough for TurnWindow(5), so stays in buffer
        let packets = vec![test_packet("actions", 1, "Action 1")];
        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        assert!(node.summaries().is_empty(), "nothing rolled up yet");

        node.shutdown(&mut ctx).await.expect("shutdown");

        let summaries = node.summaries();
        assert!(
            !summaries.is_empty(),
            "shutdown should flush remaining buffer"
        );

        let has_turn = summaries.iter().any(|s| s.level == SummaryLevel::Turn);
        assert!(has_turn, "shutdown should produce turn summary from buffer");
    }

    #[tokio::test]
    async fn summaries_queryable_by_level() {
        let tmp = TempDir::new().expect("temp dir");
        let summaries_dir = tmp.path().join("summaries");

        let llm = MockLlmBackend::new(vec![
            "T1 summary".to_string(),
            "T2 summary".to_string(),
            "Scene summary".to_string(),
        ]);

        let mut node =
            SummarizerNode::new(Box::new(llm), summaries_dir, RollupPolicy::TurnWindow(1));

        let mut ctx = test_ctx();
        node.init(&mut ctx).await.expect("init");

        // 2 turns -> 2 turn summaries + 1 scene summary (scene buffer >= turn window)
        let packets: Vec<Packet> = (1..=2)
            .map(|i| test_packet("actions", i, &format!("Action {}", i)))
            .collect();

        node.process("actions", &packets, &mut ctx)
            .await
            .expect("process");

        let turn_summaries: Vec<&SummaryEntry> = node
            .summaries()
            .iter()
            .filter(|s| s.level == SummaryLevel::Turn)
            .collect();
        assert_eq!(turn_summaries.len(), 2, "should have 2 turn summaries");

        let scene_summaries: Vec<&SummaryEntry> = node
            .summaries()
            .iter()
            .filter(|s| s.level == SummaryLevel::Scene)
            .collect();
        assert_eq!(scene_summaries.len(), 1, "should have 1 scene summary");
    }
}
