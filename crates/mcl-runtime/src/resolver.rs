use std::collections::HashMap;

use indexmap::IndexMap;
use mcl_spec::document::ActionDef;
use mcl_spec::signal::{MclSignal, Resolution, ResolutionOutcome};
use mcl_spec::MclError;
use rand::RngCore;

use crate::state::WorldState;

/// Trait for domain-specific action resolution
pub trait ActionResolver: Send + Sync {
    /// The resolution system name (e.g., "combat_system")
    fn system_name(&self) -> &str;

    /// Resolve an action, returning the resolution result
    fn resolve(
        &self,
        action: &ActionDef,
        signal: &MclSignal,
        state: &WorldState,
        rng: &mut dyn RngCore,
    ) -> Result<Resolution, MclError>;
}

/// Registry for action resolvers
pub struct ResolverRegistry {
    resolvers: HashMap<String, Box<dyn ActionResolver>>,
}

impl ResolverRegistry {
    pub fn new() -> Self {
        Self {
            resolvers: HashMap::new(),
        }
    }

    pub fn register(&mut self, resolver: Box<dyn ActionResolver>) {
        let name = resolver.system_name().to_string();
        self.resolvers.insert(name, resolver);
    }

    pub fn get(&self, system_name: &str) -> Option<&dyn ActionResolver> {
        self.resolvers.get(system_name).map(|r| r.as_ref())
    }

    pub fn has(&self, system_name: &str) -> bool {
        self.resolvers.contains_key(system_name)
    }
}

impl Default for ResolverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Default resolver for actions with direct `mutates` (no resolution system)
pub fn resolve_mutates(action: &ActionDef, state: &mut WorldState) -> Result<Resolution, MclError> {
    state.apply_mutations(&action.mutates)?;

    Ok(Resolution {
        action: action.name.clone(),
        outcome: ResolutionOutcome::Success,
        details: IndexMap::new(),
        narration_hint: None,
    })
}
