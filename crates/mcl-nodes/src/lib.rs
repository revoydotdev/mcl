//! Concrete nodes: Client, LLM, GameEngine, Combat, and Inventory.
//!
//! Provides ready-to-use node implementations for common MCL patterns.

pub mod client;
pub mod combat;
pub mod game_engine;
pub mod inventory;
pub mod llm;
pub mod player_llm;
pub mod scripted_player;
pub mod ws_client;

pub use client::ClientNode;
pub use combat::CombatNode;
pub use game_engine::GameEngineNode;
pub use inventory::InventoryNode;
pub use llm::LlmNode;
pub use player_llm::PlayerLlmNode;
pub use scripted_player::ScriptedPlayerNode;
pub use ws_client::WsBridgeNode;

/// Register all default node types with a NodeFactory
pub fn register_default_nodes(factory: &mut mcl_graph::NodeFactory) {
    factory.register_fn("client", |_def| Ok(Box::new(ClientNode)));

    factory.register_fn("llm", |def| {
        let temp_ctx =
            mcl_graph::NodeContext::new(mcl_graph::NodeId::new("llm"), def.config.clone());
        LlmNode::from_config(&temp_ctx).map(|n| Box::new(n) as Box<dyn mcl_graph::Node>)
    });

    factory.register_fn("game_engine", |def| {
        let temp_ctx =
            mcl_graph::NodeContext::new(mcl_graph::NodeId::new("game_engine"), def.config.clone());
        Ok(Box::new(GameEngineNode::from_config(&temp_ctx)))
    });

    factory.register_fn("combat_resolver", |_def| Ok(Box::new(CombatNode::new())));

    factory.register_fn("inventory_resolver", |_def| Ok(Box::new(InventoryNode)));

    factory.register_fn("scripted_player", |def| {
        let temp_ctx = mcl_graph::NodeContext::new(
            mcl_graph::NodeId::new("scripted_player"),
            def.config.clone(),
        );
        Ok(Box::new(ScriptedPlayerNode::from_config(&temp_ctx)))
    });

    factory.register_fn("player_llm", |def| {
        let temp_ctx =
            mcl_graph::NodeContext::new(mcl_graph::NodeId::new("player_llm"), def.config.clone());
        PlayerLlmNode::from_config(&temp_ctx).map(|n| Box::new(n) as Box<dyn mcl_graph::Node>)
    });

    factory.register_fn("ws_client", |_def| {
        // WsBridgeNode requires channels that the server provides;
        // this registration is for graph-def compatibility only.
        // The server replaces this node with a properly-wired WsBridgeNode.
        Err(mcl_graph::GraphError::NodeError(
            "ws_client must be created via WsBridgeNode::new(), not from graph def".into(),
        ))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_graph::graph_def::NodeDef;

    fn make_def(node_type: &str) -> NodeDef {
        NodeDef {
            node_type: node_type.to_string(),
            config: Default::default(),
            priority: 0,
        }
    }

    #[test]
    fn register_default_nodes_registers_all_eight_types() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);

        assert!(factory.has("client"), "missing 'client'");
        assert!(factory.has("llm"), "missing 'llm'");
        assert!(factory.has("game_engine"), "missing 'game_engine'");
        assert!(factory.has("combat_resolver"), "missing 'combat_resolver'");
        assert!(
            factory.has("inventory_resolver"),
            "missing 'inventory_resolver'"
        );
        assert!(factory.has("ws_client"), "missing 'ws_client'");
        assert!(factory.has("scripted_player"), "missing 'scripted_player'");
        assert!(factory.has("player_llm"), "missing 'player_llm'");

        let registered = factory.registered_types();
        assert_eq!(registered.len(), 8, "expected exactly 8 registered types");
    }

    #[test]
    fn factory_creates_each_node_type_successfully() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);

        for type_name in &[
            "client",
            "llm",
            "game_engine",
            "combat_resolver",
            "inventory_resolver",
            "scripted_player",
        ] {
            let node = factory.create(&make_def(type_name));
            assert!(
                node.is_ok(),
                "failed to create node type '{type_name}': {:?}",
                node.err()
            );
        }
    }

    #[test]
    fn client_node_reports_correct_capability() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);
        let node = factory.create(&make_def("client")).unwrap();
        let cap = node.capability();
        assert_eq!(cap.type_name, "client");
        assert!(cap.subscriptions.iter().any(|c| c.name == "display"));
        assert!(cap.publications.iter().any(|c| c.name == "user_input"));
    }

    #[test]
    fn game_engine_node_reports_correct_capability() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);
        let node = factory.create(&make_def("game_engine")).unwrap();
        let cap = node.capability();
        assert_eq!(cap.type_name, "game_engine");
        assert!(cap.subscriptions.iter().any(|c| c.name == "narrative"));
        assert!(cap.publications.iter().any(|c| c.name == "display"));
    }

    #[test]
    fn combat_node_reports_correct_capability() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);
        let node = factory.create(&make_def("combat_resolver")).unwrap();
        let cap = node.capability();
        assert_eq!(cap.type_name, "combat_resolver");
        assert!(cap.subscriptions.iter().any(|c| c.name == "combat_action"));
        assert!(cap.publications.iter().any(|c| c.name == "resolution"));
    }

    #[test]
    fn inventory_node_reports_correct_capability() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);
        let node = factory.create(&make_def("inventory_resolver")).unwrap();
        let cap = node.capability();
        assert_eq!(cap.type_name, "inventory_resolver");
        assert!(cap
            .subscriptions
            .iter()
            .any(|c| c.name == "inventory_action"));
        assert!(cap.publications.iter().any(|c| c.name == "resolution"));
    }

    #[test]
    fn llm_node_reports_correct_capability() {
        let mut factory = mcl_graph::NodeFactory::new();
        register_default_nodes(&mut factory);
        let node = factory.create(&make_def("llm")).unwrap();
        let cap = node.capability();
        assert_eq!(cap.type_name, "llm");
        assert!(cap.subscriptions.iter().any(|c| c.name == "user_input"));
        assert!(cap.publications.iter().any(|c| c.name == "narrative"));
    }

    #[test]
    fn unregistered_type_returns_error() {
        let factory = mcl_graph::NodeFactory::new();
        let result = factory.create(&make_def("nonexistent"));
        assert!(result.is_err());
    }
}
