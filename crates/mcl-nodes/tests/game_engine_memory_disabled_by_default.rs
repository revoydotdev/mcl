use mcl_nodes::GameEngineNode;

/// Without memory_enabled in config, GameEngineNode should have no memory.
#[test]
fn game_engine_memory_is_none_without_config() {
    let node = GameEngineNode::new();
    assert!(!node.has_memory(), "memory should be None by default");
}

/// GameEngineNode::default() should also have no memory.
#[test]
fn game_engine_default_has_no_memory() {
    let node = GameEngineNode::default();
    assert!(!node.has_memory(), "default() should not enable memory");
}
