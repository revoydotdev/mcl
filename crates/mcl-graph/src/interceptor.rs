//! Packet interception trait for signal registry integration.
//!
//! Defines [`PacketInterceptor`] for intercepting outbound packets that carry
//! signals, and [`InterceptorVerdict`] for the accept/reject/defer disposition.
//!
//! The executor calls the interceptor after draining outboxes but before
//! pushing packets to channels. Packets without signals pass through unaffected.

use crate::id::NodeId;
use crate::message::Packet;

/// Verdict returned by a packet interceptor.
#[derive(Debug, Clone)]
pub enum InterceptorVerdict {
    /// Accept the packet and forward it normally.
    ///
    /// `registry_ids` are attached to the packet's `meta` field.
    Accept {
        /// Hex-encoded registry IDs for accepted signals.
        registry_ids: Vec<String>,
    },
    /// Reject the packet. It will be dropped and an error packet synthesized
    /// back to the origin node.
    Reject { reason: String },
    /// Defer the packet. It is forwarded with `quarantined=true` in meta.
    Defer,
    /// Route the packet to a different channel with policy metadata attached.
    RouteTo {
        /// Target channel for the packet.
        channel: String,
        /// Policy ID that caused the reroute.
        policy_id: String,
        /// Effect flags from the policy (e.g., "replay_unsafe", "external(network)").
        effect_flags: Vec<String>,
    },
}

/// Trait for intercepting outbound packets that carry signals.
///
/// Implementations inspect a packet's signals and return a verdict.
/// The executor only invokes this trait for packets with non-empty signals.
///
/// # Lifecycle
/// - [`intercept`](PacketInterceptor::intercept) is called once per outbound packet with signals.
/// - [`on_super_step_end`](PacketInterceptor::on_super_step_end) is called at the end of each super-step.
/// - [`on_shutdown`](PacketInterceptor::on_shutdown) is called when the executor shuts down.
pub trait PacketInterceptor: Send + Sync {
    /// Inspect a packet's signals and return a verdict.
    ///
    /// `origin` is the node that emitted the packet.
    /// `turn` is the current super-step number.
    /// `channel` is the channel the packet is destined for.
    fn intercept(
        &self,
        packet: &Packet,
        origin: &NodeId,
        turn: u64,
        channel: &str,
    ) -> InterceptorVerdict;

    /// Called at the end of each super-step (e.g., for fsync).
    fn on_super_step_end(&self) {}

    /// Called when the executor shuts down (e.g., for final fsync).
    fn on_shutdown(&self) {}
}
