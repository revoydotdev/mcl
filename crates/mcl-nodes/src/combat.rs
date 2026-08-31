use async_trait::async_trait;
use indexmap::IndexMap;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};
use mcl_spec::value::MclValue;
use rand::rngs::StdRng;
use rand::Rng;
use rand::RngCore;
use rand::SeedableRng;
use std::sync::Mutex;

/// Combat resolver node: d20-style attack resolution.
/// Receives action packets, rolls dice, emits resolution packets.
pub struct CombatNode {
    rng: Mutex<Box<dyn RngCore + Send>>,
}

impl Default for CombatNode {
    fn default() -> Self {
        Self::new()
    }
}

impl CombatNode {
    /// Create a new CombatNode with an entropy-seeded RNG.
    pub fn new() -> Self {
        CombatNode {
            rng: Mutex::new(Box::new(StdRng::from_entropy())),
        }
    }

    /// Create a new CombatNode with a seeded RNG for deterministic behavior.
    pub fn with_seed(seed: u64) -> Self {
        CombatNode {
            rng: Mutex::new(Box::new(StdRng::seed_from_u64(seed))),
        }
    }
}

#[async_trait]
impl Node for CombatNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "combat_resolver".into(),
            description: "d20-style combat resolution".into(),
            subscriptions: vec![ChannelSpec::new("combat_action")],
            publications: vec![ChannelSpec::new("resolution")],
        }
    }

    async fn init(&mut self, ctx: &mut NodeContext) -> Result<(), GraphError> {
        if let Some(seed_value) = ctx.config.get("rng_seed").and_then(|v| v.as_int()) {
            if seed_value >= 0 {
                let seed = seed_value as u64;
                *self.rng.lock().expect("rng mutex poisoned") =
                    Box::new(StdRng::seed_from_u64(seed));
            }
        }
        Ok(())
    }

    async fn process(
        &mut self,
        _channel: &str,
        packets: &[Packet],
        ctx: &mut NodeContext,
    ) -> Result<(), GraphError> {
        for pkt in packets {
            let map = match pkt.payload.as_map() {
                Some(m) => m,
                None => continue,
            };

            let signal_str = map.get("signal").and_then(|v| v.as_str()).unwrap_or("");

            let target = extract_kwarg(signal_str, "target").unwrap_or("enemy".into());

            let attack_bonus = map
                .get("attack_bonus")
                .or_else(|| map.get("player_attack"))
                .and_then(|v| v.as_int())
                .unwrap_or(5);

            let target_ac = map
                .get("target_ac")
                .or_else(|| map.get("ac"))
                .and_then(|v| v.as_int())
                .unwrap_or(12);

            let roll: i64 = self
                .rng
                .lock()
                .expect("rng mutex poisoned")
                .gen_range(1..=20);
            let total = roll + attack_bonus;
            let hit = total >= target_ac;

            let mut result = IndexMap::new();
            result.insert("roll".into(), MclValue::Int(roll));
            result.insert("total".into(), MclValue::Int(total));
            result.insert("target".into(), MclValue::String(target.clone()));

            if hit {
                let damage: i64 = self
                    .rng
                    .lock()
                    .expect("rng mutex poisoned")
                    .gen_range(1..=6);
                result.insert("outcome".into(), MclValue::String("hit".into()));
                result.insert("damage".into(), MclValue::Int(damage));
                result.insert(
                    "hint".into(),
                    MclValue::String(format!(
                        "Attack hits {target} for {damage} damage! (rolled {roll}+{attack_bonus}={total} vs AC {target_ac})"
                    )),
                );
            } else {
                result.insert("outcome".into(), MclValue::String("miss".into()));
                result.insert(
                    "hint".into(),
                    MclValue::String(format!(
                        "Attack misses {target}. (rolled {roll}+{attack_bonus}={total} vs AC {target_ac})"
                    )),
                );
            }

            ctx.emit("resolution", MclValue::Map(result));
        }

        Ok(())
    }
}

