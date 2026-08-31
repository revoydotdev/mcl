use tokio::sync::mpsc;

use mcl_graph::node::Node;
use mcl_graph::{NodeContext, NodeId, Packet};
use mcl_nodes::ws_client::WsBridgeNode;
use mcl_spec::value::MclValue;

#[test]
fn ws_bridge_node_reports_correct_capability() {
    let (tx, _) = mpsc::unbounded_channel();
    let (_, rx) = mpsc::unbounded_channel();
    let node = WsBridgeNode::new(rx, tx);

    let cap = node.capability();
    assert_eq!(cap.type_name, "ws_client");
    assert!(cap.subscriptions.iter().any(|c| c.name == "display"));
    assert!(cap.publications.iter().any(|c| c.name == "user_input"));
}

#[test]
fn ws_bridge_node_is_source() {
    let (tx, _) = mpsc::unbounded_channel();
    let (_, rx) = mpsc::unbounded_channel();
    let node = WsBridgeNode::new(rx, tx);

    assert!(node.is_source());
}

#[tokio::test]
async fn ws_bridge_node_init_succeeds() {
    let (tx, _) = mpsc::unbounded_channel();
    let (_, rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(rx, tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    let result = node.init(&mut ctx).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn ws_bridge_node_poll_source_returns_none_when_empty() {
    let (outbound_tx, _) = mpsc::unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    let result = node.poll_source(&mut ctx).await.unwrap();
    assert!(result.is_none());

    drop(inbound_tx);
}

#[tokio::test]
async fn ws_bridge_node_poll_source_emits_user_input() {
    let (outbound_tx, _) = mpsc::unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    inbound_tx.send("hello world".to_string()).unwrap();

    let pkt = node
        .poll_source(&mut ctx)
        .await
        .unwrap()
        .expect("should produce a packet");
    assert_eq!(pkt.channel, "user_input");
    assert_eq!(pkt.text_content(), Some("hello world"));
}

#[tokio::test]
async fn ws_bridge_node_poll_source_ignores_empty_input() {
    let (outbound_tx, _) = mpsc::unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    inbound_tx.send("   ".to_string()).unwrap();

    let result = node.poll_source(&mut ctx).await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn ws_bridge_node_poll_source_quit_returns_error() {
    let (outbound_tx, _) = mpsc::unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    inbound_tx.send("/quit".to_string()).unwrap();

    let result = node.poll_source(&mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("user quit"));
}

#[tokio::test]
async fn ws_bridge_node_process_sends_text_to_outbound() {
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
    let (_, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    let pkt = Packet::text("display", NodeId::new("engine"), 1, "You see a dark room.");
    node.process("display", &[pkt], &mut ctx).await.unwrap();

    let msg = outbound_rx.recv().await.unwrap();
    assert_eq!(msg, "You see a dark room.");
}

#[tokio::test]
async fn ws_bridge_node_process_sends_hint_to_outbound() {
    let (outbound_tx, mut outbound_rx) = mpsc::unbounded_channel();
    let (_, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    let mut payload = indexmap::IndexMap::new();
    payload.insert("text".into(), MclValue::String("description".into()));
    payload.insert("hint".into(), MclValue::String("try looking around".into()));
    let pkt = Packet::new("display", NodeId::new("engine"), 1, MclValue::Map(payload));

    node.process("display", &[pkt], &mut ctx).await.unwrap();

    let text_msg = outbound_rx.recv().await.unwrap();
    assert_eq!(text_msg, "description");
    let hint_msg = outbound_rx.recv().await.unwrap();
    assert!(hint_msg.contains("try looking around"));
}

#[tokio::test]
async fn ws_bridge_node_process_returns_error_on_disconnected_client() {
    let (outbound_tx, outbound_rx) = mpsc::unbounded_channel();
    let (_, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    drop(outbound_rx);

    let pkt = Packet::text("display", NodeId::new("engine"), 1, "test");
    let result = node.process("display", &[pkt], &mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("disconnected"));
}

#[tokio::test]
async fn ws_bridge_node_poll_source_returns_error_when_channel_closed() {
    let (outbound_tx, _) = mpsc::unbounded_channel();
    let (inbound_tx, inbound_rx) = mpsc::unbounded_channel();
    let mut node = WsBridgeNode::new(inbound_rx, outbound_tx);
    let mut ctx = NodeContext::new(NodeId::new("ws_client"), Default::default());

    node.init(&mut ctx).await.unwrap();

    drop(inbound_tx);

    let result = node.poll_source(&mut ctx).await;
    assert!(result.is_err());
    assert!(result.unwrap_err().to_string().contains("disconnected"));
}
