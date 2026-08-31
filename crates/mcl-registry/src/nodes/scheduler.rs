use std::collections::HashMap;

use async_trait::async_trait;
use mcl_graph::context::NodeContext;
use mcl_graph::error::GraphError;
use mcl_graph::id::NodeId;
use mcl_graph::message::Packet;
use mcl_graph::node::{ChannelSpec, GovernanceKind, GoverningNode, Node, NodeCapability};
use mcl_spec::value::MclValue;

pub const DEFAULT_MAX_WAIT: u32 = 30;
pub const DEFAULT_TIMEOUT_STEPS: u32 = 10;
pub const DEFAULT_MAX_CONSECUTIVE_FAILURES: u32 = 3;

#[derive(Debug, Clone)]
pub struct NodeState {
    pub priority: u32,
    pub consecutive_failures: u32,
    pub disabled: bool,
    pub last_fired_step: u64,
    pub timeout_steps: u32,
}

#[derive(Debug)]
pub struct SchedulerNode {
    priorities: HashMap<String, u32>,
    node_state: HashMap<String, NodeState>,
    turn_order: Vec<NodeId>,
    current_step: u64,
    max_wait: u32,
    max_consecutive_failures: u32,
    managed_nodes: Vec<NodeId>,
}

impl SchedulerNode {
    pub fn new() -> Self {
        Self {
            priorities: HashMap::new(),
            node_state: HashMap::new(),
            turn_order: Vec::new(),
            current_step: 0,
            max_wait: DEFAULT_MAX_WAIT,
            max_consecutive_failures: DEFAULT_MAX_CONSECUTIVE_FAILURES,
            managed_nodes: Vec::new(),
        }
    }

    pub fn with_max_wait(mut self, max_wait: u32) -> Self {
        self.max_wait = max_wait;
        self
    }

    pub fn with_max_consecutive_failures(mut self, max: u32) -> Self {
        self.max_consecutive_failures = max;
        self
    }

    pub fn register_node(&mut self, id: &NodeId, priority: u32) {
        self.priorities.insert(id.0.clone(), priority);
        self.managed_nodes.push(id.clone());
        self.node_state.insert(
            id.0.clone(),
            NodeState {
                priority,
                consecutive_failures: 0,
                disabled: false,
                last_fired_step: 0,
                timeout_steps: DEFAULT_TIMEOUT_STEPS,
            },
        );
        self.rebuild_turn_order();
    }

    pub fn rebuild_turn_order(&mut self) {
        let mut nodes: Vec<&String> = self
            .priorities
            .keys()
            .filter(|k| !self.is_disabled(k))
            .collect();
        nodes.sort_by(|a, b| {
            let pa = self.priorities.get(*a).copied().unwrap_or(0);
            let pb = self.priorities.get(*b).copied().unwrap_or(0);
            pb.cmp(&pa)
        });
        self.turn_order = nodes.into_iter().map(|s| NodeId::new(s.as_str())).collect();
    }

    fn is_disabled(&self, id: &str) -> bool {
        self.node_state
            .get(id)
            .map(|s| s.disabled || s.consecutive_failures >= self.max_consecutive_failures)
            .unwrap_or(false)
    }

    pub fn record_success(&mut self, id: &NodeId) {
        if let Some(state) = self.node_state.get_mut(&id.0) {
            state.consecutive_failures = 0;
            state.last_fired_step = self.current_step;
        }
    }

    pub fn record_failure(&mut self, id: &NodeId) {
        if let Some(state) = self.node_state.get_mut(&id.0) {
            state.consecutive_failures += 1;
            if state.consecutive_failures >= self.max_consecutive_failures {
                state.disabled = true;
                self.rebuild_turn_order();
            }
        }
    }

    pub fn check_timeout(&mut self, id: &NodeId) -> bool {
        let timed_out = self
            .node_state
            .get(&id.0)
            .map(|s| {
                if self.current_step <= s.last_fired_step {
                    return false;
                }
                let elapsed = self.current_step - s.last_fired_step;
                elapsed > s.timeout_steps as u64
            })
            .unwrap_or(false);
        if timed_out {
            if let Some(state) = self.node_state.get_mut(&id.0) {
                state.last_fired_step = self.current_step;
            }
        }
        timed_out
    }

    pub fn active_nodes(&self) -> Vec<&NodeId> {
        self.managed_nodes
            .iter()
            .filter(|id| !self.is_disabled(id.as_str()))
            .collect()
    }

    pub fn turn_order(&self) -> &[NodeId] {
        &self.turn_order
    }

    pub fn is_node_disabled(&self, id: &NodeId) -> bool {
        self.is_disabled(id.as_str())
    }

    pub fn next_node(&mut self, candidates: &[NodeId]) -> Option<NodeId> {
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            return Some(candidates[0].clone());
        }

        let mut boosted: Vec<(NodeId, u32)> = Vec::new();
        for id in candidates {
            let effective_priority = self.effective_priority(id);
            boosted.push((id.clone(), effective_priority));
        }
        boosted.sort_by_key(|b| std::cmp::Reverse(b.1));