fn extract_kwarg(signal: &str, key: &str) -> Option<String> {
    let pattern = format!("{key}=");
    let start = signal.find(&pattern)? + pattern.len();
    let rest = &signal[start..];
    let end = rest.find([',', ')']).unwrap_or(rest.len());
    Some(rest[..end].trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use indexmap::IndexMap;
    use mcl_graph::{Node, NodeContext, Packet};
    use mcl_spec::value::MclValue;

    fn make_packet(payload: MclValue) -> Packet {
        Packet::new("test_channel", mcl_graph::NodeId::new("test"), 1, payload)
    }

    fn make_attack_packet(
        signal: Option<&str>,
        attack_bonus: Option<i64>,
        target_ac: Option<i64>,
    ) -> Packet {
        let mut map = IndexMap::new();
        if let Some(s) = signal {
            map.insert("signal".into(), MclValue::String(s.into()));
        }
        if let Some(ab) = attack_bonus {
            map.insert("attack_bonus".into(), MclValue::Int(ab));
        }
        if let Some(ac) = target_ac {
            map.insert("target_ac".into(), MclValue::Int(ac));
        }
        make_packet(MclValue::Map(map))
    }

    fn test_context() -> NodeContext {
        NodeContext::new(mcl_graph::NodeId::new("test"), IndexMap::new())
    }

    fn get_resolution_packets(ctx: &mut NodeContext) -> Vec<Packet> {
        ctx.drain_outbox()
            .into_iter()
            .filter(|(channel, _)| channel == "resolution")
            .map(|(_, packet)| packet)
            .collect()
    }

    #[tokio::test]
    async fn seeded_rng_produces_identical_results() {
        let packet = make_attack_packet(Some("attack"), Some(5), Some(12));

        let mut node_1 = CombatNode::with_seed(42);
        let mut ctx_1 = test_context();

        let mut node_2 = CombatNode::with_seed(42);
        let mut ctx_2 = test_context();

        node_1
            .process("combat_action", std::slice::from_ref(&packet), &mut ctx_1)
            .await
            .unwrap();

        node_2
            .process("combat_action", &[packet], &mut ctx_2)
            .await
            .unwrap();

        let emitted_1 = get_resolution_packets(&mut ctx_1);
        let output_1 = emitted_1
            .first()
            .and_then(|p| p.payload.as_map())
            .expect("resolution packet should be a map");

        let emitted_2 = get_resolution_packets(&mut ctx_2);
        let output_2 = emitted_2
            .first()
            .and_then(|p| p.payload.as_map())
            .expect("resolution packet should be a map");

        let result_1 = (
            output_1.get("roll").and_then(|v| v.as_int()),
            output_1.get("total").and_then(|v| v.as_int()),
            output_1.get("outcome").and_then(|v| v.as_str()),
            output_1.get("damage").and_then(|v| v.as_int()),
        );

        let result_2 = (
            output_2.get("roll").and_then(|v| v.as_int()),
            output_2.get("total").and_then(|v| v.as_int()),
            output_2.get("outcome").and_then(|v| v.as_str()),
            output_2.get("damage").and_then(|v| v.as_int()),
        );

        assert_eq!(
            result_1, result_2,
            "Seeded RNG should produce identical results"
        );
    }

    #[tokio::test]
    async fn default_rng_is_non_deterministic_across_instances() {
        let packet = make_attack_packet(Some("attack"), Some(5), Some(12));

        let mut node_1 = CombatNode::new();
        let mut ctx_1 = test_context();

        let mut node_2 = CombatNode::new();
        let mut ctx_2 = test_context();

        node_1
            .process("combat_action", std::slice::from_ref(&packet), &mut ctx_1)
            .await
            .unwrap();

        node_2
            .process("combat_action", &[packet], &mut ctx_2)
            .await
            .unwrap();

        let emitted_1 = get_resolution_packets(&mut ctx_1);
        let output_1 = emitted_1
            .first()
            .and_then(|p| p.payload.as_map())
            .expect("resolution packet should be a map");

        let emitted_2 = get_resolution_packets(&mut ctx_2);
        let output_2 = emitted_2
            .first()
            .and_then(|p| p.payload.as_map())
            .expect("resolution packet should be a map");

        let roll_1 = output_1.get("roll").and_then(|v| v.as_int());
        let roll_2 = output_2.get("roll").and_then(|v| v.as_int());

        if roll_1 == roll_2 {
            let damage_1 = output_1.get("damage");
            let damage_2 = output_2.get("damage");
            if damage_1.is_some() && damage_2.is_some() {
                assert_ne!(
                    damage_1, damage_2,
                    "Default RNG should not produce identical results"
                );
            }
        }
    }
}
