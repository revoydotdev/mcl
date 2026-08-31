use indexmap::IndexMap;
use mcl_graph::lifecycle::MemoryPopulationHook;
use mcl_graph::{LifecycleHook, Node, NodeContext, NodeId, Packet};
use mcl_nodes::LlmNode;
use mcl_spec::value::MclValue;

fn setup_mock_embedding() {
    std::env::set_var("MCL_MOCK_EMBEDDING", "1");
}

#[tokio::test]
async fn llm_node_memory_stores_incoming_user_input() {
    setup_mock_embedding();
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    config.insert("backend".into(), MclValue::String("mock".into()));
    config.insert(
        "mock_responses".into(),
        MclValue::String("You look around and see stone walls.".into()),
    );
    let config_ctx = NodeContext::new(NodeId::new("llm"), config);
    let mut node = LlmNode::from_config(&config_ctx).expect("from_config should succeed");

    let mut init_ctx = NodeContext::new(NodeId::new("llm"), IndexMap::new());
    node.init(&mut init_ctx).await.expect("init should succeed");

    let mut proc_ctx = NodeContext::new(NodeId::new("llm"), IndexMap::new());
    proc_ctx.set_memory(init_ctx.take_memory());

    let pkt = Packet::text(
        "user_input",
        NodeId::new("test"),
        1,
        "I look around the room",
    );

    let hook = MemoryPopulationHook::new();
    hook.on_packet_in("llm", "user_input", &pkt, &mut proc_ctx);

    node.process("user_input", &[pkt], &mut proc_ctx)
        .await
        .expect("process should succeed");

    let mem = proc_ctx.memory().expect("memory should exist on ctx");
    assert!(
        !mem.incoming.is_empty(),
        "incoming memory should have entries after hook fires"
    );
}

#[tokio::test]
async fn llm_node_memory_stores_outgoing_narrative() {
    setup_mock_embedding();
    let mut config = IndexMap::new();
    config.insert("memory_enabled".into(), MclValue::Bool(true));
    config.insert("backend".into(), MclValue::String("mock".into()));
    config.insert(
        "mock_responses".into(),
        MclValue::String("You attack the goblin with your sword.".into()),
    );
    let config_ctx = NodeContext::new(NodeId::new("llm"), config);
    let mut node = LlmNode::from_config(&config_ctx).expect("from_config should succeed");

    let mut init_ctx = NodeContext::new(NodeId::new("llm"), IndexMap::new());
    node.init(&mut init_ctx).await.expect("init should succeed");

    let pkt = Packet::text("user_input", NodeId::new("test"), 1, "attack the goblin");
    let mut proc_ctx = NodeContext::new(NodeId::new("llm"), IndexMap::new());

    proc_ctx.set_memory(init_ctx.take_memory());

    node.process("user_input", &[pkt], &mut proc_ctx)
        .await
        .expect("process should succeed");

    let hook = MemoryPopulationHook::new();
    for (channel, out_pkt) in proc_ctx.drain_outbox() {
        hook.on_packet_out("llm", &channel, &out_pkt, &mut proc_ctx);
    }

    let mem = proc_ctx.memory().expect("memory should exist on ctx");
    assert!(
        !mem.outgoing.is_empty(),
        "outgoing memory should have entries after hook fires on outgoing packets"
    );
}
