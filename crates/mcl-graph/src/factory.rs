use std::collections::HashMap;

use crate::error::GraphError;
use crate::graph_def::NodeDef;
use crate::node::Node;

/// Factory function type for creating nodes from definitions
pub type NodeConstructor = Box<dyn Fn(&NodeDef) -> Result<Box<dyn Node>, GraphError> + Send + Sync>;

/// Registry of node type constructors
pub struct NodeFactory {
    constructors: HashMap<String, NodeConstructor>,
}

impl NodeFactory {
    pub fn new() -> Self {
        Self {
            constructors: HashMap::new(),
        }
    }

    /// Register a constructor for a node type name
    pub fn register(&mut self, type_name: &str, ctor: NodeConstructor) {
        self.constructors.insert(type_name.to_string(), ctor);
    }

    /// Register with a closure (convenience)
    pub fn register_fn<F>(&mut self, type_name: &str, f: F)
    where
        F: Fn(&NodeDef) -> Result<Box<dyn Node>, GraphError> + Send + Sync + 'static,
    {
        self.constructors.insert(type_name.to_string(), Box::new(f));
    }

    /// Create a node from its definition
    pub fn create(&self, def: &NodeDef) -> Result<Box<dyn Node>, GraphError> {
        let ctor = self
            .constructors
            .get(&def.node_type)
            .ok_or_else(|| GraphError::UnknownNodeType(def.node_type.clone()))?;
        ctor(def)
    }

    /// Check if a node type is registered
    pub fn has(&self, type_name: &str) -> bool {
        self.constructors.contains_key(type_name)
    }

    /// List all registered node type names
    pub fn registered_types(&self) -> Vec<&str> {
        self.constructors.keys().map(|s| s.as_str()).collect()
    }
}

impl Default for NodeFactory {
    fn default() -> Self {
        Self::new()
    }
}
