use indexmap::IndexMap;
use mcl_registry::{RegistryHandle, SignalLogEntry, Verdict};
use mcl_spec::document::KnowledgeEntry;
use mcl_spec::label::LabelSet;

use crate::state::WorldState;

// ---------------------------------------------------------------------------
// Scope enum — visibility boundary for registry-backed filtering
// ---------------------------------------------------------------------------

/// Visibility scope for registry signal entries.
///
/// Scopes determine which signals are visible to a given consumer.
/// The arbiter tags each signal with a scope on ingest; the filter
/// enforces scope on read.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Scope {
    /// Visible to all consumers.
    Global { labels: LabelSet },
    /// Visible only within a specific context (e.g. room, scene).
    Context(String, LabelSet),
    /// Visible only to the owning entity and its observers.
    Entity(String, LabelSet),
    /// Visible to the player (actions, player-facing mutations).
    Player { labels: LabelSet },
}

// ---------------------------------------------------------------------------
// Scope tagging — determine scope from signal metadata
// ---------------------------------------------------------------------------

/// Channel-name convention for scope tagging.
///
/// Channels encode scope via prefix:
/// - `"global"` or `"system"`          → `Scope::Global`
/// - `"context:{id}"`                  → `Scope::Context(id)`
/// - `"entity:{id}"`                   → `Scope::Entity(id)`
/// - `"player"` or `"actions"`         → `Scope::Player`
/// - anything else                     → `Scope::Global` (permissive default)
pub fn tag_entry_scope(entry: &SignalLogEntry) -> Scope {
    let ch = entry.channel.as_str();

    // Player-visible channels
    if ch == "player" || ch == "actions" {
        return Scope::Player {
            labels: LabelSet::empty(),
        };
    }

    // Context-scoped channels
    if let Some(id) = ch.strip_prefix("context:") {
        return Scope::Context(id.to_string(), LabelSet::empty());
    }

    // Entity-scoped channels
    if let Some(id) = ch.strip_prefix("entity:") {
        return Scope::Entity(id.to_string(), LabelSet::empty());
    }

    // Everything else is global
    Scope::Global {
        labels: LabelSet::empty(),
    }
}

// ---------------------------------------------------------------------------
// Scope arbiter — validates scope at ingest time
// ---------------------------------------------------------------------------

/// Enforces scope rules on signal entries before they are committed.
///
/// Returns [`Verdict::Accept`] if the entry's channel encodes a valid scope,
/// [`Verdict::Reject`] if the scope is malformed (e.g. `context:` with empty id).
pub fn enforce_scope(entry: &SignalLogEntry) -> Verdict {
    let ch = entry.channel.as_str();

    if ch == "context:" || ch == "entity:" {
        return Verdict::Reject {
            reason: format!("scope channel '{ch}' has empty scope id"),
        };
    }

    if let Some(id) = ch.strip_prefix("context:") {
        if id.is_empty() {
            return Verdict::Reject {
                reason: "context scope id must not be empty".to_string(),
            };
        }
    }

    if let Some(id) = ch.strip_prefix("entity:") {
        if id.is_empty() {
            return Verdict::Reject {
                reason: "entity scope id must not be empty".to_string(),
            };
        }
    }

    Verdict::Accept
}

// ---------------------------------------------------------------------------
// Registry-backed filter v2
// ---------------------------------------------------------------------------

/// Filter registry entries by scope.
///
/// Reads all entries from the registry handle, tags each with its scope
/// based on channel convention, and returns only those matching the
/// requested scope.
///
/// # Errors
///
/// Returns the underlying [`mcl_registry::SignalLogError`] if the registry
/// read fails.
pub fn filter_with_registry(
    handle: &RegistryHandle,
    scope: &Scope,
) -> Result<Vec<SignalLogEntry>, mcl_registry::SignalLogError> {
    let entries = handle.read()?;
    Ok(filter_entries_by_scope(&entries, scope))
}

