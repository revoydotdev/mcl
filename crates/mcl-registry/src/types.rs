//! Core types for the signal registry.
//!
//! Defines [`SignalLogEntry`] (the unit of the append-only log),
//! [`Verdict`] (accept/reject/defer disposition), and [`SignalId`]
//! (blake3 content-addressed identifier).

use mcl_graph::id::NodeId;
use mcl_spec::signal::MclSignal;
use serde::{Deserialize, Serialize};

/// Content-addressed identifier for a signal log entry.
///
/// Hashes origin + turn + channel + signal to produce a deterministic ID.
/// Uses blake3 internally. The `hash` field is the raw 32-byte digest.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct SignalId {
    pub hash: [u8; 32],
}

impl SignalId {
    /// Compute a deterministic hash from the signal's context.
    ///
    /// Hashes: origin node, turn number, channel name, and serialized signal.
    pub fn from_signal(origin: &NodeId, turn: u64, channel: &str, signal: &MclSignal) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(origin.as_str().as_bytes());
        hasher.update(&turn.to_le_bytes());
        hasher.update(channel.as_bytes());
        // Serialize the signal for hashing — use bincode-style manual bytes
        // to avoid depending on signal's Display impl format.
        let signal_bytes =
            serde_json::to_vec(signal).expect("MclSignal serialization should not fail");
        hasher.update(&signal_bytes);
        Self {
            hash: *hasher.finalize().as_bytes(),
        }
    }

    /// Parse a hex-encoded string back into a SignalId.
    ///
    /// Returns an error if the string is not exactly 64 hex characters.
    pub fn from_hex(hex_str: &str) -> Result<Self, String> {
        let hex_str = hex_str.trim();
        if hex_str.len() != 64 {
            return Err(format!(
                "invalid hex string length: expected 64, got {}",
                hex_str.len()
            ));
        }

        let mut hash = [0u8; 32];
        for (i, byte) in hash.iter_mut().enumerate() {
            let byte_str = &hex_str[i * 2..i * 2 + 2];
            *byte = u8::from_str_radix(byte_str, 16)
                .map_err(|e| format!("invalid hex at position {}: {}", i, e))?;
        }

        Ok(Self { hash })
    }

    /// Hex-encoded string of the hash (lowercase, 64 chars).
    pub fn hex(&self) -> String {
        hex::encode(self.hash)
    }
}

// Inline hex encoding to avoid adding the `hex` crate dependency.
mod hex {
    const HEX_CHARS: &[u8; 16] = b"0123456789abcdef";

    pub fn encode(bytes: [u8; 32]) -> String {
        let mut s = String::with_capacity(64);
        for &b in &bytes {
            s.push(HEX_CHARS[(b >> 4) as usize] as char);
            s.push(HEX_CHARS[(b & 0x0f) as usize] as char);
        }
        s
    }
}

/// Disposition applied to a signal by the registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Verdict {
    /// Signal accepted and applied to world state.
    Accept,
    /// Signal rejected with a reason.
    Reject { reason: String },
    /// Signal deferred for later arbitration.
    Defer,
    /// Signal routed to a specific channel by policy.
    RouteTo { channel: String, policy_id: String },
}

/// A single entry in the append-only signal log.
///
/// Each entry records a signal's identity, origin, and the verdict
/// that was applied to it. When a policy fired and produced a
/// non-Accept verdict, [`policy_bindings`] carries the matched
/// expression environment so an auditor can later reconstruct *why*
/// the policy fired (R2 §2.3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignalLogEntry {
    /// Content-addressed identifier.
    pub id: SignalId,
    /// The signal itself.
    pub signal: MclSignal,
    /// Which node emitted the signal.
    pub origin: NodeId,
    /// The super-step / turn number when the signal was emitted.
    pub turn: u64,
    /// The channel the signal was emitted on.
    pub channel: String,
    /// Monotonic timestamp (milliseconds since UNIX_EPOCH).
    pub timestamp: u64,
    /// The verdict applied to this signal.
    pub verdict: Verdict,
    /// Bindings captured from the matched policy's expression environment
    /// at the moment of violation. `None` when no policy fired or the
    /// verdict was `Accept`. Older log entries deserialize with `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy_bindings: Option<serde_json::Value>,
}

/// Policy controlling when the signal log calls `fsync`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FsyncPolicy {
    /// Call `fsync` after every append.
    EveryStep,
    /// Call `fsync` every N steps.
    EveryNSteps(u64),
    /// Only call `fsync` on shutdown / drop.
    OnShutdown,
}

