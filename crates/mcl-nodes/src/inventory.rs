use async_trait::async_trait;
use indexmap::IndexMap;
use mcl_graph::node::{ChannelSpec, NodeCapability};
use mcl_graph::{GraphError, Node, NodeContext, Packet};
use mcl_spec::value::MclValue;

/// Inventory resolver node: handles item pickup/drop.
pub struct InventoryNode;

#[async_trait]
impl Node for InventoryNode {
    fn capability(&self) -> NodeCapability {
        NodeCapability {
            type_name: "inventory_resolver".into(),
            description: "inventory management (pickup, drop)".into(),
            subscriptions: vec![ChannelSpec::new("inventory_action")],
            publications: vec![ChannelSpec::new("resolution")],
        }
    }

    async fn init(&mut self, _ctx: &mut NodeContext) -> Result<(), GraphError> {
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

            let action_name = map
                .get("action")
                .and_then(|v| v.as_str())
                .unwrap_or("pickup");

            let signal_str = map.get("signal").and_then(|v| v.as_str()).unwrap_or("");

            let target = extract_kwarg(signal_str, "target").unwrap_or("item".into());

            let mut result = IndexMap::new();
            result.insert("action".into(), MclValue::String(action_name.to_string()));
            result.insert("item".into(), MclValue::String(target.clone()));

            match action_name {
                "pickup" | "take" | "grab" => {
                    result.insert("outcome".into(), MclValue::String("success".into()));
                    result.insert(
                        "hint".into(),
                        MclValue::String(format!("You pick up the {target}.")),
                    );
                }
                "drop" | "discard" => {
                    result.insert("outcome".into(), MclValue::String("success".into()));
                    result.insert(
                        "hint".into(),
                        MclValue::String(format!("You drop the {target}.")),
                    );
                }
                _ => {
                    result.insert("outcome".into(), MclValue::String("failure".into()));
                    result.insert(
                        "hint".into(),
                        MclValue::String(format!("Unknown inventory action: {action_name}")),
                    );
                }
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