/// Filter a pre-fetched slice of entries by scope.
///
/// Separated from [`filter_with_registry`] for testability without
/// needing a live registry handle.
pub fn filter_entries_by_scope(entries: &[SignalLogEntry], scope: &Scope) -> Vec<SignalLogEntry> {
    match scope {
        Scope::Global { labels: _ } => {
            // Global scope sees everything
            entries.to_vec()
        }
        Scope::Context(id, _) => entries
            .iter()
            .filter(|e| {
                let tagged = tag_entry_scope(e);
                matches!(tagged, Scope::Context(ref cid, _) if cid == id)
                    || matches!(tagged, Scope::Global { .. })
            })
            .cloned()
            .collect(),
        Scope::Entity(id, _) => entries
            .iter()
            .filter(|e| {
                let tagged = tag_entry_scope(e);
                matches!(tagged, Scope::Entity(ref eid, _) if eid == id)
                    || matches!(tagged, Scope::Global { .. })
            })
            .cloned()
            .collect(),
        Scope::Player { labels: _ } => entries
            .iter()
            .filter(|e| {
                let tagged = tag_entry_scope(e);
                matches!(tagged, Scope::Player { .. } | Scope::Global { .. })
            })
            .cloned()
            .collect(),
    }
}

/// Filter a WorldState view based on knowledge entries.
/// Known entries are included. Unknown entries are excluded.
/// Supports glob patterns: "player.*" matches all player fields.
/// If no knowledge entries are provided, returns the full state (permissive default).
pub fn filter_state_for_knowledge(state: &WorldState, knowledge: &[KnowledgeEntry]) -> WorldState {
    if knowledge.is_empty() {
        let mut full = state.clone();
        full.changelog.clear();
        return full;
    }

    let mut filtered = WorldState::new();

    // Build include/exclude lists from knowledge entries
    // Known = include pattern, Unknown = exclude pattern
    // Strategy: start with full state, remove anything matching Unknown patterns
    // If there are Known patterns, restrict to only those (whitelist mode)

    let has_known = knowledge
        .iter()
        .any(|k| matches!(k, KnowledgeEntry::Known(_)));

    if has_known {
        // Whitelist mode: only include fields matching Known patterns
        for entry in knowledge {
            if let KnowledgeEntry::Known(pattern) = entry {
                include_matching(&mut filtered, state, pattern);
            }
        }
    } else {
        // No explicit known list — start with everything
        filtered = state.clone();
        filtered.changelog.clear(); // don't leak changelog
    }

    // Remove anything matching Unknown patterns
    for entry in knowledge {
        if let KnowledgeEntry::Unknown(pattern) = entry {
            exclude_matching(&mut filtered, pattern);
        }
    }

    filtered.changelog.clear();
    filtered
}

fn matches_pattern(key: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix(".*") {
        return key == prefix || key.starts_with(&format!("{prefix}."));
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return key.starts_with(prefix);
    }
    key == pattern
}