        Some(boosted[0].0.clone())
    }

    fn effective_priority(&self, id: &NodeId) -> u32 {
        let base = self.priorities.get(&id.0).copied().unwrap_or(0);
        let state = self.node_state.get(&id.0);

        let starved = state
            .map(|s| {
                self.current_step > s.last_fired_step
                    && (self.current_step - s.last_fired_step) >= self.max_wait as u64
            })
            .unwrap_or(false);

        if starved {
            return u32::MAX;
        }

        let wait_boost = state
            .map(|s| {
                if self.current_step > s.last_fired_step {
                    let wait = self.current_step - s.last_fired_step;
                    let ratio = wait as f64 / self.max_wait as f64;
                    (ratio * base as f64 * 0.5) as u32
                } else {
                    0
                }
            })
            .unwrap_or(0);

        base.saturating_add(wait_boost)
    }

    pub fn advance_step(&mut self) {
        self.current_step += 1;
    }

    pub fn current_step(&self) -> u64 {
        self.current_step
    }

    pub fn node_state(&self, id: &NodeId) -> Option<&NodeState> {
        self.node_state.get(&id.0)
    }

    pub fn managed_node_count(&self) -> usize {
        self.managed_nodes.len()
    }
}

impl Default for SchedulerNode {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Node for SchedulerNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "scheduler".into(),
            description:
                "governs turn order with priority-based scheduling, timeouts, and circuit breakers"
                    .into(),
            subscriptions: vec![ChannelSpec::wildcard()],
            publications: vec![],
        }
    }

    async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        _packets: &[Packet],
        ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        self.advance_step();

        let active = self.active_nodes();
        if active.len() <= 1 {
            return Ok(());
        }

        let active_ids: Vec<NodeId> = active.into_iter().cloned().collect();

        for id in &active_ids {
            if self.check_timeout(id) {
                ctx.emit(
                    "scheduler_events",
                    MclValue::String(format!("timeout:{}", id)),
                );
            }
        }

        Ok(())
    }
}

