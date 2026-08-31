use indexmap::IndexMap;
use mcl_graph::{NodeContext, NodeId};
use mcl_llm::MockLlmBackend;
use mcl_nodes::LlmNode;
use mcl_spec::value::MclValue;

/// Without memory_enabled in config, LlmNode should have no memory store.
#[test]
fn llm_node_memory_is_none_without_config() {
    let config = IndexMap::new();
    let ctx = NodeContext::new(NodeId::new("llm"), config);
    let node = LlmNode::from_config(&ctx).expect("from_config should succeed");
    assert!(!node.has_memory(), "memory should be None by default");
}

/// LlmNode::new() constructor should also have no memory.
#[test]
fn llm_node_new_has_no_memory() {
    let backend = Box::new(MockLlmBackend::new(vec!["response".into()]));
    let node = LlmNode::new(backend, "prompt".into());
    assert!(!node.has_memory(), "new() should not enable memory");
}

/// Explicitly setting memory_enabled = false should not enable memory.
#[test]
fn llm_node_memory_false_in_config() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(false));
    let ctx = NodeContext::new(NodeId::new("llm"), config);
    let node = LlmNode::from_config(&ctx).expect("from_config should succeed");
    assert!(
        !node.has_memory(),
        "memory_enabled=false should not enable memory"
    );
}