// include_matching copies from source to dest any fields matching the pattern
fn include_matching(dest: &mut WorldState, source: &WorldState, pattern: &str) {
    for (k, v) in &source.local {
        if matches_pattern(k, pattern) {
            dest.local.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &source.player {
        let full_key = format!("player.{k}");
        if matches_pattern(&full_key, pattern) || matches_pattern(k, pattern) {
            dest.player.insert(k.clone(), v.clone());
        }
    }
    for (k, v) in &source.global {
        let full_key = format!("global.{k}");
        if matches_pattern(&full_key, pattern) || matches_pattern(k, pattern) {
            dest.global.insert(k.clone(), v.clone());
        }
    }
    for (eid, props) in &source.entities {
        if matches_pattern(eid, pattern) || matches_pattern(&format!("entities.{eid}"), pattern) {
            dest.entities.insert(eid.clone(), props.clone());
        } else {
            // Check individual entity props
            let mut matched_props = IndexMap::new();
            for (pk, pv) in props {
                let full = format!("entities.{eid}.{pk}");
                if matches_pattern(&full, pattern) {
                    matched_props.insert(pk.clone(), pv.clone());
                }
            }
            if !matched_props.is_empty() {
                dest.entities
                    .entry(eid.clone())
                    .or_default()
                    .extend(matched_props);
            }
        }
    }
}

// exclude_matching removes fields from dest matching the pattern
fn exclude_matching(state: &mut WorldState, pattern: &str) {
    state.local.retain(|k, _| !matches_pattern(k, pattern));
    state.player.retain(|k, _| {
        !matches_pattern(k, pattern) && !matches_pattern(&format!("player.{k}"), pattern)
    });
    state.global.retain(|k, _| {
        !matches_pattern(k, pattern) && !matches_pattern(&format!("global.{k}"), pattern)
    });
    state.entities.retain(|eid, _| {
        !matches_pattern(eid, pattern) && !matches_pattern(&format!("entities.{eid}"), pattern)
    });
    // Also filter individual entity props
    for (eid, props) in &mut state.entities {
        props.retain(|pk, _| !matches_pattern(&format!("entities.{eid}.{pk}"), pattern));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use mcl_graph::id::NodeId;
    use mcl_registry::{FsyncPolicy, SignalId};
    use mcl_spec::label::LabelSet;
    use mcl_spec::signal::{MclSignal, SignalKind};
    use mcl_spec::span::Span;
    use mcl_spec::value::MclValue;
    use tempfile::NamedTempFile;

    fn action_signal(name: &str) -> MclSignal {
        MclSignal {
            kind: SignalKind::Action {
                name: name.to_string(),
                args: Default::default(),
            },
            span: Span::new(0, 1, 1, 1),
            labels: LabelSet::default(),
        }
    }

    fn make_entry(channel: &str, turn: u64) -> SignalLogEntry {
        let origin = NodeId::new("player");
        let signal = action_signal("test");
        SignalLogEntry {
            id: SignalId::from_signal(&origin, turn, channel, &signal),
            signal,
            origin,
            turn,
            channel: channel.to_string(),
            timestamp: turn * 1000,
            verdict: Verdict::Accept,
            policy_bindings: None,
        }
    }

    // --- Existing prompt-level filter tests (regression) ---

    #[test]
    fn empty_knowledge_returns_full_state() {
        let mut state = WorldState::new();
        state.set("hp", MclValue::Int(100)).unwrap();
        let filtered = filter_state_for_knowledge(&state, &[]);
        assert_eq!(filtered.get_by_str("hp"), Some(&MclValue::Int(100)));
    }

    #[test]
    fn unknown_excludes_field() {
        let mut state = WorldState::new();
        state.set("visible", MclValue::Bool(true)).unwrap();
        state.set("secret", MclValue::Bool(true)).unwrap();
        let knowledge = vec![KnowledgeEntry::Unknown("secret".into())];
        let filtered = filter_state_for_knowledge(&state, &knowledge);
        assert!(filtered.get_by_str("visible").is_some());
        assert!(filtered.get_by_str("secret").is_none());
    }

    #[test]
    fn known_whitelists_fields() {
        let mut state = WorldState::new();
        state.set("allowed", MclValue::Bool(true)).unwrap();
        state.set("hidden", MclValue::Bool(true)).unwrap();
        let knowledge = vec![KnowledgeEntry::Known("allowed".into())];
        let filtered = filter_state_for_knowledge(&state, &knowledge);
        assert!(filtered.get_by_str("allowed").is_some());
        assert!(filtered.get_by_str("hidden").is_none());
    }

    #[test]
    fn glob_pattern_matches_prefix() {
        let mut state = WorldState::new();
        state.set("player.hp", MclValue::Int(100)).unwrap();
        state
            .set("player.hidden_items", MclValue::List(vec![]))
            .unwrap();
        let knowledge = vec![
            KnowledgeEntry::Known("player.*".into()),
            KnowledgeEntry::Unknown("player.hidden_items".into()),
        ];
        let filtered = filter_state_for_knowledge(&state, &knowledge);
        assert!(filtered.player.get("hp").is_some());
        assert!(filtered.player.get("hidden_items").is_none());
    }

    #[test]
    fn entity_filtering() {
        let mut state = WorldState::new();
        let mut skeleton = IndexMap::new();
        skeleton.insert("hp".to_string(), MclValue::Int(10));
        skeleton.insert(
            "secret_weakness".to_string(),
            MclValue::String("fire".into()),
        );
        state.entities.insert("skeleton".to_string(), skeleton);

        let knowledge = vec![KnowledgeEntry::Unknown(
            "entities.skeleton.secret_weakness".into(),
        )];
        let filtered = filter_state_for_knowledge(&state, &knowledge);
        let skel = filtered.entities.get("skeleton").unwrap();
        assert!(skel.get("hp").is_some());
        assert!(skel.get("secret_weakness").is_none());
    }

    #[test]
    fn changelog_is_always_cleared() {
        let mut state = WorldState::new();
        state.set("hp", MclValue::Int(100)).unwrap();
        state.set("hp", MclValue::Int(50)).unwrap();
        assert!(!state.changelog.is_empty());
        let filtered = filter_state_for_knowledge(&state, &[]);
        assert!(filtered.changelog.is_empty());
    }

    // --- Scope tagging tests ---

    #[test]
    fn tag_player_channel() {
        let entry = make_entry("player", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Player {
                labels: LabelSet::empty()
            }
        );
    }

    #[test]
    fn tag_actions_channel() {
        let entry = make_entry("actions", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Player {
                labels: LabelSet::empty()
            }
        );
    }

    #[test]
    fn tag_context_channel() {
        let entry = make_entry("context:antechamber", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Context("antechamber".to_string(), LabelSet::empty())
        );
    }

    #[test]
    fn tag_entity_channel() {
        let entry = make_entry("entity:skeleton", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Entity("skeleton".to_string(), LabelSet::empty())
        );
    }

    #[test]
    fn tag_global_channel() {
        let entry = make_entry("system", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Global {
                labels: LabelSet::empty()
            }
        );
    }

    #[test]
    fn tag_unknown_channel_is_global() {
        let entry = make_entry("custom_channel", 1);
        assert_eq!(
            tag_entry_scope(&entry),
            Scope::Global {
                labels: LabelSet::empty()
            }
        );
    }

    // --- Scope arbiter tests ---

    #[test]
    fn enforce_scope_accepts_valid_context() {
        let entry = make_entry("context:antechamber", 1);
        assert_eq!(enforce_scope(&entry), Verdict::Accept);
    }

    #[test]
    fn enforce_scope_accepts_valid_entity() {
        let entry = make_entry("entity:skeleton", 1);
        assert_eq!(enforce_scope(&entry), Verdict::Accept);
    }

    #[test]
    fn enforce_scope_rejects_empty_context_id() {
        let entry = make_entry("context:", 1);
        let v = enforce_scope(&entry);
        assert!(
            matches!(v, Verdict::Reject { ref reason } if reason.contains("context")),
            "expected Reject for empty context id, got {v:?}"
        );
    }

    #[test]
    fn enforce_scope_rejects_empty_entity_id() {
        let entry = make_entry("entity:", 1);
        let v = enforce_scope(&entry);
        assert!(
            matches!(v, Verdict::Reject { ref reason } if reason.contains("entity")),
            "expected Reject for empty entity id, got {v:?}"
        );
    }

    #[test]
    fn enforce_scope_accepts_player_channel() {
        let entry = make_entry("player", 1);
        assert_eq!(enforce_scope(&entry), Verdict::Accept);
    }

    #[test]
    fn enforce_scope_accepts_global_channel() {
        let entry = make_entry("system", 1);
        assert_eq!(enforce_scope(&entry), Verdict::Accept);
    }

    // --- Scope filtering tests (in-memory, no registry handle) ---

    fn seed_entries() -> Vec<SignalLogEntry> {
        vec![
            make_entry("actions", 1),
            make_entry("context:antechamber", 2),
            make_entry("context:inner_crypt", 3),
            make_entry("entity:skeleton", 4),
            make_entry("entity:ghost", 5),
            make_entry("system", 6),
            make_entry("player", 7),
        ]
    }

    #[test]
    fn global_scope_sees_all() {
        let entries = seed_entries();
        let filtered = filter_entries_by_scope(
            &entries,
            &Scope::Global {
                labels: LabelSet::empty(),
            },
        );
        assert_eq!(filtered.len(), 7);
    }

    #[test]
    fn context_scope_sees_own_and_global() {
        let entries = seed_entries();
        let filtered = filter_entries_by_scope(
            &entries,
            &Scope::Context("antechamber".to_string(), LabelSet::empty()),
        );
        let channels: Vec<&str> = filtered.iter().map(|e| e.channel.as_str()).collect();
        assert!(
            channels.contains(&"context:antechamber"),
            "should contain own context"
        );
        assert!(
            channels.contains(&"system"),
            "should contain global entries"
        );
        assert!(
            !channels.contains(&"context:inner_crypt"),
            "should not contain other contexts"
        );
        assert!(
            !channels.contains(&"actions"),
            "should not contain player entries"
        );
    }

    #[test]
    fn entity_scope_sees_own_and_global() {
        let entries = seed_entries();
        let filtered = filter_entries_by_scope(
            &entries,
            &Scope::Entity("skeleton".to_string(), LabelSet::empty()),
        );
        let channels: Vec<&str> = filtered.iter().map(|e| e.channel.as_str()).collect();
        assert!(
            channels.contains(&"entity:skeleton"),
            "should contain own entity"
        );
        assert!(
            channels.contains(&"system"),
            "should contain global entries"
        );
        assert!(
            !channels.contains(&"entity:ghost"),
            "should not contain other entities"
        );
    }

    #[test]
    fn player_scope_sees_player_and_global() {
        let entries = seed_entries();
        let filtered = filter_entries_by_scope(
            &entries,
            &Scope::Player {
                labels: LabelSet::empty(),
            },
        );
        let channels: Vec<&str> = filtered.iter().map(|e| e.channel.as_str()).collect();
        assert!(
            channels.contains(&"actions"),
            "should contain actions channel"
        );
        assert!(
            channels.contains(&"player"),
            "should contain player channel"
        );
        assert!(
            channels.contains(&"system"),
            "should contain global entries"
        );
        assert!(
            !channels.contains(&"context:antechamber"),
            "should not contain context entries"
        );
        assert!(
            !channels.contains(&"entity:skeleton"),
            "should not contain entity entries"
        );
    }

    #[test]
    fn empty_entries_returns_empty() {
        let filtered = filter_entries_by_scope(
            &[],
            &Scope::Global {
                labels: LabelSet::empty(),
            },
        );
        assert!(filtered.is_empty());
    }

    // --- Registry handle integration test ---

    #[test]
    fn filter_with_registry_handle() {
        let tmp = NamedTempFile::new().expect("temp file");
        let path = tmp.path().to_path_buf();
        let handle = RegistryHandle::new(&path, FsyncPolicy::OnShutdown).expect("create handle");

        handle
            .append(
                action_signal("attack"),
                NodeId::new("player"),
                1,
                "actions".to_string(),
                Verdict::Accept,
            )
            .expect("append player");
        handle
            .append(
                action_signal("patrol"),
                NodeId::new("guard"),
                2,
                "entity:skeleton".to_string(),
                Verdict::Accept,
            )
            .expect("append entity");
        handle
            .append(
                action_signal("tick"),
                NodeId::new("engine"),
                3,
                "system".to_string(),
                Verdict::Accept,
            )
            .expect("append global");

        let player_entries = filter_with_registry(
            &handle,
            &Scope::Player {
                labels: LabelSet::empty(),
            },
        )
        .expect("filter player");
        assert_eq!(player_entries.len(), 2, "player sees actions + system");
        let channels: Vec<&str> = player_entries.iter().map(|e| e.channel.as_str()).collect();
        assert!(channels.contains(&"actions"));
        assert!(channels.contains(&"system"));

        let entity_entries = filter_with_registry(
            &handle,
            &Scope::Entity("skeleton".to_string(), LabelSet::empty()),
        )
        .expect("filter entity");
        assert_eq!(entity_entries.len(), 2, "entity:skeleton sees own + system");

        let global_entries = filter_with_registry(
            &handle,
            &Scope::Global {
                labels: LabelSet::empty(),
            },
        )
        .expect("filter global");
        assert_eq!(global_entries.len(), 3, "global sees everything");
    }

    #[test]
    fn scope_with_empty_labels_backward_compat() {
        let entries = seed_entries();

        let global_result = filter_entries_by_scope(
            &entries,
            &Scope::Global {
                labels: LabelSet::empty(),
            },
        );
        assert_eq!(
            global_result.len(),
            7,
            "Global scope should see all entries"
        );

        let context_result = filter_entries_by_scope(
            &entries,
            &Scope::Context("antechamber".to_string(), LabelSet::empty()),
        );
        let context_channels: Vec<&str> =
            context_result.iter().map(|e| e.channel.as_str()).collect();
        assert!(context_channels.contains(&"context:antechamber"));
        assert!(context_channels.contains(&"system"));
        assert!(!context_channels.contains(&"context:inner_crypt"));

        let entity_result = filter_entries_by_scope(
            &entries,
            &Scope::Entity("skeleton".to_string(), LabelSet::empty()),
        );
        let entity_channels: Vec<&str> = entity_result.iter().map(|e| e.channel.as_str()).collect();
        assert!(entity_channels.contains(&"entity:skeleton"));
        assert!(entity_channels.contains(&"system"));
        assert!(!entity_channels.contains(&"entity:ghost"));

        let player_result = filter_entries_by_scope(
            &entries,
            &Scope::Player {
                labels: LabelSet::empty(),
            },
        );
        let player_channels: Vec<&str> = player_result.iter().map(|e| e.channel.as_str()).collect();
        assert!(player_channels.contains(&"actions"));
        assert!(player_channels.contains(&"player"));
        assert!(player_channels.contains(&"system"));
        assert!(!player_channels.contains(&"context:antechamber"));
    }
}
