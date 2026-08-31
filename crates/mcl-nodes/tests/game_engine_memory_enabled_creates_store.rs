use indexmap::IndexMap;
use mcl_graph::{NodeContext, NodeId};
use mcl_nodes::GameEngineNode;
use mcl_spec::value::MclValue;

/// With memory_enabled = true in config, GameEngineNode should have a memory store.
/// This test only checks config parsing — no real embeddings needed.
#[test]
fn game_engine_memory_is_some_when_enabled() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    let ctx = NodeContext::new(NodeId::new("game_engine"), config);
    let node = GameEngineNode::from_config(&ctx);
    assert!(
        node.has_memory(),
        "memory should be Some when memory_enabled=true"
    );
}

/// Custom memory_capacity should be respected.
#[test]
fn game_engine_memory_uses_custom_capacity() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    config.insert("memory_capacity".into(), MclValue::Int(200));
    let ctx = NodeContext::new(NodeId::new("game_engine"), config);
    let node = GameEngineNode::from_config(&ctx);
    assert_eq!(node.memory_capacity(), Some(200));
}

/// Default capacity when memory_enabled but no capacity specified.
#[test]
fn game_engine_memory_uses_default_capacity() {
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    let ctx = NodeContext::new(NodeId::new("game_engine"), config);
    let node = GameEngineNode::from_config(&ctx);
    assert_eq!(node.memory_capacity(), Some(1000));
}