impl GoverningNode for SchedulerNode {
    fn governance_kind(&self) -> GovernanceKind {
        GovernanceKind::Scheduler
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_id(name: &str) -> NodeId {
        NodeId::new(name)
    }

    #[test]
    fn priority_ordering_matches_priority_values() {
        let mut sched = SchedulerNode::new();
        sched.register_node(&make_id("low"), 1);
        sched.register_node(&make_id("medium"), 5);
        sched.register_node(&make_id("high"), 10);

        let candidates = vec![make_id("low"), make_id("medium"), make_id("high")];
        let next = sched.next_node(&candidates).expect("should pick a node");
        assert_eq!(next.as_str(), "high");
    }

    #[test]
    fn priority_ordering_with_three_llm_nodes() {
        let mut sched = SchedulerNode::new();
        sched.register_node(&make_id("narrator"), 2);
        sched.register_node(&make_id("advisor"), 8);
        sched.register_node(&make_id("editor"), 5);

        let order = sched.turn_order();
        assert_eq!(order.len(), 3);
        assert_eq!(order[0].as_str(), "advisor");
        assert_eq!(order[1].as_str(), "editor");
        assert_eq!(order[2].as_str(), "narrator");
    }

    #[test]
    fn single_node_graph_is_noop() {
        let mut sched = SchedulerNode::new();
        sched.register_node(&make_id("only"), 1);

        let candidates = vec![make_id("only")];
        let next = sched
            .next_node(&candidates)
            .expect("should pick the single node");
        assert_eq!(next.as_str(), "only");
    }

    #[test]
    fn empty_candidates_returns_none() {
        let mut sched = SchedulerNode::new();
        assert_eq!(sched.next_node(&[]), None);
    }

    #[test]
    fn circuit_breaker_disables_after_max_failures() {
        let mut sched = SchedulerNode::new().with_max_consecutive_failures(3);
        sched.register_node(&make_id("flaky"), 5);
        sched.register_node(&make_id("stable"), 3);

        for _ in 0..3 {
            sched.record_failure(&make_id("flaky"));
        }

        assert!(sched.is_node_disabled(&make_id("flaky")));
        assert!(!sched.is_node_disabled(&make_id("stable")));

        let order = sched.turn_order();
        assert_eq!(order.len(), 1);
        assert_eq!(order[0].as_str(), "stable");
    }

    #[test]
    fn circuit_breaker_resets_on_success() {
        let mut sched = SchedulerNode::new().with_max_consecutive_failures(3);
        sched.register_node(&make_id("node"), 1);

        sched.record_failure(&make_id("node"));
        sched.record_failure(&make_id("node"));
        assert!(!sched.is_node_disabled(&make_id("node")));

        sched.record_success(&make_id("node"));
        assert_eq!(
            sched
                .node_state(&make_id("node"))
                .expect("state")
                .consecutive_failures,
            0
        );

        sched.record_failure(&make_id("node"));
        sched.record_failure(&make_id("node"));
        sched.record_failure(&make_id("node"));
        assert!(sched.is_node_disabled(&make_id("node")));
    }

    #[test]
    fn timeout_triggers_fallback_emission() {
        let mut sched = SchedulerNode::new().with_max_wait(5);
        sched.register_node(&make_id("slow"), 5);
        sched.register_node(&make_id("fast"), 3);

        // Simulate "fast" firing at step 1, "slow" never firing
        sched.advance_step();
        sched.record_success(&make_id("fast"));

        // Step 2-6: slow hasn't fired, timeout_steps=10 by default but we check timeout
        for _ in 0..5 {
            sched.advance_step();
        }
        // current_step is now 6
        assert_eq!(sched.current_step(), 6);

        // The node_state for "slow" has last_fired_step=0, current_step=6 > 0, so timed out
        // But timeout_steps is DEFAULT_TIMEOUT_STEPS=10, and 6 < 10, so NOT timed out yet
        assert!(!sched.check_timeout(&make_id("slow")));

        // Advance more to exceed timeout_steps
        for _ in 0..5 {
            sched.advance_step();
        }
        // current_step is now 11, slow.last_fired_step=0, 11-0=11 > 10
        assert!(sched.check_timeout(&make_id("slow")));
    }

    #[test]
    fn starvation_prevention_boosts_waited_nodes() {
        let mut sched = SchedulerNode::new().with_max_wait(5);
        sched.register_node(&make_id("high_pri"), 10);
        sched.register_node(&make_id("low_pri"), 1);

        // Simulate high_pri firing every step, low_pri never
        for _ in 0..6 {
            sched.advance_step();
            sched.record_success(&make_id("high_pri"));
        }

        // low_pri has waited 6 steps, max_wait is 5 → starvation boost kicks in
        let candidates = vec![make_id("high_pri"), make_id("low_pri")];
        let next = sched.next_node(&candidates).expect("should pick a node");
        // Starvation boost should override: low_pri gets u32::MAX effective priority
        assert_eq!(next.as_str(), "low_pri");
    }

    #[test]
    fn max_wait_prevents_indefinite_starvation() {
        let mut sched = SchedulerNode::new().with_max_wait(3);
        sched.register_node(&make_id("dominant"), 100);
        sched.register_node(&make_id("starved"), 1);

        // Run for many steps, only dominant fires
        for _ in 0..20 {
            sched.advance_step();
            sched.record_success(&make_id("dominant"));
        }

        // starved has waited 20 steps, way past max_wait=3
        let candidates = vec![make_id("dominant"), make_id("starved")];
        let next = sched.next_node(&candidates).expect("should pick a node");
        assert_eq!(next.as_str(), "starved");
    }

    #[test]
    fn turn_order_rebuilds_after_disable() {
        let mut sched = SchedulerNode::new().with_max_consecutive_failures(2);
        sched.register_node(&make_id("a"), 10);
        sched.register_node(&make_id("b"), 5);
        sched.register_node(&make_id("c"), 1);

        assert_eq!(sched.turn_order().len(), 3);

        sched.record_failure(&make_id("a"));
        sched.record_failure(&make_id("a"));

        assert!(sched.is_node_disabled(&make_id("a")));
        let order = sched.turn_order();
        assert_eq!(order.len(), 2);
        assert_eq!(order[0].as_str(), "b");
        assert_eq!(order[1].as_str(), "c");
    }

    #[test]
    fn default_max_wait_is_thirty() {
        let sched = SchedulerNode::new();
        assert_eq!(sched.max_wait, DEFAULT_MAX_WAIT);
        assert_eq!(DEFAULT_MAX_WAIT, 30);
    }

    #[test]
    fn managed_node_count() {
        let mut sched = SchedulerNode::new();
        assert_eq!(sched.managed_node_count(), 0);

        sched.register_node(&make_id("a"), 1);
        sched.register_node(&make_id("b"), 2);
        assert_eq!(sched.managed_node_count(), 2);
    }

    #[tokio::test]
    async fn scheduler_node_implements_governing_node() {
        let sched = SchedulerNode::new();
        assert_eq!(sched.governance_kind(), GovernanceKind::Scheduler);
    }

    #[tokio::test]
    async fn scheduler_process_advances_step() {
        let mut sched = SchedulerNode::new();
        sched.register_node(&make_id("a"), 1);

        let config = indexmap::IndexMap::new();
        let mut ctx = NodeContext::new(make_id("scheduler"), config);

        assert_eq!(sched.current_step(), 0);
        sched
            .process("any_channel", &[], &mut ctx)
            .await
            .expect("process ok");
        assert_eq!(sched.current_step(), 1);
    }

    #[tokio::test]
    async fn scheduler_process_single_node_no_timeout_check() {
        let mut sched = SchedulerNode::new();
        sched.register_node(&make_id("only"), 1);

        let config = indexmap::IndexMap::new();
        let mut ctx = NodeContext::new(make_id("scheduler"), config);

        // Advance many steps
        for _ in 0..100 {
            sched.process("ch", &[], &mut ctx).await.expect("ok");
        }
        // Should not panic or emit anything — single node is no-op
    }
}