impl Default for FsyncPolicy {
    fn default() -> Self {
        Self::EveryNSteps(100)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_spec::label::LabelSet;
    use mcl_spec::span::Span;

    fn test_signal(name: &str) -> MclSignal {
        MclSignal {
            kind: mcl_spec::signal::SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        }
    }

    #[test]
    fn signal_id_is_deterministic() {
        let origin = NodeId::new("player");
        let sig = test_signal("attack");

        let id1 = SignalId::from_signal(&origin, 1, "actions", &sig);
        let id2 = SignalId::from_signal(&origin, 1, "actions", &sig);

        assert_eq!(id1, id2, "same inputs must produce same hash");
    }

    #[test]
    fn signal_id_differs_by_turn() {
        let origin = NodeId::new("player");
        let sig = test_signal("attack");

        let id1 = SignalId::from_signal(&origin, 1, "actions", &sig);
        let id2 = SignalId::from_signal(&origin, 2, "actions", &sig);

        assert_ne!(id1, id2, "different turns must produce different hashes");
    }

    #[test]
    fn signal_id_differs_by_channel() {
        let origin = NodeId::new("player");
        let sig = test_signal("attack");

        let id1 = SignalId::from_signal(&origin, 1, "actions", &sig);
        let id2 = SignalId::from_signal(&origin, 1, "internal", &sig);

        assert_ne!(id1, id2, "different channels must produce different hashes");
    }

    #[test]
    fn signal_id_hex_is_64_chars() {
        let id = SignalId::from_signal(&NodeId::new("a"), 0, "ch", &test_signal("x"));
        let hex = id.hex();
        assert_eq!(hex.len(), 64);
        assert!(hex.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn signal_id_differs_by_origin() {
        let sig = test_signal("attack");

        let id1 = SignalId::from_signal(&NodeId::new("player"), 1, "actions", &sig);
        let id2 = SignalId::from_signal(&NodeId::new("guard"), 1, "actions", &sig);

        assert_ne!(id1, id2, "different origins must produce different hashes");
    }

    #[test]
    fn signal_id_differs_by_signal() {
        let origin = NodeId::new("player");
        let sig1 = test_signal("attack");
        let sig2 = test_signal("defend");

        let id1 = SignalId::from_signal(&origin, 1, "actions", &sig1);
        let id2 = SignalId::from_signal(&origin, 1, "actions", &sig2);

        assert_ne!(id1, id2, "different signals must produce different hashes");
    }

    #[test]
    fn verdict_serde_roundtrip() {
        let cases = vec![
            Verdict::Accept,
            Verdict::Reject {
                reason: "not allowed".to_string(),
            },
            Verdict::Defer,
        ];

        for v in cases {
            let json = serde_json::to_string(&v).expect("serialize");
            let back: Verdict = serde_json::from_str(&json).expect("deserialize");
            assert_eq!(v, back);
        }
    }

    #[test]
    fn signal_log_entry_serde_roundtrip() {
        let entry = SignalLogEntry {
            id: SignalId::from_signal(&NodeId::new("player"), 1, "actions", &test_signal("attack")),
            signal: test_signal("attack"),
            origin: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            timestamp: 1714000000000,
            verdict: Verdict::Accept,
            policy_bindings: None,
        };

        let json = serde_json::to_string(&entry).expect("serialize");
        let back: SignalLogEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(entry.id, back.id);
        assert_eq!(entry.origin, back.origin);
        assert_eq!(entry.turn, back.turn);
        assert_eq!(entry.channel, back.channel);
        assert_eq!(entry.verdict, back.verdict);
        assert_eq!(entry.policy_bindings, back.policy_bindings);
    }

    #[test]
    fn signal_log_entry_with_policy_bindings_roundtrip() {
        let bindings = serde_json::json!({
            "policy_id": "spend_limit",
            "signal.kind": "transfer",
            "signal.args.amount": 99999,
            "matched": true
        });
        let entry = SignalLogEntry {
            id: SignalId::from_signal(
                &NodeId::new("player"),
                1,
                "actions",
                &test_signal("transfer"),
            ),
            signal: test_signal("transfer"),
            origin: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            timestamp: 1714000000000,
            verdict: Verdict::Reject {
                reason: "spend_limit: amount exceeds daily_limit".to_string(),
            },
            policy_bindings: Some(bindings.clone()),
        };

        let json = serde_json::to_string(&entry).expect("serialize");
        let back: SignalLogEntry = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(back.policy_bindings, Some(bindings));
    }

    #[test]
    fn signal_log_entry_old_format_deserializes_with_none_bindings() {
        // Older log entries written before R2 audit-bindings landed must still
        // deserialize cleanly with `policy_bindings: None`. This is the
        // backward-compat contract for on-disk audit logs.
        // Round-trip an entry serialized BEFORE adding the field by stripping
        // policy_bindings from the serialized form, then deserializing back.
        let entry = SignalLogEntry {
            id: SignalId::from_signal(&NodeId::new("player"), 1, "actions", &test_signal("attack")),
            signal: test_signal("attack"),
            origin: NodeId::new("player"),
            turn: 1,
            channel: "actions".to_string(),
            timestamp: 1714000000000,
            verdict: Verdict::Accept,
            policy_bindings: None,
        };
        let mut as_value = serde_json::to_value(&entry).expect("serialize");
        // Simulate an on-disk entry from before the field existed.
        if let Some(map) = as_value.as_object_mut() {
            map.remove("policy_bindings");
        }
        assert!(as_value.get("policy_bindings").is_none());

        let back: SignalLogEntry =
            serde_json::from_value(as_value).expect("legacy entry must deserialize");
        assert_eq!(back.policy_bindings, None);
        assert_eq!(back.verdict, Verdict::Accept);
    }

    #[test]
    fn fsync_policy_default() {
        assert_eq!(FsyncPolicy::default(), FsyncPolicy::EveryNSteps(100));
    }
}
