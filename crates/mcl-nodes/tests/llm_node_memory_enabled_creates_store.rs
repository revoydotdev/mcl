use indexmap::IndexMap;
use mcl_graph::{NodeContext, NodeId};
use mcl_nodes::LlmNode;
use mcl_spec::value::MclValue;

/// With memory_enabled = true in config, LlmNode should have a memory store.
/// This test only checks config parsing — no real embeddings needed.
#[test]
fn llm_node_memory_is_some_when_enabled_in_config() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    let ctx = NodeContext::new(NodeId::new("llm"), config);
    let node = LlmNode::from_config(&ctx).expect("from_config should succeed");
    assert!(
        node.has_memory(),
        "memory should be Some when memory_enabled=true"
    );
}

/// Custom memory_capacity should be respected.
#[test]
fn llm_node_memory_uses_custom_capacity() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    config.insert("memory_capacity".into(), MclValue::Int(500));
    let ctx = NodeContext::new(NodeId::new("llm"), config);
    let node = LlmNode::from_config(&ctx).expect("from_config should succeed");
    assert_eq!(node.memory_capacity(), Some(500));
}

/// Default capacity when memory_enabled but no capacity specified.
#[test]
fn llm_node_memory_uses_default_capacity() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    let ctx = NodeContext::new(NodeId::new("llm"), config);
    let node = LlmNode::from_config(&ctx).expect("from_config should succeed");
    assert_eq!(node.memory_capacity(), Some(1000));
}
