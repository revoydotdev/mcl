pub mod combat;
pub mod inventory;
#[cfg(test)]
mod tests;

pub use combat::CombatResolver;
pub use inventory::InventoryResolver;

/// Register all RPG resolvers
pub fn register_rpg_resolvers(registry: &mut mcl_runtime::ResolverRegistry) {
    registry.register(Box::new(CombatResolver));
    registry.register(Box::new(InventoryResolver));
}
