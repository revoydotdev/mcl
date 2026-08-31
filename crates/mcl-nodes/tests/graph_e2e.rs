use mcl_graph::{GraphDefParser, GraphExecutor, NodeFactory, NodeId, Packet, TomlGraphDefParser};
use mcl_llm::MockLlmBackend;
use mcl_nodes::{register_default_nodes, LlmNode};

/// Set CWD to workspace root so relative paths in graph.toml resolve.
fn chdir_workspace_root() {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR not set");
    let workspace_root = std::path::Path::new(&manifest_dir)
        .parent()
        .expect("expected parent (crates/)")
        .parent()
        .expect("expected grandparent (workspace root)");
    std::env::set_current_dir(workspace_root).expect("failed to set CWD to workspace root");
}

/// Build a factory with all default nodes except "llm", which uses a mock backend.
fn build_factory_with_mock_llm(responses: Vec<String>) -> NodeFactory {
    let mut factory = NodeFactory::new();
    register_default_nodes(&mut factory);

    factory.register_fn("llm", move |_def| {
        let backend = Box::new(MockLlmBackend::new(responses.clone()));
        Ok(Box::new(LlmNode::new(backend, String::new())) as Box<dyn mcl_graph::Node>)
    });

    factory
}

#[tokio::test]
async fn graph_builds_crypt_adventure() {
    chdir_workspace_root();

    let def = TomlGraphDefParser
        .parse_file(std::path::Path::new(
            "examples/crypt/crypt_adventure.graph.toml",
        ))
        .expect("failed to parse crypt_adventure graph definition");

    assert_eq!(def.name, "crypt_adventure");
    assert_eq!(
        def.nodes.len(),
        5,
        "expected 5 nodes in crypt_adventure graph"
    );
    assert_eq!(
        def.edges.len(),
        9,
        "expected 9 edges in crypt_adventure graph"
    );

    let factory = build_factory_with_mock_llm(vec![
        "MCL::noop()".to_string(),
        "You see a dusty room. MCL::noop()".to_string(),
    ]);

    let executor = GraphExecutor::build(&def, &factory)
        .await
        .expect("failed to build graph executor");

    assert_eq!(executor.node_ids().len(), 5, "executor should have 5 nodes");
}

#[tokio::test]
async fn graph_packets_flow_end_to_end() {
    chdir_workspace_root();

    let def = TomlGraphDefParser
        .parse_file(std::path::Path::new(
            "examples/crypt/crypt_adventure.graph.toml",
        ))
        .expect("failed to parse crypt_adventure graph definition");

    let factory = build_factory_with_mock_llm(vec![
        "MCL::noop()".to_string(),
        "You see a dusty room. MCL::noop()".to_string(),
    ]);

    let mut executor = GraphExecutor::build(&def, &factory)
        .await
        .expect("failed to build graph executor");

    // Process init outputs (game_engine emits system_prompt + display during init)
    executor
        .run_until_quiescent()
        .await
        .expect("quiescent after init should succeed");

    // Inject a user_input packet — the LLM node subscribes to this channel
    executor.inject(
        "user_input",
        Packet::text("user_input", NodeId::new("test"), 1, "look around"),
    );

    // Run a few steps to propagate: user_input → LLM → narrative → game_engine → display → client
    for i in 0..3 {
        let result = executor
            .step()
            .await
            .unwrap_or_else(|e| panic!("step {i} failed: {e}"));
        if !result.has_pending {
            break;
        }
    }
}
