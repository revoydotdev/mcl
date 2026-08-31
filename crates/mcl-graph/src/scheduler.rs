//! Scheduling strategies for node execution order.
//!
//! The [`Scheduler`] trait determines which candidate node should execute
//! next during a super-step. The executor uses a default round-robin
//! strategy when no [`SchedulerNode`](mcl_registry::nodes::scheduler::SchedulerNode)
//! is configured in the graph.

use crate::id::NodeId;

/// Selects the next node to execute from a set of candidates.
///
/// Implementations enforce fairness (e.g., starvation prevention),
/// priority ordering, or custom policies.
pub trait Scheduler: Send + Sync {
    /// Pick the next node from `candidates`.
    ///
    /// Returns `None` when no candidate is eligible (all disabled or
    /// the scheduler has no preference).
    fn next_node(&mut self, candidates: &[NodeId]) -> Option<NodeId>;
}

/// Default round-robin scheduler.
///
/// Cycles through candidates in insertion order. Used when no
/// [`SchedulerNode`](mcl_registry::nodes::scheduler::SchedulerNode) is
/// present in the graph — this preserves the existing executor behaviour.
#[derive(Debug, Default)]
pub struct RoundRobinScheduler {
    /// Index into the previous candidate list for cycling.
    last_index: usize,
}

impl RoundRobinScheduler {
    pub fn new() -> Self {
        Self { last_index: 0 }
    }
}

impl Scheduler for RoundRobinScheduler {
    fn next_node(&mut self, candidates: &[NodeId]) -> Option<NodeId> {
        if candidates.is_empty() {
            return None;
        }
        let idx = self.last_index % candidates.len();
        self.last_index = idx + 1;
        Some(candidates[idx].clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_robin_cycles_through_candidates() {
        let mut sched = RoundRobinScheduler::new();
        let ids: Vec<NodeId> = vec!["a", "b", "c"].into_iter().map(NodeId::new).collect();

        assert_eq!(sched.next_node(&ids), Some(NodeId::new("a")));
        assert_eq!(sched.next_node(&ids), Some(NodeId::new("b")));
        assert_eq!(sched.next_node(&ids), Some(NodeId::new("c")));
        assert_eq!(sched.next_node(&ids), Some(NodeId::new("a")));
    }

    #[test]
    fn round_robin_empty_candidates_returns_none() {
        let mut sched = RoundRobinScheduler::new();
        assert_eq!(sched.next_node(&[]), None);
    }

    #[test]
    fn round_robin_single_candidate() {
        let mut sched = RoundRobinScheduler::new();
        let ids: Vec<NodeId> = vec!["only"].into_iter().map(NodeId::new).collect();
        assert_eq!(sched.next_node(&ids), Some(NodeId::new("only")));
        assert_eq!(sched.next_node(&ids), Some(NodeId::new("only")));
    }
}
